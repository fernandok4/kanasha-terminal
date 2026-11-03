import { FitAddon } from "@xterm/addon-fit";
import { WebLinksAddon } from "@xterm/addon-web-links";
import { Terminal } from "@xterm/xterm";
import "@xterm/xterm/css/xterm.css";
import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { useEffect, useRef, useState, type CSSProperties } from "react";
import { terminalApi } from "../api";
import type { AgentStatus, TerminalExit, TerminalOutput, TerminalPanelModel } from "../types";

function openExternalTerminalLink(value: string) {
  try {
    const url = new URL(value);
    if (url.protocol !== "http:" && url.protocol !== "https:") return;
    void openUrl(url.href).catch(() => undefined);
  } catch {
    // A saída do terminal não pode abrir esquemas, caminhos ou comandos inválidos.
  }
}

type TerminalPanelProps = {
  style?: CSSProperties;
  panel: TerminalPanelModel;
  active: boolean;
  agentStatus?: AgentStatus;
  focused: boolean;
  maximized: boolean;
  onStart: (terminalId: string) => void;
  onStop: (terminalId: string) => void;
  onNewConversation?: (terminalId: string) => void;
  newConversationDisabled?: boolean;
  onClose: (terminalId: string) => void;
  onSplit: (terminalId: string, direction: "horizontal" | "vertical") => void;
  onFocus: (terminalId: string) => void;
  onToggleMaximize: (terminalId: string) => void;
  onContextMenu: (event: React.MouseEvent, terminalId: string) => void;
};

