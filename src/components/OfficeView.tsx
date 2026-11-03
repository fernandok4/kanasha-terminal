import { useRef, useState, type CSSProperties } from "react";
import type { AppSnapshot, Area, TerminalPanelModel } from "../types";
import { isTeamWorkspace } from "../types";
import "./OfficeView.css";

type Props = {
  area: Area;
  snapshot: AppSnapshot;
  onOpen: (workspaceId: string, terminalId: string) => Promise<void>;
  onTeam: (workspaceId: string) => void;
  onClose: () => void;
};

const COLORS = ["#a9c799", "#d8a078", "#8fbbce", "#c1a0ce", "#dfc47d"];
function colorFor(id: string) {
  return COLORS[Array.from(id).reduce((hash, letter) => (hash * 31 + letter.charCodeAt(0)) >>> 0, 0) % COLORS.length];
}

function statusFor(person: TerminalPanelModel, snapshot: AppSnapshot) {
  const runtime = snapshot.memberStates?.[person.id];
  if (person.member?.paused) return { tone: "paused", label: "Pausado" };
  if (runtime?.request) return { tone: "waiting", label: "Precisa de resposta" };
  if (!runtime?.connected && !snapshot.activeTerminalIds.includes(person.id)) return { tone: "offline", label: "Desconectado" };
  if (runtime?.turnId) return { tone: "working", label: "Trabalhando" };
  const tone = snapshot.agentStates?.[person.id]?.status ?? "idle";
  const labels = { idle: "Disponível", working: "Trabalhando", waiting: "Aguardando", blocked: "Bloqueado", done: "Concluído" };
  return { tone, label: labels[tone] };
}

// Original pixel artwork on an integer grid; no external assets or game engine.
function Desk({ color, manager }: { color: string; manager: boolean }) {
  return <svg className="office-desk-art" viewBox="0 0 96 96" aria-hidden="true" shapeRendering="crispEdges">
    <path fill="#00000028" d="M13 28h74v28H13zM33 65h36v26H33z" />
    <path fill="#664d3e" d="M9 22h78v32H9z" /><path fill="#c89d71" d="M9 18h78v29H9z" />
    <path fill="#e4bd8c" d="M9 18h78v4H9z" /><path fill="#b2875e" d="M12 43h72v4H12z" />
    <path fill="#394846" d="M30 15h35v23H30zM43 38h9v4H43z" /><path fill="#718f83" d="M33 18h29v16H33z" />
    <path className="office-monitor" fill="#bde4c5" d="M36 21h13v2H36zM36 26h22v2H36zM36 30h17v2H36z" />
    <path fill="#e2daca" d="M34 43h28v5H34zM73 30h7v9h-7z" /><path fill="#879489" d="M36 44h24v1H36zM38 46h20v1H38z" />
    <path fill="#4d655e" d="M29 63h38v21H29zM34 84h28v5H34z" /><path fill="#283634" d="M34 87h7v6h-7zM55 87h7v6h-7z" />
    <path fill={color} d="M34 58h28v21H34zM29 61h5v12h-5zM62 61h5v12h-5z" />
    <path fill="#d7a581" d="M29 57h5v7h-5zM62 57h5v7h-5zM38 44h20v17H38z" />
    <path fill="#473c39" d="M36 39h24v14H36zM40 36h16v4H40zM39 52h18v4H39z" /><path fill="#65504a" d="M39 40h16v4H39z" />
    {manager ? <path fill="#f3d484" d="M43 65h10v4H43zM46 62h4v10h-4z" /> : null}
  </svg>;
}

function Plant() {
  return <svg className="office-plant" viewBox="0 0 32 40" aria-hidden="true" shapeRendering="crispEdges">
    <path fill="#00000025" d="M7 32h22v6H7z" /><path fill="#b57c5b" d="M9 24h16v11H9z" />
    <path fill="#deb58b" d="M7 23h20v5H7z" /><path fill="#49644c" d="M12 4h9v21h-9zM2 12h29v9H2z" />
    <path fill="#7f9f67" d="M6 9h8v9H6zM15 2h5v12h-5zM20 10h8v6h-8z" />
  </svg>;
}

