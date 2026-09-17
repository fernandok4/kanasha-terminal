import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";
import type { AppSnapshot } from "./types";

const { invoke, appWindow } = vi.hoisted(() => ({
  invoke: vi.fn(),
  appWindow: { close: vi.fn(), onCloseRequested: vi.fn() },
}));

vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => undefined) }));
vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => appWindow,
}));
vi.mock("./components/TerminalPanel", () => ({
  TerminalPanel: ({ panel, focused, onFocus, onClose, onContextMenu }: { panel: { id: string; title: string; label?: string | null }; focused: boolean; onFocus: (id: string) => void; onClose: (id: string) => void; onContextMenu: (event: React.MouseEvent, id: string) => void }) => <div data-testid={`terminal-panel-${panel.id}`} data-focused={focused} data-label={panel.label ?? ""} onContextMenu={(event) => onContextMenu(event, panel.id)}><button onClick={() => onFocus(panel.id)}>{panel.title}</button><button aria-label={`Fechar ${panel.title}`} onClick={() => onClose(panel.id)}>×</button></div>,
}));

describe("KanashaTerminal", () => {
  afterEach(cleanup);

  beforeEach(() => {
    invoke.mockReset();
    invoke.mockResolvedValue({ areas: [], profiles: [], activeTerminalIds: [] });
    appWindow.close.mockReset();
    appWindow.onCloseRequested.mockReset().mockResolvedValue(() => undefined);
  });

  it("orienta a criação da primeira área quando não existe configuração", async () => {
    render(<App />);

    expect(
      await screen.findByRole("heading", {
        name: "Organize seus agentes por área e workspace.",
      }),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Criar primeira área" })).toBeEnabled();
  });

  it("abre a criação de área pelo atalho local", async () => {
    render(<App />);
    await screen.findByRole("heading", {
      name: "Organize seus agentes por área e workspace.",
    });

    fireEvent.keyDown(window, { key: "a", ctrlKey: true, shiftKey: true });

    expect(screen.getByRole("heading", { name: "Nova área de trabalho" })).toBeInTheDocument();
  });

  it("cria e inicia o PTY ao abrir um terminal", async () => {
    const initial = {
      areas: [
        {
          id: "area-1",
          name: "Área",
          rootPath: "/tmp",
          workspaces: [{ id: "workspace-1", name: "Workspace 1", panels: [], layout: null }],
        },
      ],
      profiles: [
        { id: "shell", name: "Shell", builtIn: true, available: true, configured: true },
      ],
      activeTerminalIds: [],
    };
    const created = {
      ...initial,
      areas: [
        {
          ...initial.areas[0],
          workspaces: [
            {
              ...initial.areas[0].workspaces[0],
              panels: [{ id: "panel-1", title: "Shell", profileId: "shell" }],
              layout: { kind: "panel", panelId: "panel-1" },
            },
          ],
        },
      ],
    };
    const started = { ...created, activeTerminalIds: ["panel-1"] };
    invoke.mockImplementation((command: string) => {
      if (command === "get_snapshot") return Promise.resolve(initial);
      if (command === "create_terminal") return Promise.resolve(created);
      if (command === "start_terminal") return Promise.resolve(started);
      return Promise.reject(new Error(`Comando inesperado: ${command}`));
    });

    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: /Terminal/ }));

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("create_terminal", {
        input: { workspaceId: "workspace-1", profileId: "shell" },
      }),
    );
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("start_terminal", { terminalId: "panel-1" }),
    );
  });

  it("pede confirmação ao fechar a janela com processo ativo", async () => {
    const active = {
      areas: [
        {
          id: "area-1",
          name: "Área",
          rootPath: "/tmp",
          workspaces: [
            {
              id: "workspace-1",
              name: "Workspace 1",
              panels: [{ id: "panel-1", title: "Shell", profileId: "shell" }],
              layout: { kind: "panel", panelId: "panel-1" },
            },
          ],
        },
      ],
      profiles: [
        { id: "shell", name: "Shell", builtIn: true, available: true, configured: true },
      ],
      activeTerminalIds: ["panel-1"],
    };
    invoke.mockImplementation((command: string) => {
      if (command === "get_snapshot") return Promise.resolve(active);
      if (command === "stop_terminal") {
        return Promise.resolve({ ...active, activeTerminalIds: [] });
      }
      return Promise.reject(new Error(`Comando inesperado: ${command}`));
    });
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(true);

    render(<App />);
    await waitFor(() =>
      expect(appWindow.onCloseRequested.mock.calls.length).toBeGreaterThanOrEqual(2),
    );
    const closeCalls = appWindow.onCloseRequested.mock.calls;
    const callback = closeCalls[closeCalls.length - 1][0];
    const event = { preventDefault: vi.fn() };
    await callback(event);

    expect(event.preventDefault).toHaveBeenCalledOnce();
    expect(invoke).toHaveBeenCalledWith("stop_terminal", { terminalId: "panel-1" });
    expect(appWindow.close).toHaveBeenCalledOnce();
    confirm.mockRestore();
  });

  it("persiste o ajuste acessível de um divisor sem abrir canal de terminal", async () => {
    const initial = {
      areas: [
        {
          id: "area-1",
          name: "Área",
          rootPath: "/tmp",
          workspaces: [
            {
              id: "workspace-1",
              name: "Workspace 1",
              panels: [
                { id: "panel-1", title: "Shell 1", profileId: "shell" },
                { id: "panel-2", title: "Shell 2", profileId: "shell" },
              ],
              layout: {
                kind: "split",
                id: "split-1",
                direction: "vertical",
                ratio: 0.5,
                first: { kind: "panel", panelId: "panel-1" },
                second: { kind: "panel", panelId: "panel-2" },
              },
            },
          ],
        },
      ],
      profiles: [
        { id: "shell", name: "Shell", builtIn: true, available: true, configured: true },
      ],
      activeTerminalIds: [],
    };
    invoke.mockImplementation((command: string) => {
      if (command === "get_snapshot" || command === "set_split_ratio") return Promise.resolve(initial);
      return Promise.reject(new Error(`Comando inesperado: ${command}`));
    });

    render(<App />);
    const divider = await screen.findByRole("separator", { name: "Ajustar divisão vertical" });
    fireEvent.keyDown(divider, { key: "ArrowRight" });

    await waitFor(() => expect(invoke).toHaveBeenCalledWith("set_split_ratio", {
      input: { workspaceId: "workspace-1", splitId: "split-1", ratio: 0.55 },
    }));
  });

  it("navega entre painéis pelo atalho mesmo quando não há processo ativo", async () => {
    const initial = {
      areas: [
        {
          id: "area-1",
          name: "Área",
          rootPath: "/tmp",
          workspaces: [
            {
              id: "workspace-1",
              name: "Workspace 1",
              panels: [
                { id: "panel-1", title: "Primeiro", profileId: "shell" },
                { id: "panel-2", title: "Segundo", profileId: "shell" },
              ],
              layout: {
                kind: "split",
                id: "split-1",
                direction: "vertical",
                ratio: 0.5,
                first: { kind: "panel", panelId: "panel-1" },
                second: { kind: "panel", panelId: "panel-2" },
              },
            },
          ],
        },
      ],
      profiles: [
        { id: "shell", name: "Shell", builtIn: true, available: true, configured: true },
      ],
      activeTerminalIds: [],
    };
    invoke.mockResolvedValue(initial);

    render(<App />);
    const first = await screen.findByTestId("terminal-panel-panel-1");
    const second = screen.getByTestId("terminal-panel-panel-2");
    await waitFor(() => expect(first).toHaveAttribute("data-focused", "true"));
    fireEvent.keyDown(window, { key: "ArrowRight", ctrlKey: true, altKey: true });

    await waitFor(() => expect(second).toHaveAttribute("data-focused", "true"));
  });

  it("mantém o painel montado ao alternar workspaces para preservar o buffer efêmero", async () => {
    const initial: AppSnapshot = {
      areas: [
        {
          id: "area-1",
          name: "Área",
          rootPath: "/tmp",
          workspaces: [
            {
              id: "workspace-1",
              name: "Investigação",
              panels: [{ id: "panel-1", title: "Terminal 1", profileId: "shell" }],
              layout: { kind: "panel", panelId: "panel-1" },
            },
            {
              id: "workspace-2",
              name: "Implementação",
              panels: [{ id: "panel-2", title: "Terminal 2", profileId: "shell" }],
              layout: { kind: "panel", panelId: "panel-2" },
            },
          ],
        },
      ],
      profiles: [{ id: "shell", name: "Shell", builtIn: true, available: true, configured: true }],
      activeTerminalIds: ["panel-1", "panel-2"],
    };
    invoke.mockResolvedValue(initial);

    render(<App />);
    const firstPanel = await screen.findByTestId("terminal-panel-panel-1");

    fireEvent.click(screen.getByRole("tab", { name: "Implementação" }));
    await waitFor(() => expect(screen.getByRole("tab", { name: "Implementação" })).toHaveAttribute("aria-selected", "true"));
    expect(screen.getByTestId("terminal-panel-panel-1")).toBe(firstPanel);
    expect(firstPanel.parentElement).toHaveClass("is-hidden");

    fireEvent.click(screen.getByRole("tab", { name: "Investigação" }));
    await waitFor(() => expect(screen.getByRole("tab", { name: "Investigação" })).toHaveAttribute("aria-selected", "true"));
    expect(screen.getByTestId("terminal-panel-panel-1")).toBe(firstPanel);
    expect(firstPanel.parentElement).not.toHaveClass("is-hidden");
  });

  it("não reposiciona o contêiner do terminal ao mover o workspace", async () => {
    const initial: AppSnapshot = {
      areas: [{
        id: "area-1",
        name: "Área",
        rootPath: "/tmp",
        workspaces: [
          {
            id: "workspace-1",
            name: "Investigação",
            panels: [{ id: "panel-1", title: "Terminal 1", profileId: "shell" }],
            layout: { kind: "panel", panelId: "panel-1" },
          },
          {
            id: "workspace-2",
            name: "Implementação",
            panels: [{ id: "panel-2", title: "Terminal 2", profileId: "shell" }],
            layout: { kind: "panel", panelId: "panel-2" },
          },
        ],
      }],
      profiles: [{ id: "shell", name: "Shell", builtIn: true, available: true, configured: true }],
      activeTerminalIds: ["panel-1", "panel-2"],
    };
    const moved: AppSnapshot = {
      ...initial,
      areas: [{
        ...initial.areas[0],
        workspaces: [...initial.areas[0].workspaces].reverse(),
      }],
    };
    invoke.mockImplementation((command: string) => {
      if (command === "get_snapshot") return Promise.resolve(initial);
      if (command === "move_workspace") return Promise.resolve(moved);
      return Promise.reject(new Error(`Comando inesperado: ${command}`));
    });

    render(<App />);
    const firstPanel = await screen.findByTestId("terminal-panel-panel-1");
    const secondPanel = screen.getByTestId("terminal-panel-panel-2");
    const firstLayout = firstPanel.parentElement;
    const secondLayout = secondPanel.parentElement;
    expect(document.querySelectorAll(".workspace-terminal-layout")[0]).toBe(firstLayout);

    fireEvent.click(screen.getByRole("button", { name: "Ações do workspace Investigação" }));
    fireEvent.click(screen.getByRole("menuitem", { name: /Mover para direita/ }));

    await waitFor(() => expect(invoke).toHaveBeenCalledWith("move_workspace", {
      areaId: "area-1",
      workspaceId: "workspace-1",
      direction: "later",
    }));
    expect(screen.getByTestId("terminal-panel-panel-1")).toBe(firstPanel);
    expect(screen.getByTestId("terminal-panel-panel-2")).toBe(secondPanel);
    expect(document.querySelectorAll(".workspace-terminal-layout")[0]).toBe(firstLayout);
    expect(document.querySelectorAll(".workspace-terminal-layout")[1]).toBe(secondLayout);
  });

  it("mantém o painel montado ao alternar áreas para preservar o buffer efêmero", async () => {
    const initial: AppSnapshot = {
      areas: [
        {
          id: "area-1",
          name: "Plataforma",
          rootPath: "/tmp/plataforma",
          workspaces: [{
            id: "workspace-1",
            name: "Investigação",
            panels: [{ id: "panel-1", title: "Terminal 1", profileId: "shell" }],
            layout: { kind: "panel", panelId: "panel-1" },
          }],
        },
        {
          id: "area-2",
          name: "Integrações",
          rootPath: "/tmp/integracoes",
          workspaces: [{
            id: "workspace-2",
            name: "Implementação",
            panels: [{ id: "panel-2", title: "Terminal 2", profileId: "shell" }],
            layout: { kind: "panel", panelId: "panel-2" },
          }],
        },
      ],
      profiles: [{ id: "shell", name: "Shell", builtIn: true, available: true, configured: true }],
      activeTerminalIds: ["panel-1", "panel-2"],
    };
    invoke.mockResolvedValue(initial);

    render(<App />);
    const firstPanel = await screen.findByTestId("terminal-panel-panel-1");

    fireEvent.click(screen.getByRole("button", { name: "Integrações" }));
    await waitFor(() => expect(screen.getByRole("tab", { name: "Implementação" })).toHaveAttribute("aria-selected", "true"));
    expect(screen.getByTestId("terminal-panel-panel-1")).toBe(firstPanel);
    expect(firstPanel.parentElement).toHaveClass("is-hidden");

    fireEvent.click(screen.getByRole("button", { name: "Plataforma" }));
    await waitFor(() => expect(screen.getByRole("tab", { name: "Investigação" })).toHaveAttribute("aria-selected", "true"));
    expect(screen.getByTestId("terminal-panel-panel-1")).toBe(firstPanel);
    expect(firstPanel.parentElement).not.toHaveClass("is-hidden");
  });

  it("remove um painel de quatro sem manter um ramo de split ou scroll residual", async () => {
    let current: AppSnapshot = {
      areas: [
        {
          id: "area-1",
          name: "Área",
          rootPath: "/tmp",
          workspaces: [
            {
              id: "workspace-1",
              name: "Workspace 1",
              panels: ["panel-1", "panel-2", "panel-3", "panel-4"].map((id) => ({
                id,
                title: `Terminal ${id.slice(-1)}`,
                profileId: "shell",
              })),
              layout: {
                kind: "split",
                id: "split-root",
                direction: "vertical",
                ratio: 0.5,
                first: {
                  kind: "split",
                  id: "split-left",
                  direction: "horizontal",
                  ratio: 0.5,
                  first: { kind: "panel", panelId: "panel-1" },
                  second: { kind: "panel", panelId: "panel-2" },
                },
                second: {
                  kind: "split",
                  id: "split-right",
                  direction: "horizontal",
                  ratio: 0.5,
                  first: { kind: "panel", panelId: "panel-3" },
                  second: { kind: "panel", panelId: "panel-4" },
                },
              },
            },
          ],
        },
      ],
      profiles: [{ id: "shell", name: "Shell", builtIn: true, available: true, configured: true }],
      activeTerminalIds: [],
    };
    const afterRemoval: AppSnapshot = {
      ...current,
      areas: [{
        ...current.areas[0],
        workspaces: [{
          ...current.areas[0].workspaces[0],
          panels: current.areas[0].workspaces[0].panels.filter((panel) => panel.id !== "panel-4"),
          layout: {
            kind: "split",
            id: "split-root",
            direction: "vertical",
            ratio: 0.5,
            first: current.areas[0].workspaces[0].layout!.kind === "split"
              ? current.areas[0].workspaces[0].layout!.first
              : { kind: "panel", panelId: "panel-1" },
            second: { kind: "panel", panelId: "panel-3" },
          },
        }],
      }],
    };
    invoke.mockImplementation((command: string) => {
      if (command === "get_snapshot") return Promise.resolve(current);
      if (command === "remove_terminal") {
        current = afterRemoval;
        return Promise.resolve(current);
      }
      return Promise.reject(new Error(`Comando inesperado: ${command}`));
    });

    render(<App />);
    expect(await screen.findAllByTestId(/terminal-panel-/)).toHaveLength(4);
    expect(screen.getAllByRole("separator")).toHaveLength(3);

    fireEvent.click(screen.getByRole("button", { name: "Fechar Terminal 4" }));

    await waitFor(() => expect(invoke).toHaveBeenCalledWith("remove_terminal", { terminalId: "panel-4" }));
    expect(await screen.findAllByTestId(/terminal-panel-/)).toHaveLength(3);
    expect(screen.queryByTestId("terminal-panel-panel-4")).not.toBeInTheDocument();
    expect(screen.getAllByRole("separator")).toHaveLength(2);
  });

  it("cria um workspace pelo diálogo interno", async () => {
    const initial = {
      areas: [
        {
          id: "area-1",
          name: "Área",
          rootPath: "/tmp",
          workspaces: [{ id: "workspace-1", name: "Workspace 1", panels: [], layout: null }],
        },
      ],
      profiles: [{ id: "shell", name: "Shell", builtIn: true, available: true, configured: true }],
      activeTerminalIds: [],
    };
    const created = {
      ...initial,
      areas: [{
        ...initial.areas[0],
        workspaces: [
          ...initial.areas[0].workspaces,
          { id: "workspace-2", name: "Observabilidade", panels: [], layout: null },
        ],
      }],
    };
    invoke.mockImplementation((command: string) => {
      if (command === "get_snapshot") return Promise.resolve(initial);
      if (command === "create_workspace") return Promise.resolve(created);
      return Promise.reject(new Error(`Comando inesperado: ${command}`));
    });

    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: "Criar novo workspace" }));

    expect(screen.getByRole("dialog", { name: "Novo workspace" })).toBeInTheDocument();
    const name = screen.getByLabelText("Nome do workspace");
    expect(name).toHaveValue("Workspace novo");
    fireEvent.change(name, { target: { value: "Observabilidade" } });
    fireEvent.click(screen.getByRole("button", { name: "Criar workspace" }));

    await waitFor(() => expect(invoke).toHaveBeenCalledWith("create_workspace", {
      input: { areaId: "area-1", name: "Observabilidade" },
    }));
    expect(screen.queryByRole("dialog", { name: "Novo workspace" })).not.toBeInTheDocument();
  });

  it("edita nomes de área, workspace e terminal pelos diálogos internos", async () => {
    let current: AppSnapshot = {
      areas: [
        {
          id: "area-1",
          name: "Área",
          rootPath: "/tmp",
          workspaces: [{ id: "workspace-1", name: "Workspace 1", panels: [{ id: "panel-1", title: "Shell", profileId: "shell" }], layout: { kind: "panel", panelId: "panel-1" } }],
        },
      ],
      profiles: [{ id: "shell", name: "Shell", builtIn: true, available: true, configured: true }],
      activeTerminalIds: [],
    };
    invoke.mockImplementation((command: string, input?: Record<string, string>) => {
      if (command === "get_snapshot") return Promise.resolve(current);
      if (command === "rename_area") {
        current = { ...current, areas: [{ ...current.areas[0], name: input?.name ?? "" }] };
      }
      if (command === "rename_workspace") {
        current = {
          ...current,
          areas: [{
            ...current.areas[0],
            workspaces: [{ ...current.areas[0].workspaces[0], name: input?.name ?? "" }],
          }],
        };
      }
      if (command === "rename_terminal") {
        current = {
          ...current,
          areas: [{
            ...current.areas[0],
            workspaces: [{
              ...current.areas[0].workspaces[0],
              panels: [{ ...current.areas[0].workspaces[0].panels[0], title: input?.name ?? "" }],
            }],
          }],
        };
      }
      if (["rename_area", "rename_workspace", "rename_terminal"].includes(command)) {
        return Promise.resolve(current);
      }
      return Promise.reject(new Error(`Comando inesperado: ${command}`));
    });

    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: "Ações da área Área" }));
    fireEvent.click(screen.getByRole("menuitem", { name: /^Renomear área/ }));
    fireEvent.change(screen.getByLabelText("Nome"), { target: { value: "Plataforma" } });
    fireEvent.click(screen.getByRole("button", { name: "Salvar nome" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("rename_area", {
      areaId: "area-1", name: "Plataforma",
    }));

    fireEvent.click(await screen.findByRole("button", { name: "Ações do workspace Workspace 1" }));
    fireEvent.click(screen.getByRole("menuitem", { name: /^Renomear/ }));
    expect(screen.getByRole("dialog", { name: "Renomear workspace" })).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("Nome"), { target: { value: "Revisão" } });
    fireEvent.click(screen.getByRole("button", { name: "Salvar nome" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("rename_workspace", {
      workspaceId: "workspace-1", name: "Revisão",
    }));

    fireEvent.contextMenu(await screen.findByTestId("terminal-panel-panel-1"));
    fireEvent.click(screen.getByRole("menuitem", { name: /^Renomear terminal/ }));
    expect(screen.getByRole("dialog", { name: "Renomear terminal" })).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("Nome"), { target: { value: "Codex" } });
    fireEvent.click(screen.getByRole("button", { name: "Salvar nome" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("rename_terminal", {
      terminalId: "panel-1", name: "Codex",
    }));

    await waitFor(() => {
      expect(screen.getByRole("button", { name: "Plataforma" })).toBeInTheDocument();
      expect(screen.getByRole("tab", { name: "Revisão" })).toBeInTheDocument();
      expect(screen.getByTestId("terminal-panel-panel-1")).toHaveTextContent("Codex");
    });
    cleanup();
    render(<App />);
    expect(await screen.findByRole("button", { name: "Plataforma" })).toBeInTheDocument();
    expect(await screen.findByRole("tab", { name: "Revisão" })).toBeInTheDocument();
    expect(screen.getByTestId("terminal-panel-panel-1")).toHaveTextContent("Codex");
  });

  it("define, altera e remove a etiqueta pelo diálogo interno", async () => {
    let current: AppSnapshot = {
      areas: [
        {
          id: "area-1",
          name: "Área",
          rootPath: "/tmp",
          workspaces: [{ id: "workspace-1", name: "Workspace 1", panels: [{ id: "panel-1", title: "Shell", profileId: "shell" }], layout: { kind: "panel", panelId: "panel-1" } }],
        },
      ],
      profiles: [{ id: "shell", name: "Shell", builtIn: true, available: true, configured: true }],
      activeTerminalIds: [],
    };
    invoke.mockImplementation((command: string, input?: { input?: { label: string | null } }) => {
      if (command === "get_snapshot") return Promise.resolve(current);
      if (command === "set_terminal_label") {
        current = {
          ...current,
          areas: [{
            ...current.areas[0],
            workspaces: [{
              ...current.areas[0].workspaces[0],
              panels: [{ ...current.areas[0].workspaces[0].panels[0], label: input?.input?.label ?? null }],
            }],
          }],
        };
        return Promise.resolve(current);
      }
      return Promise.reject(new Error(`Comando inesperado: ${command}`));
    });

    render(<App />);
    fireEvent.contextMenu(await screen.findByTestId("terminal-panel-panel-1"));
    fireEvent.click(screen.getByRole("menuitem", { name: /^Definir etiqueta/ }));
    const label = screen.getByLabelText("Etiqueta");
    expect(label).toHaveFocus();
    fireEvent.change(label, { target: { value: "backend" } });
    fireEvent.click(screen.getByRole("button", { name: "Salvar etiqueta" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("set_terminal_label", {
      input: { terminalId: "panel-1", label: "backend" },
    }));
    await waitFor(() => expect(screen.getByTestId("terminal-panel-panel-1")).toHaveAttribute("data-label", "backend"));
    cleanup();
    render(<App />);
    expect(await screen.findByTestId("terminal-panel-panel-1")).toHaveAttribute("data-label", "backend");

    fireEvent.contextMenu(screen.getByTestId("terminal-panel-panel-1"));
    fireEvent.click(screen.getByRole("menuitem", { name: /^Alterar etiqueta/ }));
    expect(screen.getByLabelText("Etiqueta")).toHaveValue("backend");
    fireEvent.change(screen.getByLabelText("Etiqueta"), { target: { value: "deploy" } });
    fireEvent.click(screen.getByRole("button", { name: "Salvar etiqueta" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("set_terminal_label", {
      input: { terminalId: "panel-1", label: "deploy" },
    }));
    await waitFor(() => expect(screen.getByTestId("terminal-panel-panel-1")).toHaveAttribute("data-label", "deploy"));
    cleanup();
    render(<App />);
    expect(await screen.findByTestId("terminal-panel-panel-1")).toHaveAttribute("data-label", "deploy");

    fireEvent.contextMenu(screen.getByTestId("terminal-panel-panel-1"));
    fireEvent.click(screen.getByRole("menuitem", { name: /^Alterar etiqueta/ }));
    expect(screen.getByLabelText("Etiqueta")).toHaveValue("deploy");
    fireEvent.change(screen.getByLabelText("Etiqueta"), { target: { value: "" } });
    fireEvent.click(screen.getByRole("button", { name: "Salvar etiqueta" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("set_terminal_label", {
      input: { terminalId: "panel-1", label: null },
    }));
    await waitFor(() => expect(screen.getByTestId("terminal-panel-panel-1")).toHaveAttribute("data-label", ""));
    cleanup();
    render(<App />);
    expect(await screen.findByTestId("terminal-panel-panel-1")).toHaveAttribute("data-label", "");
  });

  it("valida nome vazio e permite cancelar sem chamar IPC de edição", async () => {
    const initial = {
      areas: [
        {
          id: "area-1",
          name: "Área",
          rootPath: "/tmp",
          workspaces: [{ id: "workspace-1", name: "Workspace 1", panels: [], layout: null }],
        },
      ],
      profiles: [{ id: "shell", name: "Shell", builtIn: true, available: true, configured: true }],
      activeTerminalIds: [],
    };
    invoke.mockResolvedValue(initial);

    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: "Ações da área Área" }));
    fireEvent.click(screen.getByRole("menuitem", { name: /^Renomear área/ }));
    fireEvent.change(screen.getByLabelText("Nome"), { target: { value: "   " } });
    expect(screen.getByRole("button", { name: "Salvar nome" })).toBeDisabled();
    fireEvent.submit(screen.getByRole("dialog", { name: "Renomear área de trabalho" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Informe um nome para continuar.");
    expect(invoke).not.toHaveBeenCalledWith("rename_area", expect.anything());

    fireEvent.click(screen.getByRole("button", { name: "Cancelar" }));
    expect(screen.queryByRole("dialog", { name: "Renomear área de trabalho" })).not.toBeInTheDocument();
    expect(invoke).not.toHaveBeenCalledWith("rename_area", expect.anything());
  });

  it("permite reconfigurar um perfil personalizado sem criar outro", async () => {
    const initial = {
      areas: [],
      profiles: [
        { id: "shell", name: "Shell", builtIn: true, available: true, configured: true },
        { id: "profile-1", name: "Antigravity", builtIn: false, available: false, configured: false },
      ],
      activeTerminalIds: [],
    };
    const configured = {
      ...initial,
      profiles: [
        initial.profiles[0],
        { ...initial.profiles[1], available: true, configured: true },
      ],
    };
    invoke.mockImplementation((command: string) => {
      if (command === "get_snapshot") return Promise.resolve(initial);
      if (command === "configure_custom_profile") return Promise.resolve(configured);
      return Promise.reject(new Error(`Comando inesperado: ${command}`));
    });

    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: "Gerenciar perfis" }));
    fireEvent.click(await screen.findByRole("menuitem", { name: /^Configurar Antigravity/ }));
    fireEvent.change(screen.getByLabelText("Executável local"), { target: { value: "antigravity" } });
    fireEvent.click(screen.getByRole("button", { name: "Configurar" }));

    await waitFor(() => expect(invoke).toHaveBeenCalledWith("configure_custom_profile", {
      input: { profileId: "profile-1", executable: "antigravity", arguments: [] },
    }));
  });
});
