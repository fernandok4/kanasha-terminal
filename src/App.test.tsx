import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";
import type { AppSnapshot, LayoutNode, TerminalPanelModel } from "./types";

const { invoke, appWindow, listen } = vi.hoisted(() => ({
  invoke: vi.fn(),
  appWindow: { close: vi.fn(), onCloseRequested: vi.fn() },
  listen: vi.fn(),
}));

vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen }));
vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => appWindow,
}));
vi.mock("./components/TerminalPanel", () => ({
  TerminalPanel: ({ panel, style, focused, onNewConversation, newConversationDisabled, onFocus, onClose, onToggleMaximize, onContextMenu }: { onNewConversation?: (id: string) => void; newConversationDisabled?: boolean; panel: { id: string; title: string; label?: string | null }; style?: React.CSSProperties; onToggleMaximize: (id: string) => void; focused: boolean; onFocus: (id: string) => void; onClose: (id: string) => void; onContextMenu: (event: React.MouseEvent, id: string) => void }) => <div style={style} data-testid={`terminal-panel-${panel.id}`} data-focused={focused} data-label={panel.label ?? ""} onContextMenu={(event) => onContextMenu(event, panel.id)}><button onClick={() => onFocus(panel.id)}>{panel.title}</button>{onNewConversation ? <button disabled={newConversationDisabled} onClick={() => onNewConversation(panel.id)}>Nova conversa com {panel.title}</button> : null}<button aria-label={`Alternar maximização de ${panel.title}`} onClick={() => onToggleMaximize(panel.id)}>Ampliar</button><button aria-label={`Fechar ${panel.title}`} onClick={() => onClose(panel.id)}>×</button></div>,
}));

