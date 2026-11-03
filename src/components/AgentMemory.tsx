import { useState } from "react";
import { terminalApi } from "../api";
import type { AppSnapshot, TerminalPanelModel } from "../types";

export function AgentMemory({ person, onSnapshot, onClose }: {
  person: TerminalPanelModel; onSnapshot: (snapshot: AppSnapshot) => void; onClose: () => void;
}) {
  const [saved, setSaved] = useState(person.member?.memory ?? { content: "", revision: 0 });
  const [content, setContent] = useState(saved.content);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const dirty = content !== saved.content;
  const training = [...(person.member?.jobs ?? [])].reverse().find(job => job.training);
  async function save() {
    setBusy(true); setError(""); setNotice("");
    try {
      const snapshot = await terminalApi.setAgentMemory(person.id, content, saved.revision);
      const memory = snapshot.areas.flatMap(a => a.workspaces).flatMap(w => w.panels).find(p => p.id === person.id)?.member?.memory;
      if (memory) { setSaved(memory); setContent(memory.content); }
      onSnapshot(snapshot); setNotice("Memória salva. Será recuperada nas próximas atribuições e ao reabrir a sessão.");
    } catch (e) { setError(String(e)); } finally { setBusy(false); }
  }
  async function approve() {
    setBusy(true); setError("");
    try { onSnapshot(await terminalApi.completeTraining(person.id, saved.revision, person.member?.soul?.revision ?? 0)); setNotice("Treinamento concluído após sua revisão."); }
    catch (e) { setError(String(e)); } finally { setBusy(false); }
  }
  function close() {
    if (busy) return;
    if (!dirty || window.confirm("Descartar as alterações ainda não salvas na memória?")) onClose();
  }
  return <div className="modal-backdrop" onKeyDown={e => { e.stopPropagation(); if (e.key === "Escape") close(); }}><section className="modal agent-memory" role="dialog" aria-modal="true" aria-labelledby="agent-memory-title">
    <h2 id="agent-memory-title">Memória de {person.title}</h2>
    <p>Aprendizados posteriores, preferências observadas e contexto útil das demandas, com origem, referências e dúvidas. A memória é separada da alma e do histórico da conversa; até 16.000 caracteres.</p>
    {training?.result ? <details><summary>Último retorno do treinamento</summary><pre>{training.result}</pre></details> : null}
    <label htmlFor="agent-memory-content">Aprendizado em Markdown</label>
    <textarea id="agent-memory-content" autoFocus maxLength={16000} value={content} onChange={e => { setContent(e.target.value); setNotice(""); }} placeholder="O aprendizado registrado pelo agente aparecerá aqui." />
    <p>Revisão {saved.revision}{person.member?.memory?.updatedBy === "user" ? " · última edição pelo usuário" : ""}</p>
    {person.member?.training === "review" ? <p>Confira o conteúdo e as pendências. Concluir o treinamento libera esta pessoa para receber trabalho{person.member.manager ? " e permite que ela treine os novos contratados" : ""}.</p> : null}
    {person.member?.soul?.content ? <details><summary>Alma a revisar junto ao treinamento</summary><pre>{person.member.soul.content}</pre></details> : <p>A alma ainda não foi consolidada. Abra Alma e treinamento para revisar a identidade permanente antes de concluir a formação.</p>}
    {error ? <p role="alert">{error}</p> : null}{notice ? <p role="status">{notice}</p> : null}
    <footer><button disabled={busy} onClick={close}>Fechar</button><button disabled={busy || !dirty || !content.trim()} onClick={() => void save()}>Salvar memória</button>{person.member?.training === "review" ? <button disabled={busy || dirty || !person.member?.soul?.content.trim()} onClick={() => void approve()}>Revisado: concluir treinamento</button> : null}</footer>
  </section></div>;
}
