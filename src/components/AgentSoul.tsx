import { useEffect, useState } from "react";
import { terminalApi } from "../api";
import type { AgentSoul as Soul, AppSnapshot, TerminalPanelModel } from "../types";
import { TaskMarkdown } from "./TaskMarkdown";

const empty: Soul = { content: "", revision: 0, updatedAt: null, updatedBy: null, approved: null };

export function AgentSoul({ person, onSnapshot, onClose }: {
  person: TerminalPanelModel; onSnapshot: (snapshot: AppSnapshot) => void; onClose: () => void;
}) {
  const [saved, setSaved] = useState(person.member?.soul ?? empty);
  const [content, setContent] = useState(saved.content);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const dirty = content !== saved.content;
  const current = person.member?.soul ?? empty;
  const stale = current.revision > saved.revision;
  const pending = saved.approved?.revision !== saved.revision;
  const review = person.member?.training === "review";

  useEffect(() => {
    if (current.revision > saved.revision) {
      if (dirty) setError("A alma foi atualizada durante sua edição. Seu rascunho foi preservado; reabra a tela após copiar suas alterações.");
      else { setSaved(current); setContent(current.content); }
    } else if (current.revision === saved.revision && current.approved && (!saved.approved || current.approved.approvedAt > saved.approved.approvedAt)) {
      setSaved(current);
    }
  }, [current, saved.revision, saved.approved?.revision, dirty]);

  function accept(snapshot: AppSnapshot) {
    const soul = snapshot.areas.flatMap(a => a.workspaces).flatMap(w => w.panels).find(p => p.id === person.id)?.member?.soul;
    if (soul) { setSaved(soul); setContent(soul.content); }
    onSnapshot(snapshot);
  }
  async function run(work: () => Promise<void>) {
    setBusy(true); setError(""); setNotice("");
    try { await work(); } catch (e) { setError(String(e)); } finally { setBusy(false); }
  }
  function close() {
    if (!busy && (!dirty || window.confirm("Descartar as alterações ainda não salvas na alma?"))) onClose();
  }
  return <div className="modal-backdrop" onKeyDown={e => { e.stopPropagation(); if (e.key === "Escape") close(); }}>
    <section className="modal agent-memory" role="dialog" aria-modal="true" aria-labelledby="agent-soul-title">
      <h2 id="agent-soul-title">Alma de {person.title}</h2>
      <p>Quem esta pessoa é, sua missão, o que cuida, responsabilidades, valores, modo de trabalhar e critérios de decisão. Formada pelo treinamento inicial e estável entre demandas; até 12.000 caracteres.</p>
      {saved.approved ? <details open><summary>Identidade aprovada · revisão {saved.approved.revision}</summary><TaskMarkdown content={saved.approved.content} /><p>Aprovada {saved.approved.approvedBy === "user" ? "por você" : "pelo gerente"} em {new Date(saved.approved.approvedAt * 1000).toLocaleString("pt-BR")}.</p></details> : <p>Ainda sem alma aprovada. Perfil profissional e memória anteriores foram preservados; não viram identidade aprovada automaticamente.</p>}
      <label htmlFor="agent-soul-content">{pending ? "Proposta de alma em Markdown" : "Alma em Markdown"}</label>
      <textarea id="agent-soul-content" autoFocus maxLength={12000} value={content} onChange={e => { setContent(e.target.value); setNotice(""); }} placeholder="Consolide o que foi ensinado: missão, responsabilidades, valores, modo de trabalhar, critérios de decisão, limites e origem das orientações." />
      <p>Revisão {saved.revision}. Salvar uma proposta preserva a identidade aprovada até sua revisão. Aprendizados posteriores pertencem à memória; andamento e resultados ficam nas demandas.</p>
      {review ? <>
        <details><summary>Memória a revisar junto ao treinamento</summary>{person.member?.memory?.content ? <TaskMarkdown content={person.member.memory.content} /> : <p>Sem aprendizados posteriores registrados. A identidade inicial fica na alma.</p>}</details>
        <p>Revise alma e memória. Concluir o treinamento aprova esta alma e libera a pessoa para trabalhar{person.member?.manager ? " e treinar a equipe" : ""}.</p>
      </> : <p>Aprovar a alma atualiza a identidade permanente. A conclusão do treinamento exige uma conversa de formação encerrada e revisada.</p>}
      {error ? <p role="alert">{error}</p> : null}{notice ? <p role="status">{notice}</p> : null}
      <footer>
        <button disabled={busy} onClick={close}>Fechar</button>
        <button disabled={busy || !dirty || stale || !content.trim()} onClick={() => void run(async () => {
          accept(await terminalApi.setAgentSoul(person.id, content, saved.revision));
          setNotice("Proposta de alma salva. A identidade aprovada permanece vigente até a revisão.");
        })}>Salvar proposta de alma</button>
        {review ? <button disabled={busy || dirty || stale || !saved.content.trim()} onClick={() => void run(async () => {
          accept(await terminalApi.completeTraining(person.id, person.member?.memory?.revision ?? 0, saved.revision));
          setNotice("Alma aprovada e treinamento concluído após sua revisão.");
        })}>Revisado: concluir treinamento</button> : <button disabled={busy || dirty || stale || !pending || !saved.content.trim()} onClick={() => void run(async () => {
          accept(await terminalApi.approveAgentSoul(person.id, saved.revision));
          setNotice("Alma aprovada. Atualizações de memória não alteram esta identidade.");
        })}>Aprovar alma</button>}
      </footer>
    </section>
  </div>;
}