describe("KanashaTerminal", () => {
  afterEach(cleanup);

  beforeEach(() => {
    invoke.mockReset();
    invoke.mockResolvedValue({ areas: [], profiles: [], activeTerminalIds: [] });
    listen.mockReset().mockResolvedValue(() => undefined);
    appWindow.close.mockReset();
    appWindow.onCloseRequested.mockReset().mockResolvedValue(() => undefined);
  });

  function managedSnapshot(employeeCount: number): AppSnapshot {
    const panels: TerminalPanelModel[] = [
      { id: "manager", title: "Gerente", profileId: "codex", member: { manager: true, specialty: "Coordenação", instructions: "", threadId: null, paused: false, jobs: [] } },
      ...Array.from({ length: employeeCount }, (_, index) => ({ id: `employee-${index}`, title: `Pessoa ${index + 1}`, profileId: "codex", member: { manager: false, specialty: "Desenvolvimento", instructions: "", threadId: null, paused: false, jobs: [] } })),
    ];
    // Simulate an existing saved layout that repeatedly shrank the manager.
    const layout = panels.slice(1).reduce<LayoutNode>((first, panel, index) => ({
      kind: "split", id: `legacy-${index}`, direction: "vertical", ratio: 0.2,
      first, second: { kind: "panel", panelId: panel.id },
    }), { kind: "panel", panelId: "manager" });
    return {
      areas: [{ id: "area", name: "Área", rootPath: "/tmp", workspaces: [{ id: "team", name: "Sala", panels, layout }] }],
      profiles: [], activeTerminalIds: [],
    };
  }

  it("oculta Equipe na organização normal e preserva os terminais ao trocar de aba", async () => {
    const initial = managedSnapshot(0);
    initial.areas[0].workspaces.push({ id: "normal", kind: "normal", name: "Livre", panels: [{ id: "shell", title: "Shell", profileId: "shell" }], layout: { kind: "panel", panelId: "shell" } });
    invoke.mockResolvedValue(initial);
    render(<App />);
    const manager = await screen.findByTestId("terminal-panel-manager");
    const shell = screen.getByTestId("terminal-panel-shell");
    fireEvent.click(screen.getByRole("button", { name: "Equipe" }));
    expect(screen.getByRole("region", { name: "Equipe de Sala" })).toBeVisible();
    fireEvent.click(screen.getByRole("tab", { name: "Livre" }));
    expect(screen.queryByRole("button", { name: "Equipe" })).not.toBeInTheDocument();
    expect(screen.queryByRole("region", { name: "Equipe de Livre" })).not.toBeInTheDocument();
    expect(screen.getByTestId("terminal-panel-shell")).toBe(shell);
    expect(shell).toBeVisible();
    fireEvent.click(screen.getByRole("tab", { name: "Sala" }));
    expect(screen.getByTestId("terminal-panel-manager")).toBe(manager);
    expect(manager).toBeVisible();
  });

  it.each(["normal", "team"] as const)("permite escolher %s para o primeiro workspace da área", async workspaceKind => {
    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: "Criar primeira área" }));
    fireEvent.change(screen.getByLabelText("Nome"), { target: { value: "Projetos" } });
    fireEvent.change(screen.getByLabelText("Pasta-raiz local"), { target: { value: "/tmp/projetos" } });
    expect(screen.getByLabelText("Tipo do primeiro workspace")).toHaveValue("normal");
    fireEvent.change(screen.getByLabelText("Tipo do primeiro workspace"), { target: { value: workspaceKind } });
    fireEvent.click(screen.getByRole("button", { name: "Criar área" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("create_area", { input: { name: "Projetos", rootPath: "/tmp/projetos", workspaceKind } }));
  });

  it("reinicia a conversa pelo terminal e reaproveita o pedido após resposta incerta", async () => {
    const initial = managedSnapshot(0);
    let rejectReset!: (reason: string) => void;
    invoke.mockImplementation((command: string) => command === "reset_agent_context"
      ? new Promise((_, reject) => { rejectReset = reject; }) : Promise.resolve(initial));
    render(<App />);
    const button = await screen.findByRole("button", { name: "Nova conversa com Gerente" });
    fireEvent.click(button);
    expect(button).toBeDisabled();
    fireEvent.click(button);
    const resets = () => invoke.mock.calls.filter(([command]) => command === "reset_agent_context");
    expect(resets()).toHaveLength(1);
    const firstInput = resets()[0][1];
    expect(firstInput).toEqual({ input: { terminalId: "manager", requestId: expect.any(String) } });
    rejectReset("O Codex não confirmou a operação.");
    await screen.findByText("O Codex não confirmou a operação.");
    await waitFor(() => expect(button).toBeEnabled());
    invoke.mockResolvedValue(initial);
    fireEvent.click(button);
    await waitFor(() => expect(button).toBeEnabled());
    expect(resets()[1][1]).toEqual(firstInput);
    fireEvent.click(button);
    await waitFor(() => expect(button).toBeEnabled());
    expect(resets()[2][1].input.requestId).not.toBe(firstInput.input.requestId);
    expect(screen.queryByRole("region", { name: "Equipe de Sala" })).not.toBeInTheDocument();
  });

  it("bloqueia nova conversa no terminal enquanto o gerente tem turno ativo", async () => {
    const initial = managedSnapshot(0);
    initial.memberStates = { manager: { connected: true, turnId: "running-turn", activity: "Trabalhando", updatedAt: 0 } };
    invoke.mockResolvedValue(initial);
    render(<App />);
    expect(await screen.findByRole("button", { name: "Nova conversa com Gerente" })).toBeDisabled();
  });

  it.each([0, 1, 2, 3, 4, 5, 6, 9])("reserva metade para o gerente com %s funcionários e toda a área quando sozinho", async count => {
    invoke.mockResolvedValue(managedSnapshot(count));
    render(<App />);
    const manager = await screen.findByTestId("terminal-panel-manager");
    expect(manager).toHaveStyle({ width: count ? "calc(50% - 4.5px)" : "calc(100% + 0px)", height: "calc(100% + 0px)" });
    expect(screen.queryByRole("separator")).not.toBeInTheDocument();
    for (let index = 0; index < count; index += 1) {
      expect(screen.getByTestId(`terminal-panel-employee-${index}`)).toBeVisible();
    }
  });

  it("distribui quatro funcionários em 2×2 e restaura a grade após maximizar sem remontar os terminais", async () => {
    invoke.mockResolvedValue(managedSnapshot(4));
    render(<App />);
    const manager = await screen.findByTestId("terminal-panel-manager");
    const employees = Array.from({ length: 4 }, (_, index) => screen.getByTestId(`terminal-panel-employee-${index}`));
    employees.forEach((element, index) => {
      expect(element).toHaveStyle({
        width: "calc(25% - 6.75px)", height: "calc(50% - 4.5px)",
        left: index % 2 === 0 ? "calc(50% + 4.5px)" : "calc(75% + 6.75px)",
        top: index < 2 ? "calc(0% + 0px)" : "calc(50% + 4.5px)",
      });
    });
    fireEvent.click(screen.getByRole("button", { name: "Gerente" }));
    for (const employee of employees) {
      fireEvent.keyDown(window, { key: "ArrowRight", ctrlKey: true, altKey: true });
      expect(employee).toHaveAttribute("data-focused", "true");
    }
    fireEvent.click(screen.getByRole("button", { name: "Alternar maximização de Pessoa 1" }));
    expect(employees[0]).toHaveStyle({ width: "calc(100% + 0px)", height: "calc(100% + 0px)" });
    expect(manager).not.toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "Alternar maximização de Pessoa 1" }));
    expect(manager).toBeVisible();
    employees.forEach((element, index) => {
      expect(screen.getByTestId(`terminal-panel-employee-${index}`)).toBe(element);
      expect(element).toHaveStyle({ width: "calc(25% - 6.75px)" });
    });
    expect(invoke.mock.calls.every(([command]) => command === "get_snapshot" || command === "prepare_area")).toBe(true);
  });

  it("reorganiza a grade ao adicionar e remover pessoas preservando os painéis existentes", async () => {
    let current = managedSnapshot(3);
    invoke.mockImplementation(async () => current);
    render(<App />);
    const manager = await screen.findByTestId("terminal-panel-manager");
    const employee = screen.getByTestId("terminal-panel-employee-2");
    expect(employee).toHaveStyle({ width: "calc(50% - 4.5px)" });
    const refresh = listen.mock.calls.find(([event]) => event === "task-state-changed")![1];
    current = managedSnapshot(4);
    refresh();
    await screen.findByTestId("terminal-panel-employee-3");
    expect(screen.getByTestId("terminal-panel-employee-2")).toBe(employee);
    expect(employee).toHaveStyle({ width: "calc(25% - 6.75px)" });
    current = managedSnapshot(1);
    refresh();
    await waitFor(() => expect(screen.queryByTestId("terminal-panel-employee-2")).not.toBeInTheDocument());
    expect(screen.getByTestId("terminal-panel-manager")).toBe(manager);
    expect(manager).toHaveStyle({ width: "calc(50% - 4.5px)" });
    expect(screen.getByTestId("terminal-panel-employee-0")).toHaveStyle({ width: "calc(50% - 4.5px)", height: "calc(100% + 0px)" });
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

  function officeSnapshot(): AppSnapshot {
    const person = (id: string, title: string) => ({ id, title, profileId: "codex", member: { specialty: "Desenvolvimento", instructions: "", manager: false, threadId: `thread-${id}`, paused: false, jobs: [] } });
    return {
      areas: [
        { id: "office", name: "Produto", rootPath: "/tmp", workspaces: [
          { id: "backend", name: "Backend", panels: [person("ana", "Ana"), { id: "shell", title: "Shell", profileId: "shell" }], layout: { kind: "split", id: "split", direction: "vertical", ratio: .5, first: { kind: "panel", panelId: "ana" }, second: { kind: "panel", panelId: "shell" } } },
          { id: "quality", name: "Qualidade", panels: [person("bia", "Bia")], layout: { kind: "panel", panelId: "bia" } },
        ] },
        { id: "other", name: "Outra área", rootPath: "/tmp/other", workspaces: [{ id: "other-team", name: "Outro time", panels: [person("caio", "Caio")], layout: { kind: "panel", panelId: "caio" } }] },
      ], profiles: [], activeTerminalIds: ["ana", "shell"],
    };
  }

  it("oferece o escritório opcional por área e preserva os terminais ao entrar e sair", async () => {
    invoke.mockResolvedValue(officeSnapshot());
    render(<App />);
    const terminal = await screen.findByTestId("terminal-panel-ana");
    const toggle = screen.getByRole("button", { name: "Escritório" });
    expect(toggle).toHaveAttribute("aria-pressed", "false");
    expect(screen.queryByRole("region", { name: "Escritório de Produto" })).not.toBeInTheDocument();
    fireEvent.click(toggle);
    const office = screen.getByRole("region", { name: "Escritório de Produto" });
    expect(within(office).getByRole("region", { name: "Baia Backend" })).toBeVisible();
    expect(within(office).getByRole("region", { name: "Baia Qualidade" })).toBeVisible();
    expect(within(office).getAllByRole("button", { name: /Abrir terminal de/ })).toHaveLength(2);
    expect(within(office).queryByText("Caio")).not.toBeInTheDocument();
    expect(screen.getByTestId("terminal-panel-ana")).toBe(terminal);
    fireEvent.click(screen.getByRole("button", { name: "Voltar aos terminais" }));
    expect(screen.getByTestId("terminal-panel-ana")).toBe(terminal);
    expect(screen.getByRole("region", { name: "Terminais de Backend" })).toHaveAttribute("aria-hidden", "false");
    expect(invoke.mock.calls.some(([command]) => command === "start_terminal" || command === "stop_terminal")).toBe(false);
  });

  it.each([false, true])("abre a pessoa de outra baia no workspace correto (terminal já ativo: %s)", async active => {
    let current = officeSnapshot();
    if (active) current.activeTerminalIds.push("bia");
    invoke.mockImplementation(async (command: string) => {
      if (command === "start_terminal") current = { ...current, activeTerminalIds: [...current.activeTerminalIds, "bia"] };
      return current;
    });
    render(<App />);
    const original = await screen.findByTestId("terminal-panel-bia");
    fireEvent.click(screen.getByRole("button", { name: "Escritório" }));
    fireEvent.click(screen.getByRole("button", { name: "Abrir terminal de Bia — Qualidade" }));
    await waitFor(() => expect(screen.getByRole("tab", { name: "Qualidade" })).toHaveAttribute("aria-selected", "true"));
    expect(screen.getByTestId("terminal-panel-bia")).toBe(original);
    expect(original).toHaveAttribute("data-focused", "true");
    expect(screen.getByRole("region", { name: "Terminais de Qualidade" })).toHaveClass("is-maximized");
    expect(invoke.mock.calls.filter(([command]) => command === "start_terminal")).toEqual(active ? [] : [["start_terminal", { terminalId: "bia" }]]);
  });

  it("mantém o mapa após falha e impede aberturas duplicadas enquanto aguarda", async () => {
    let rejectStart: (reason: string) => void = () => undefined;
    invoke.mockImplementation((command: string) => command === "start_terminal" ? new Promise((_, reject) => { rejectStart = reject; }) : Promise.resolve(officeSnapshot()));
    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: "Escritório" }));
    const desk = screen.getByRole("button", { name: "Abrir terminal de Bia — Qualidade" });
    fireEvent.click(desk);
    fireEvent.click(desk);
    expect(desk).toBeDisabled();
    expect(invoke.mock.calls.filter(([command]) => command === "start_terminal")).toHaveLength(1);
    rejectStart("Codex indisponível");
    expect(await screen.findByRole("alert")).toHaveTextContent("Codex indisponível");
    expect(desk).toBeEnabled();
    expect(screen.getByRole("region", { name: "Escritório de Produto" })).toBeVisible();
    expect(screen.getByRole("tab", { name: "Backend" })).toHaveAttribute("aria-selected", "true");
  });

  it("troca o escritório junto com a área sem misturar funcionários", async () => {
    invoke.mockResolvedValue(officeSnapshot());
    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: "Escritório" }));
    fireEvent.click(screen.getByRole("button", { name: "Outra área" }));
    const office = screen.getByRole("region", { name: "Escritório de Outra área" });
    expect(within(office).getByRole("button", { name: "Abrir terminal de Caio — Outro time" })).toBeVisible();
    expect(within(office).queryByText("Ana")).not.toBeInTheDocument();
    expect(screen.getByTestId("terminal-panel-ana")).toBeInTheDocument();
  });

  it("não redireciona para uma mesa antiga se a pessoa mudar de área durante a abertura", async () => {
    let finishStart: (snapshot: AppSnapshot) => void = () => undefined;
    invoke.mockImplementation((command: string) => command === "start_terminal" ? new Promise(resolve => { finishStart = resolve; }) : Promise.resolve(officeSnapshot()));
    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: "Escritório" }));
    fireEvent.click(screen.getByRole("button", { name: "Abrir terminal de Bia — Qualidade" }));
    fireEvent.click(screen.getByRole("button", { name: "Outra área" }));
    finishStart({ ...officeSnapshot(), activeTerminalIds: ["ana", "bia", "shell"] });
    await waitFor(() => expect(screen.getByRole("region", { name: "Escritório de Outra área" })).toBeVisible());
    expect(screen.getByRole("button", { name: "Escritório" })).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByRole("tab", { name: "Outro time" })).toHaveAttribute("aria-selected", "true");
  });

  it.each(["vertical", "horizontal"] as const)("preserva o terminal ao dividir (%s) e fechar o painel vizinho", async (direction) => {
    const original = { id: "panel-1", title: "Codex original", profileId: "shell" };
    const neighbor = { id: "panel-2", title: "Vizinho", profileId: "shell" };
    const initial: AppSnapshot = {
      areas: [{ id: "area-1", name: "Área", rootPath: "/tmp", workspaces: [{ id: "workspace-1", name: "Sala", panels: [original], layout: { kind: "panel", panelId: original.id } }] }],
      profiles: [{ id: "shell", name: "Shell", builtIn: true, available: true, configured: true }],
      activeTerminalIds: [original.id],
    };
    const split: AppSnapshot = { ...initial, areas: [{ ...initial.areas[0], workspaces: [{
      ...initial.areas[0].workspaces[0], panels: [original, neighbor],
      layout: { kind: "split", id: "split-1", direction, ratio: 0.5, first: { kind: "panel", panelId: original.id }, second: { kind: "panel", panelId: neighbor.id } },
    }] }] };
    invoke.mockImplementation((command: string) => {
      if (command === "get_snapshot" || command === "remove_terminal") return Promise.resolve(initial);
      if (command === "create_terminal" || command === "start_terminal") return Promise.resolve(split);
      return Promise.reject(new Error(`Comando inesperado: ${command}`));
    });
    render(<App />);
    const element = await screen.findByTestId("terminal-panel-panel-1");
    fireEvent.contextMenu(element);
    fireEvent.click(screen.getByRole("menuitem", { name: direction === "vertical" ? /Dividir à direita/ : /Dividir abaixo/ }));
    await screen.findByTestId("terminal-panel-panel-2");
    expect(screen.getByTestId("terminal-panel-panel-1")).toBe(element);
    fireEvent.click(screen.getByRole("button", { name: "Fechar Vizinho" }));
    await waitFor(() => expect(screen.queryByTestId("terminal-panel-panel-2")).not.toBeInTheDocument());
    expect(screen.getByTestId("terminal-panel-panel-1")).toBe(element);
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

  it("mostra o resumo da tarefa sem desmontar o terminal", async () => {
    const initial: AppSnapshot = {
      areas: [{
        id: "area-1",
        name: "Área",
        rootPath: "/tmp",
        workspaces: [{
          id: "workspace-1",
          name: "Investigação",
          task: {
            status: "in_progress",
            summary: "Investigando a integração MCP.",
            steps: [
              { id: "investigar", title: "Investigar a integração", status: "done" },
              { id: "validar", title: "Validar o estado", status: "waiting" },
            ],
            current: "Validando o estado persistido.",
            next: "Executar os testes nativos.",
            updatedAt: 1_700_000_000,
            revision: 3,
          },
          panels: [
            { id: "panel-1", title: "Implementação", profileId: "codex" },
            { id: "panel-2", title: "Revisão", profileId: "codex" },
          ],
          layout: {
            kind: "split",
            id: "split-1",
            direction: "vertical",
            ratio: 0.5,
            first: { kind: "panel", panelId: "panel-1" },
            second: { kind: "panel", panelId: "panel-2" },
          },
        }],
      }],
      profiles: [{ id: "shell", name: "Shell", builtIn: true, available: true, configured: true }],
      activeTerminalIds: ["panel-1", "panel-2"],
      agentStates: {
        "panel-1": { status: "working", current: "Implementando o vínculo MCP." },
        "panel-2": { status: "waiting", current: "Aguardando a implementação." },
      },
    };
    let current = initial;
    invoke.mockImplementation((command: string) => {
      if (command === "get_snapshot") return Promise.resolve(current);
      return Promise.reject(new Error(`Comando inesperado: ${command}`));
    });

    render(<App />);
    const panel = await screen.findByTestId("terminal-panel-panel-1");
    const terminalStage = panel.closest(".terminal-stage");

    fireEvent.click(screen.getByRole("button", { name: "Resumo" }));

    expect(screen.getByRole("region", { name: "Resumo da tarefa Investigação" })).toBeInTheDocument();
    expect(screen.getByText("Investigando a integração MCP.")).toBeInTheDocument();
    expect(screen.getByText("Em andamento")).toBeInTheDocument();
    const steps = screen.getByRole("list", { name: "Etapas da tarefa" });
    expect(within(steps).getAllByRole("listitem")).toHaveLength(2);
    expect(within(steps).getByText("Investigar a integração")).toBeInTheDocument();
    expect(within(steps).getByText("Concluída")).toBeInTheDocument();
    expect(within(steps).getByText("Aguardando")).toBeInTheDocument();
    expect(screen.getByRole("region", { name: "Status dos agentes" })).toBeInTheDocument();
    expect(screen.getByText("Implementando o vínculo MCP.")).toBeInTheDocument();
    expect(screen.getByText("1 trabalhando")).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "Investigação" }).parentElement).toHaveAttribute("data-agent-status", "working");
    expect(screen.getByTestId("terminal-panel-panel-1")).toBe(panel);
    expect(terminalStage).toHaveClass("is-hidden");

    current = {
      ...initial,
      areas: [{
        ...initial.areas[0],
        workspaces: [{
          ...initial.areas[0].workspaces[0],
          task: {
            ...initial.areas[0].workspaces[0].task!,
            current: "Atualização recebida do MCP.",
            steps: [
              { id: "investigar", title: "Investigar a integração", status: "done" },
              { id: "validar", title: "Validar o estado", status: "done" },
            ],
            revision: 4,
          },
        }],
      }],
      agentStates: {
        "panel-1": { status: "done", current: "Implementação concluída." },
        "panel-2": { status: "waiting", current: "Aguardando a implementação." },
      },
    };
    const taskListener = listen.mock.calls.find(([event]) => event === "task-state-changed")?.[1];
    taskListener?.({ payload: "workspace-1" });
    expect(await screen.findByText("Atualização recebida do MCP.")).toBeInTheDocument();
    expect(within(screen.getByRole("list", { name: "Etapas da tarefa" })).getAllByText("Concluída")).toHaveLength(2);
    expect(screen.getByText("Implementação concluída.")).toBeInTheDocument();
    expect(screen.getByText("0 trabalhando")).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "Investigação" }).parentElement).toHaveAttribute("data-agent-status", "waiting");

    fireEvent.click(screen.getByRole("button", { name: "Terminais" }));
    expect(screen.getByTestId("terminal-panel-panel-1")).toBe(panel);
    expect(terminalStage).not.toHaveClass("is-hidden");
    expect(invoke.mock.calls.some(([command]) => command === "start_terminal")).toBe(false);
    expect(invoke.mock.calls.some(([command]) => command === "stop_terminal")).toBe(false);
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

  it.each(["normal", "team"] as const)("cria um workspace %s pelo diálogo interno", async kind => {
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
          { id: "workspace-2", kind, name: "Observabilidade", panels: [], layout: null },
        ],
      }],
    };
    let current: AppSnapshot = initial;
    invoke.mockImplementation((command: string) => {
      if (command === "get_snapshot") return Promise.resolve(current);
      if (command === "prepare_area") return Promise.resolve(null);
      if (command === "create_workspace") { current = created; return Promise.resolve(created); }
      return Promise.reject(new Error(`Comando inesperado: ${command}`));
    });

    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: "Criar novo workspace" }));

    expect(screen.queryByRole("button", { name: "Equipe" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Escritório" })).not.toBeInTheDocument();
    expect(screen.getByRole("dialog", { name: "Novo workspace" })).toBeInTheDocument();
    const name = screen.getByLabelText("Nome do workspace");
    expect(name).toHaveValue("Workspace novo");
    fireEvent.change(name, { target: { value: "Observabilidade" } });
    expect(screen.getByLabelText("Tipo de workspace")).toHaveValue("normal");
    fireEvent.change(screen.getByLabelText("Tipo de workspace"), { target: { value: kind } });
    fireEvent.click(screen.getByRole("button", { name: "Criar workspace" }));

    await waitFor(() => expect(invoke).toHaveBeenCalledWith("create_workspace", {
      input: { areaId: "area-1", name: "Observabilidade", kind },
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