export function TerminalPanel({
  style,
  panel,
  active,
  agentStatus,
  focused,
  maximized,
  onStart,
  onStop,
  onNewConversation,
  newConversationDisabled = false,
  onClose,
  onSplit,
  onFocus,
  onToggleMaximize,
  onContextMenu,
}: TerminalPanelProps) {
  const [contextGeneration, setContextGeneration] = useState(0);
  const hostRef = useRef<HTMLDivElement>(null);
  const panelRef = useRef<HTMLElement>(null);
  const terminalRef = useRef<Terminal | null>(null);
  const pendingOutputRef = useRef<string[]>([]);

  useEffect(() => {
    let disposed = false;
    let unlistenOutput: (() => void) | undefined;
    let unlistenReset: (() => void) | undefined;
    let unlistenExit: (() => void) | undefined;

    void listen<TerminalOutput>("terminal-output", (event) => {
      if (!disposed && event.payload.terminalId === panel.id) {
        if (terminalRef.current) terminalRef.current.write(event.payload.data);
        else pendingOutputRef.current.push(event.payload.data);
      }
    }).then((unlisten) => {
      if (disposed) unlisten();
      else unlistenOutput = unlisten;
    });
    void listen<TerminalExit>("terminal-context-reset", event => {
      if (!disposed && event.payload.terminalId === panel.id) {
        pendingOutputRef.current = [];
        // A fresh PTY starts at 120x36. Recreate xterm at that size and buffer
        // replacement output until React attaches it, then fit as on first open.
        terminalRef.current = null;
        setContextGeneration(value => value + 1);
      }
    }).then(unlisten => { if (disposed) unlisten(); else unlistenReset = unlisten; });
    void listen<TerminalExit>("terminal-exit", (event) => {
      if (!disposed && event.payload.terminalId === panel.id) {
        terminalRef.current?.write("\r\n\x1b[33mProcesso encerrado.\x1b[0m\r\n");
      }
    }).then((unlisten) => {
      if (disposed) unlisten();
      else unlistenExit = unlisten;
    });

    return () => {
      disposed = true;
      pendingOutputRef.current = [];
      unlistenOutput?.();
      unlistenExit?.();
      unlistenReset?.();
    };
  }, [panel.id]);

  useEffect(() => {
    if (!active || !hostRef.current) return;

    const terminal = new Terminal({
      cursorBlink: true,
      // Must match start_pty's initial size until its queued output is parsed.
      cols: 120,
      rows: 36,
      // PTY output already contains its own CR/LF and cursor positioning.
      convertEol: false,
      fontFamily: "ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace",
      fontSize: 12,
      linkHandler: {
        activate: (event, text) => {
          event.preventDefault();
          openExternalTerminalLink(text);
        },
      },
      theme: {
        background: "#111827",
        foreground: "#e5e7eb",
        cursor: "#fbbf24",
        selectionBackground: "#374151",
      },
    });
    const fitAddon = new FitAddon();
    terminal.loadAddon(fitAddon);
    terminal.loadAddon(new WebLinksAddon((event, uri) => {
      event.preventDefault();
      openExternalTerminalLink(uri);
    }));
    terminal.open(hostRef.current);
    terminalRef.current = terminal;
    const disposeInput = terminal.onData((data) => {
      void terminalApi.writeTerminal(panel.id, data);
    });
    // The PTY starts before start_terminal returns the active-panel snapshot.
    // Replay its early output after input is connected: these bytes include mouse
    // modes and terminal queries, not just text that can safely be discarded.
    for (const data of pendingOutputRef.current) terminal.write(data);
    pendingOutputRef.current = [];
    const resize = () => {
      if (
        !hostRef.current ||
        hostRef.current.clientWidth < 20 ||
        hostRef.current.clientHeight < 20
      ) {
        return;
      }
      fitAddon.fit();
      if (terminal.cols >= 2 && terminal.rows >= 2) {
        void terminalApi.resizeTerminal(panel.id, terminal.cols, terminal.rows);
      }
    };
    const observer = new ResizeObserver(resize);
    let disposed = false;
    // write() parses asynchronously. Fitting before this barrier would interpret
    // the initial screen's cursor commands using a different width from the PTY.
    terminal.write("", () => {
      if (disposed || !hostRef.current) return;
      observer.observe(hostRef.current);
      resize();
    });
    terminal.focus();

    return () => {
      disposed = true;
      observer.disconnect();
      disposeInput.dispose();
      terminalRef.current = null;
      terminal.dispose();
    };
  }, [active, panel.id, contextGeneration]);

  useEffect(() => {
    if (!focused) return;
    if (active) {
      terminalRef.current?.focus();
    } else {
      panelRef.current?.focus();
    }
  }, [active, focused]);

  const labelTone = panel.label
    ? ["amber", "blue", "violet", "emerald"][
        [...panel.label].reduce((sum, character) => sum + character.charCodeAt(0), 0) % 4
      ]
    : null;
  const effectiveAgentStatus = active ? (agentStatus ?? "idle") : "offline";
  const agentStatusLabel = {
    offline: "Parado",
    idle: "Disponível",
    working: "Trabalhando",
    waiting: "Aguardando",
    blocked: "Bloqueado",
    done: "Concluído",
  }[effectiveAgentStatus];

  return (
    <section
      style={style}
      ref={panelRef}
      className={`terminal-panel ${focused ? "is-focused" : ""} ${maximized ? "is-maximized" : ""}`}
      aria-label={`Terminal ${panel.title}`}
      tabIndex={0}
      onFocus={() => onFocus(panel.id)}
      onMouseDown={() => onFocus(panel.id)}
      onContextMenu={(event) => onContextMenu(event, panel.id)}
    >
      <header className="terminal-header">
        <span className={`terminal-status agent-${effectiveAgentStatus}`} aria-hidden="true" />
        <strong>{panel.title}</strong>
        {panel.label ? <span className={`terminal-label ${labelTone ? `tone-${labelTone}` : ""}`}>{panel.label}</span> : null}
        <span className="terminal-profile">{panel.profileId}</span>
        <span className="terminal-state" role="status">{agentStatusLabel}</span>
        <div className="terminal-actions">
          {active ? (
            <button title="Encerrar processo" aria-label={`Encerrar ${panel.title}`} onClick={() => onStop(panel.id)}>
              ■
            </button>
          ) : (
            <button title="Iniciar terminal" aria-label={`Iniciar ${panel.title}`} onClick={() => onStart(panel.id)}>
              ▶
            </button>
          )}
          {panel.member?.manager && onNewConversation ? <button
            className="new-conversation"
            title={newConversationDisabled ? "Nova conversa — aguarde o gerente ficar disponível, sem execução ou pergunta pendente." : "Nova conversa — preserva alma, memória, arquivos, histórico e fila."}
            aria-label={`Nova conversa com ${panel.title}`}
            disabled={newConversationDisabled}
            onClick={() => onNewConversation(panel.id)}
          ><svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true" focusable="false">
            <path d="M20 11.5a8 8 0 0 1-8 8H5l-4 3v-6a8 8 0 0 1 11-12" />
            <path d="M18 2v8m-4-4h8" />
          </svg></button> : null}
          <button title="Dividir à direita" aria-label={`Dividir ${panel.title} à direita`} onClick={() => onSplit(panel.id, "vertical")}>
            ◧
          </button>
          <button title="Dividir abaixo" aria-label={`Dividir ${panel.title} abaixo`} onClick={() => onSplit(panel.id, "horizontal")}>
            ▤
          </button>
          <button title={maximized ? "Restaurar layout" : "Maximizar painel"} aria-label={maximized ? `Restaurar layout de ${panel.title}` : `Maximizar ${panel.title}`} aria-pressed={maximized} onClick={() => onToggleMaximize(panel.id)}>
            {maximized ? "⤡" : "⤢"}
          </button>
          <button title="Fechar painel" aria-label={`Fechar ${panel.title}`} onClick={() => onClose(panel.id)}>
            ×
          </button>
        </div>
      </header>
      {active ? (
        <div className="terminal-canvas" ref={hostRef} />
      ) : (
        <div className="terminal-empty">
          <p>Terminal restaurado sem processo em execução.</p>
          <button title="Iniciar este terminal" onClick={() => onStart(panel.id)}>Iniciar</button>
        </div>
      )}
    </section>
  );
}
