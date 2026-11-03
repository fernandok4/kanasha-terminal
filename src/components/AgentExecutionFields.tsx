import { useEffect, useState } from "react";
import { terminalApi } from "../api";
import type { AgentExecution, CodexModel } from "../types";

export const executionPresets: Record<string, AgentExecution> = {
  junior: { level: "junior", model: "gpt-6-luna", reasoningEffort: "xhigh" },
  mid: { level: "mid", model: "gpt-5.6-terra", reasoningEffort: "xhigh" },
  senior: { level: "senior", model: "gpt-6-sol", reasoningEffort: "xhigh" },
  specialist: { level: "specialist", model: "gpt-6-astra", reasoningEffort: "high" },
  manager: { level: "manager", model: "gpt-6-sol", reasoningEffort: "xhigh" },
};
export const levelLabels: Record<AgentExecution["level"], string> = {
  junior: "Júnior", mid: "Pleno", senior: "Sênior", specialist: "Especialista", manager: "Gerente", custom: "Personalizado",
};

export function AgentExecutionFields({ value, manager, workspaceId, onChange }: {
  value: AgentExecution; manager: boolean; workspaceId: string; onChange: (value: AgentExecution) => void;
}) {
  const [models, setModels] = useState<CodexModel[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [revision, setRevision] = useState(0);
  useEffect(() => {
    let disposed = false;
    setLoading(true);
    setError("");
    setModels([]);
    void terminalApi.listCodexModels(workspaceId).then(catalog => {
      if (!Array.isArray(catalog)) throw new Error("O Codex retornou um catálogo inválido.");
      if (!disposed) setModels(catalog);
    }).catch(cause => {
      if (!disposed) setError(typeof cause === "string" ? cause : cause instanceof Error ? cause.message : "Não foi possível consultar os modelos.");
    }).finally(() => { if (!disposed) setLoading(false); });
    return () => { disposed = true; };
  }, [workspaceId, revision]);

  const selected = models.find(model => model.model === value.model);
  const efforts = selected?.supportedReasoningEfforts ?? [];
  const includesEffort = efforts.some(option => option.reasoningEffort === value.reasoningEffort);
  const adjust = (patch: Partial<AgentExecution>) => onChange({ ...value, ...patch, level: manager ? "manager" : "custom" });
  function selectModel(id: string) {
    const model = models.find(option => option.model === id);
    if (!model) return;
    const supported = model.supportedReasoningEfforts.map(option => option.reasoningEffort);
    const reasoningEffort = supported.includes(value.reasoningEffort) ? value.reasoningEffort
      : supported.includes(model.defaultReasoningEffort) ? model.defaultReasoningEffort
      : supported[0] ?? value.reasoningEffort;
    adjust({ model: id, reasoningEffort });
  }
  return <fieldset className="agent-execution-fields"><legend>Modelo de execução</legend>
    <label>Nível<select value={value.level} disabled={manager} onChange={e => onChange(executionPresets[e.target.value] ?? { ...value, level: "custom" })}>
      {Object.entries(levelLabels).filter(([level]) => manager ? level === "manager" : level !== "manager").map(([level, label]) => <option key={level} value={level}>{label}</option>)}
    </select></label>
    <label>Modelo<select value={value.model} disabled={loading || models.length === 0} onChange={e => selectModel(e.target.value)}>
      {!selected ? <option value={value.model}>{value.model} — configuração atual</option> : null}
      {models.map(model => <option key={model.model} value={model.model}>{model.displayName || model.model}</option>)}
    </select></label>
    <label>Raciocínio<select value={value.reasoningEffort} disabled={loading || !efforts.length} onChange={e => adjust({ reasoningEffort: e.target.value })}>
      {!includesEffort ? <option value={value.reasoningEffort}>{value.reasoningEffort} — configuração atual</option> : null}
      {efforts.map(option => <option key={option.reasoningEffort} value={option.reasoningEffort} title={option.description}>{option.reasoningEffort}</option>)}
    </select></label>
    <button type="button" disabled={loading} onClick={() => setRevision(current => current + 1)}>Atualizar modelos</button>
    {loading ? <p role="status">Consultando modelos no Codex…</p> : error ? <p role="alert">{error} A configuração atual foi preservada. Tente atualizar a lista.</p> : !models.length ? <p role="status">O Codex não retornou modelos. Confira o login e atualize a lista.</p> : !selected || !includesEffort ? <p role="status">A configuração atual não consta no catálogo consultado. Escolha um modelo e raciocínio disponíveis antes de abrir a sessão.</p> : null}
    <p>{manager ? "Você também pode escolher o modelo do gerente." : "O nível preenche modelo e raciocínio. Você pode personalizar a combinação."} A lista acompanha o catálogo do Codex instalado; a configuração é conferida novamente ao abrir a sessão.</p>
  </fieldset>;
}
