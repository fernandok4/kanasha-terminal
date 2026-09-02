import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { TerminalPanel } from "./TerminalPanel";

const mocks = vi.hoisted(() => ({
  focus: vi.fn(),
  fit: vi.fn(),
  open: vi.fn(),
  onData: undefined as undefined | ((input: string) => void),
  invoke: vi.fn(),
  openUrl: vi.fn().mockResolvedValue(undefined),
  webLinkHandler: undefined as undefined | ((event: MouseEvent, uri: string) => void),
  linkHandler: undefined as undefined | { activate: (event: MouseEvent, text: string) => void },
}));

vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => undefined) }));
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
      mocks.linkHandler = options.linkHandler;
    }
    loadAddon = vi.fn();
    open = mocks.open;
    focus = mocks.focus;
    write = vi.fn();
    dispose = vi.fn();
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
    mocks.invoke.mockReset();
    mocks.openUrl.mockReset().mockResolvedValue(undefined);
    mocks.onData = undefined;
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
});
