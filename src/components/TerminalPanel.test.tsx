import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { TerminalPanel } from "./TerminalPanel";

const mocks = vi.hoisted(() => ({
  reset: vi.fn(),
  resetListener: undefined as undefined | ((event: { payload: { terminalId: string; data: string } }) => void),
  focus: vi.fn(),
  fit: vi.fn(),
  open: vi.fn(),
  write: vi.fn(),
  dispose: vi.fn(),
  terminalConstructed: vi.fn(),
  onData: undefined as undefined | ((input: string) => void),
  outputListener: undefined as undefined | ((event: { payload: { terminalId: string; data: string } }) => void),
  invoke: vi.fn(),
  listen: vi.fn(),
  openUrl: vi.fn().mockResolvedValue(undefined),
  webLinkHandler: undefined as undefined | ((event: MouseEvent, uri: string) => void),
  linkHandler: undefined as undefined | { activate: (event: MouseEvent, text: string) => void },
}));

vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: mocks.listen }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: mocks.openUrl }));
vi.mock("@xterm/addon-fit", () => ({
  FitAddon: class {
    fit = mocks.fit;
  },
}));
vi.mock("@xterm/addon-web-links", () => ({
  WebLinksAddon: class {
    constructor(handler: (event: MouseEvent, uri: string) => void) {
      mocks.webLinkHandler = handler;
    }
  },
}));
vi.mock("@xterm/xterm", () => ({
  Terminal: class {
    cols = 120;
    rows = 36;
    constructor(options: { linkHandler?: { activate: (event: MouseEvent, text: string) => void } }) {
      mocks.terminalConstructed(options);
      mocks.linkHandler = options.linkHandler;
    }
    loadAddon = vi.fn();
    open = mocks.open;
    focus = mocks.focus;
    reset = mocks.reset;
    write = mocks.write;
    dispose = mocks.dispose;
    onData(callback: (input: string) => void) {
      mocks.onData = callback;
      return { dispose: vi.fn() };
    }
  },
}));

class ResizeObserverMock {
  observe() {}
  disconnect() {}
}