export function OfficeView({ area, snapshot, onOpen, onTeam, onClose }: Props) {
  const [openingId, setOpeningId] = useState<string | null>(null);
  const opening = useRef(false);
  const [error, setError] = useState<string | null>(null);
  const teams = area.workspaces.filter(isTeamWorkspace);
  const peopleCount = teams.reduce((count, workspace) => count + workspace.panels.filter(panel => panel.member).length, 0);

  async function open(workspaceId: string, terminalId: string) {
    if (opening.current) return;
    opening.current = true;
    setOpeningId(terminalId);
    setError(null);
    try {
      await onOpen(workspaceId, terminalId);
    } catch (cause) {
      setError(typeof cause === "string" ? cause : "Não foi possível abrir o terminal. Tente novamente.");
    } finally {
      opening.current = false;
      setOpeningId(null);
    }
  }

  return <section className="office-stage" aria-label={`Escritório de ${area.name}`}>
    <header className="office-heading">
      <div><p className="office-eyebrow">VISÃO DO ESCRITÓRIO</p><h1>{area.name}</h1><p>{teams.length} {teams.length === 1 ? "baia" : "baias"} · {peopleCount} {peopleCount === 1 ? "pessoa" : "pessoas"}</p></div>
      <button className="toolbar-secondary" onClick={onClose}>Voltar aos terminais</button>
    </header>
    <div className="office-hint"><span aria-hidden="true">↳</span> Clique em uma pessoa para abrir seu terminal. Cada baia representa uma equipe.</div>
    {error ? <div className="office-error" role="alert">{error}</div> : null}
    <div className="office-map-scroll" tabIndex={0} aria-label="Mapa do escritório">
      <div className="office-map">
        <div className="office-reception" aria-hidden="true"><Plant /><span>KANASHA <small>ESCRITÓRIO</small></span><div className="office-sofa" /><Plant /></div>
        <div className="office-bays">
          {teams.map((workspace, index) => {
            const people = workspace.panels.filter(panel => panel.member);
            return <section key={workspace.id} className="office-bay" style={{ "--bay-color": colorFor(workspace.id) } as CSSProperties} aria-label={`Baia ${workspace.name}`}>
              <header className="office-bay-sign"><span className="office-bay-number" aria-hidden="true">{String(index + 1).padStart(2, "0")}</span><div><h2>{workspace.name}</h2><span>{people.length} {people.length === 1 ? "pessoa" : "pessoas"}</span></div><button onClick={() => onTeam(workspace.id)} aria-label={`Ver equipe de ${workspace.name}`}>Equipe ↗</button></header>
              <div className="office-desks">
                {people.map(person => {
                  const status = statusFor(person, snapshot);
                  return <button key={person.id} className={`office-person office-status-${status.tone}`} disabled={openingId !== null} onClick={() => void open(workspace.id, person.id)} aria-label={`Abrir terminal de ${person.title} — ${workspace.name}`} title={`${person.title} · ${person.member!.specialty} · ${status.label}`}>
                    <Desk color={colorFor(person.id)} manager={person.member!.manager} />
                    <strong>{person.title}</strong><span className="office-person-role">{person.member!.manager ? "Gerente" : person.member!.specialty}</span>
                    <span className="office-person-status"><i aria-hidden="true" />{openingId === person.id ? "Abrindo…" : status.label}</span>
                  </button>;
                })}
                {!people.length ? <p className="office-empty-bay">Mesas livres.<br />Contrate funcionários na tela Equipe.</p> : null}
              </div>
              <div className="office-bay-door" aria-hidden="true"><span />ENTRADA<span /></div>
            </section>;
          })}
        </div>
        {!teams.length ? <p className="office-empty-bay">Crie um workspace do tipo Equipe para adicionar a primeira baia.</p> : null}
        <div className="office-walkway" aria-hidden="true">ÁREA COMUM <span>·</span> {area.name}</div>
      </div>
    </div>
    <footer className="office-footer">As pessoas e os status acompanham sua equipe. <span>Use Tab e Enter para abrir uma mesa.</span></footer>
  </section>;
}
