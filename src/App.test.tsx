import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";

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
  TerminalPanel: ({ panel, focused, onFocus, onContextMenu }: { panel: { id: string; title: string }; focused: boolean; onFocus: (id: string) => void; onContextMenu: (event: React.MouseEvent, id: string) => void }) => <button data-testid={`terminal-panel-${panel.id}`} data-focused={focused} onClick={() => onFocus(panel.id)} onContextMenu={(event) => onContextMenu(event, panel.id)}>{panel.title}</button>,
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

  it("salva uma etiqueta curta pelo menu contextual do painel", async () => {
    const initial = {
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
    invoke.mockImplementation((command: string) => {
      if (command === "get_snapshot" || command === "set_terminal_label") return Promise.resolve(initial);
      return Promise.reject(new Error(`Comando inesperado: ${command}`));
    });
    const prompt = vi.spyOn(window, "prompt").mockReturnValue("backend");

    render(<App />);
    fireEvent.contextMenu(await screen.findByTestId("terminal-panel-panel-1"));
    fireEvent.click(await screen.findByRole("menuitem", { name: /Definir etiqueta/ }));

    await waitFor(() => expect(invoke).toHaveBeenCalledWith("set_terminal_label", {
      input: { terminalId: "panel-1", label: "backend" },
    }));
    prompt.mockRestore();
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
    fireEvent.click(await screen.findByRole("button", { name: "Ações do perfil Antigravity" }));
    fireEvent.click(await screen.findByRole("menuitem", { name: /Configurar nesta sessão/ }));
    fireEvent.change(screen.getByLabelText("Executável local"), { target: { value: "antigravity" } });
    fireEvent.click(screen.getByRole("button", { name: "Configurar" }));

    await waitFor(() => expect(invoke).toHaveBeenCalledWith("configure_custom_profile", {
      input: { profileId: "profile-1", executable: "antigravity", arguments: [] },
    }));
  });
});