describe("TerminalPanel", () => {
  beforeEach(() => {
    mocks.reset.mockReset();
    mocks.resetListener = undefined;
    mocks.focus.mockReset();
    mocks.fit.mockReset();
    mocks.open.mockReset();
    mocks.write.mockReset().mockImplementation((_data: string, callback?: () => void) => callback?.());
    mocks.dispose.mockReset();
    mocks.terminalConstructed.mockReset();
    mocks.invoke.mockReset();
    mocks.listen.mockReset().mockImplementation((event: string, callback: (event: { payload: { terminalId: string; data: string } }) => void) => {
      if (event === "terminal-output") mocks.outputListener = callback;
      if (event === "terminal-context-reset") mocks.resetListener = callback;
      return Promise.resolve(() => undefined);
    });
    mocks.openUrl.mockReset().mockResolvedValue(undefined);
    mocks.onData = undefined;
    mocks.outputListener = undefined;
    mocks.webLinkHandler = undefined;
    mocks.linkHandler = undefined;
    vi.stubGlobal("ResizeObserver", ResizeObserverMock);
  });

  afterEach(() => { cleanup(); vi.unstubAllGlobals(); });

  it("oferece nova conversa apenas para o gerente, ao lado de encerrar, respeitando o bloqueio", () => {
    const onNewConversation = vi.fn();
    const panel = { id: "manager", title: "Ana", profileId: "codex", member: { manager: true, specialty: "Coordenação", instructions: "", paused: false, threadId: null, jobs: [] } };
    const props = { panel, active: true, focused: true, maximized: false, onStart: vi.fn(), onStop: vi.fn(), onClose: vi.fn(), onSplit: vi.fn(), onFocus: vi.fn(), onToggleMaximize: vi.fn(), onContextMenu: vi.fn(), onNewConversation };
    const { rerender } = render(<TerminalPanel {...props} />);
    const button = screen.getByRole("button", { name: "Nova conversa com Ana" });
    expect(screen.getByRole("button", { name: "Encerrar Ana" }).nextElementSibling).toBe(button);
    fireEvent.click(button);
    expect(onNewConversation).toHaveBeenCalledWith("manager");
    rerender(<TerminalPanel {...props} newConversationDisabled />);
    expect(button).toBeDisabled();
    fireEvent.click(button);
    expect(onNewConversation).toHaveBeenCalledTimes(1);
    rerender(<TerminalPanel {...props} panel={{ ...panel, member: { ...panel.member, manager: false } }} />);
    expect(screen.queryByRole("button", { name: "Nova conversa com Ana" })).not.toBeInTheDocument();
    rerender(<TerminalPanel {...props} panel={{ ...panel, member: undefined }} />);
    expect(screen.queryByRole("button", { name: "Nova conversa com Ana" })).not.toBeInTheDocument();
  });

  it("descarta a tela anterior ao reconectar a conversa da mesma pessoa", async () => {
    render(<TerminalPanel panel={{ id: "member", title: "Lia", profileId: "codex" }} active focused maximized={false} onStart={vi.fn()} onStop={vi.fn()} onClose={vi.fn()} onSplit={vi.fn()} onFocus={vi.fn()} onToggleMaximize={vi.fn()} onContextMenu={vi.fn()} />);
    await waitFor(() => expect(mocks.resetListener).toBeDefined());
    act(() => mocks.resetListener!({ payload: { terminalId: "other", data: "" } }));
    expect(mocks.terminalConstructed).toHaveBeenCalledTimes(1);
    act(() => {
      mocks.outputListener!({ payload: { terminalId: "member", data: "Conversa X" } });
      mocks.resetListener!({ payload: { terminalId: "member", data: "" } });
      mocks.outputListener!({ payload: { terminalId: "member", data: "Conversa Y" } });
    });
    expect(mocks.terminalConstructed).toHaveBeenCalledTimes(2);
    expect(mocks.dispose).toHaveBeenCalledTimes(1);
    expect(mocks.write.mock.calls.some(([value]) => value === "Conversa Y")).toBe(true);
  });

  it("foca o xterm e encaminha a entrada colada para o PTY tipado", async () => {
    render(
      <TerminalPanel
        panel={{ id: "painel-1", title: "Shell", label: "backend", profileId: "shell" }}
        active
        agentStatus="working"
        focused
        maximized={false}
        onStart={vi.fn()}
        onStop={vi.fn()}
        onClose={vi.fn()}
        onSplit={vi.fn()}
        onFocus={vi.fn()}
        onToggleMaximize={mocks.focus}
        onContextMenu={vi.fn()}
      />,
    );

    await waitFor(() => expect(mocks.focus).toHaveBeenCalled());
    expect(mocks.onData).toBeTypeOf("function");
    expect(screen.getByText("backend")).toBeInTheDocument();
    expect(screen.getByRole("status")).toHaveTextContent("Trabalhando");
    expect(document.querySelector(".terminal-status")).toHaveClass("agent-working");
    fireEvent.click(screen.getByRole("button", { name: "Maximizar Shell" }));
    expect(mocks.focus).toHaveBeenCalledWith("painel-1");

    mocks.onData?.("texto-colado");

    expect(mocks.invoke).toHaveBeenCalledWith("write_terminal", {
      terminalId: "painel-1",
      input: "texto-colado",
    });
  });

  it("abre somente links HTTP(S) reconhecidos pelo terminal no navegador padrão", async () => {
    render(
      <TerminalPanel
        panel={{ id: "painel-1", title: "Shell", profileId: "shell" }}
        active
        focused={false}
        maximized={false}
        onStart={vi.fn()}
        onStop={vi.fn()}
        onClose={vi.fn()}
        onSplit={vi.fn()}
        onFocus={vi.fn()}
        onToggleMaximize={vi.fn()}
        onContextMenu={vi.fn()}
      />,
    );

    await waitFor(() => expect(mocks.webLinkHandler).toBeTypeOf("function"));
    const webLinkClick = new MouseEvent("click", { cancelable: true });
    mocks.webLinkHandler?.(webLinkClick, "https://kanasha.com.br/docs");
    expect(webLinkClick.defaultPrevented).toBe(true);
    expect(mocks.openUrl).toHaveBeenCalledWith("https://kanasha.com.br/docs");

    mocks.linkHandler?.activate(new MouseEvent("click"), "file:///tmp/nao-abrir");
    expect(mocks.openUrl).toHaveBeenCalledTimes(1);
  });

  it("preserva a instância e o histórico do xterm enquanto a tela de resumo está visível", async () => {
    const props = {
      panel: { id: "painel-1", title: "Codex", profileId: "codex" },
      active: true,
      focused: true,
      maximized: false,
      onStart: vi.fn(),
      onStop: vi.fn(),
      onClose: vi.fn(),
      onSplit: vi.fn(),
      onFocus: vi.fn(),
      onToggleMaximize: vi.fn(),
      onContextMenu: vi.fn(),
    };
    const { rerender, unmount } = render(
      <div className="terminal-stage">
        <TerminalPanel {...props} />
      </div>,
    );

    await waitFor(() => expect(mocks.outputListener).toBeTypeOf("function"));
    expect(mocks.terminalConstructed).toHaveBeenCalledOnce();
    mocks.outputListener?.({
      payload: { terminalId: "painel-1", data: "contexto anterior\r\n" },
    });
    expect(mocks.write).toHaveBeenLastCalledWith("contexto anterior\r\n");

    rerender(
      <div className="terminal-stage is-hidden">
        <TerminalPanel {...props} />
      </div>,
    );
    expect(mocks.terminalConstructed).toHaveBeenCalledOnce();
    expect(mocks.dispose).not.toHaveBeenCalled();
    mocks.outputListener?.({
      payload: { terminalId: "painel-1", data: "contexto durante resumo\r\n" },
    });
    expect(mocks.write).toHaveBeenLastCalledWith("contexto durante resumo\r\n");

    rerender(
      <div className="terminal-stage">
        <TerminalPanel {...props} />
      </div>,
    );
    mocks.outputListener?.({
      payload: { terminalId: "painel-1", data: "contexto novo\r\n" },
    });

    expect(mocks.terminalConstructed).toHaveBeenCalledOnce();
    expect(mocks.dispose).not.toHaveBeenCalled();
    expect(mocks.write.mock.calls.map(([data]) => data).filter(Boolean)).toEqual([
      "contexto anterior\r\n",
      "contexto durante resumo\r\n",
      "contexto novo\r\n",
    ]);

    unmount();
    expect(mocks.dispose).toHaveBeenCalledOnce();
  });

  it("preserva os modos de mouse emitidos antes da confirmação de início do processo", async () => {
    const props = {
      panel: { id: "painel-1", title: "Codex", profileId: "codex" },
      active: false, focused: false, maximized: false,
      onStart: vi.fn(), onStop: vi.fn(), onClose: vi.fn(), onSplit: vi.fn(),
      onFocus: vi.fn(), onToggleMaximize: vi.fn(), onContextMenu: vi.fn(),
    };
    const { rerender } = render(<TerminalPanel {...props} />);
    await waitFor(() => expect(mocks.outputListener).toBeTypeOf("function"));
    // O PTY pode emitir saída antes de start_terminal devolver activeTerminalIds.
    // A sequência pode inclusive ser dividida entre dois eventos.
    const chunks = ["\x1b[?1049h\x1b[?100", "6h\x1b[?1003h", "Conversa inicial\r\n"];
    for (const data of chunks) mocks.outputListener?.({ payload: { terminalId: "painel-1", data } });
    expect(mocks.write).not.toHaveBeenCalled();
    rerender(<TerminalPanel {...props} active />);
    expect(mocks.write.mock.calls.map(([data]) => data).join("")).toBe(chunks.join(""));
    expect(mocks.onData).toBeTypeOf("function");

    // Novos dados entram depois dos comandos iniciais, sem duplicação na troca de foco.
    mocks.outputListener?.({ payload: { terminalId: "painel-1", data: "Nova resposta" } });
    rerender(<TerminalPanel {...props} active focused />);
    expect(mocks.write.mock.calls.map(([data]) => data).join("")).toBe(chunks.join("") + "Nova resposta");
    expect(mocks.terminalConstructed).toHaveBeenCalledOnce();
  });

  it("aguarda a saída inicial ser interpretada no tamanho do PTY antes de redimensionar", async () => {
    const props = {
      panel: { id: "painel-1", title: "Codex", profileId: "codex" },
      active: false, focused: false, maximized: false,
      onStart: vi.fn(), onStop: vi.fn(), onClose: vi.fn(), onSplit: vi.fn(),
      onFocus: vi.fn(), onToggleMaximize: vi.fn(), onContextMenu: vi.fn(),
    };
    let parsed: (() => void) | undefined;
    mocks.write.mockImplementation((_data: string, callback?: () => void) => { if (callback) parsed = callback; });
    mocks.invoke.mockResolvedValue(undefined);
    const { rerender, container, unmount } = render(<TerminalPanel {...props} />);
    await waitFor(() => expect(mocks.outputListener).toBeTypeOf("function"));
    mocks.outputListener?.({ payload: { terminalId: "painel-1", data: "A".repeat(119) + "\r\nB\x1b[1A\rOK" } });
    rerender(<TerminalPanel {...props} active />);
    const host = container.querySelector(".terminal-canvas")!;
    Object.defineProperties(host, { clientWidth: { value: 500 }, clientHeight: { value: 400 } });
    expect(mocks.terminalConstructed).toHaveBeenCalledWith(expect.objectContaining({ cols: 120, rows: 36, convertEol: false }));
    expect(mocks.fit).not.toHaveBeenCalled();
    expect(mocks.invoke).not.toHaveBeenCalled();
    expect(parsed).toBeTypeOf("function");
    parsed?.();
    expect(mocks.fit).toHaveBeenCalledOnce();
    expect(mocks.invoke).toHaveBeenCalledWith("resize_terminal", { input: { terminalId: "painel-1", columns: 120, rows: 36 } });
    unmount();
    parsed?.();
    expect(mocks.fit).toHaveBeenCalledOnce();
  });
});
