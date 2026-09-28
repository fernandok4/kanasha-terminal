import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { TerminalPanel } from "./TerminalPanel";

const mocks = vi.hoisted(() => ({
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
      mocks.terminalConstructed();
      mocks.linkHandler = options.linkHandler;
    }
    loadAddon = vi.fn();
    open = mocks.open;
    focus = mocks.focus;
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
    mocks.focus.mockReset();
    mocks.fit.mockReset();
    mocks.open.mockReset();
    mocks.write.mockReset();
    mocks.dispose.mockReset();
    mocks.terminalConstructed.mockReset();
    mocks.invoke.mockReset();
    mocks.listen.mockReset().mockImplementation((event: string, callback: (event: { payload: { terminalId: string; data: string } }) => void) => {
      if (event === "terminal-output") mocks.outputListener = callback;
      return Promise.resolve(() => undefined);
    });
    mocks.openUrl.mockReset().mockResolvedValue(undefined);
    mocks.onData = undefined;
    mocks.outputListener = undefined;
    mocks.webLinkHandler = undefined;
    mocks.linkHandler = undefined;
    vi.stubGlobal("ResizeObserver", ResizeObserverMock);
  });

  afterEach(() => vi.unstubAllGlobals());

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
    expect(mocks.write.mock.calls.map(([data]) => data)).toEqual([
      "contexto anterior\r\n",
      "contexto durante resumo\r\n",
      "contexto novo\r\n",
    ]);

    unmount();
    expect(mocks.dispose).toHaveBeenCalledOnce();
  });
});
