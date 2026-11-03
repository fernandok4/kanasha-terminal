import { useEffect, useState } from "react";
import { terminalApi } from "../api";

export function AreaInstructions({ areaId, areaName, onClose, onSaved }: {
  areaId: string; areaName: string; onClose: () => void; onSaved: () => void;
}) {
  const [document, setDocument] = useState<{ path: string; content: string; revision: string } | null>(null);
  const [content, setContent] = useState("");
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    let disposed = false;
    void terminalApi.getAreaInstructions(areaId).then(value => {
      if (!disposed) { setDocument(value); setContent(value.content); }
    }).catch(e => { if (!disposed) setError(String(e)); });
    return () => { disposed = true; };
  }, [areaId]);

  function close() {
    if (busy) return;
    if (document && content !== document.content && !window.confirm("Descartar as alterações ainda não salvas nas instruções?")) return;
    onClose();
  }
  async function save() {
    if (!document) return;
    setBusy(true); setError(""); setNotice("");
    try {
      const updated = await terminalApi.setAreaInstructions(areaId, content, document.revision);
      setDocument(updated); setContent(updated.content);
      setNotice("Instruções salvas. Encerre e reabra as sessões com atualização pendente para aplicar; o histórico será preservado.");
      onSaved();
    } catch (e) { setError(String(e)); }
    finally { setBusy(false); }
  }
  return <div className="modal-backdrop" onKeyDown={e => { e.stopPropagation(); if (e.key === "Escape") close(); }}>
    <section className="modal area-instructions" role="dialog" aria-modal="true" aria-labelledby="area-instructions-title">
      <form onSubmit={e => { e.preventDefault(); void save(); }}>
        <h2 id="area-instructions-title">Instruções da Área — {areaName}</h2>
        <p>Orientações compartilhadas por todas as pessoas e workspaces desta área. Complementam os AGENTS.md dos projetos.</p>
        {document ? <>
          <p className="instructions-path">{document.path}</p>
          <label htmlFor="area-instructions-content">Instruções em Markdown</label>
          <textarea id="area-instructions-content" autoFocus required value={content} onChange={e => { setContent(e.target.value); setNotice(""); }} spellCheck={false} />
          <p>Sessões já abertas mantêm a versão carregada até serem encerradas e reabertas. Limite: 64 KiB.</p>
        </> : !error ? <p role="status">Carregando instruções…</p> : null}
        {notice ? <p role="status">{notice}</p> : null}
        {error ? <p role="alert" className="team-error">{error}</p> : null}
        <div className="dialog-actions"><button type="button" disabled={busy} onClick={close}>Fechar</button><button disabled={busy || !document || !content.trim() || content === document.content}>Salvar instruções</button></div>
      </form>
    </section>
  </div>;
}
