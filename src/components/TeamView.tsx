import { useState } from "react";
import { terminalApi } from "../api";
import { AgentExecutionFields, executionPresets, levelLabels } from "./AgentExecutionFields";
import { AgentSoul } from "./AgentSoul";
import { DeliveryHistory } from "./DeliveryHistory";
import { AgentMemory } from "./AgentMemory";
import type { AgentExecution, AppSnapshot, Assignment, MemberRuntime, TerminalPanelModel, Workspace } from "../types";

const statusLabels: Record<Assignment["status"], string> = { queued: "Na fila", running: "Em execução", done: "Concluída", failed: "Falhou", interrupted: "Interrompida", uncertain: "Verificar na sessão" };
const stepStatusLabels = { not_started: "Não iniciado", in_progress: "Em andamento", waiting: "Aguardando", blocked: "Bloqueado", done: "Concluído" };
const trainingLabels = { awaiting: "Aguardando treinamento", learning: "Em treinamento", review: "Aguardando revisão", ready: "Treinamento concluído" };
const specialties: Record<string, string> = {
  DEV: "Desenvolver soluções, investigar problemas e explicar decisões técnicas. Colaborar com o restante da equipe conforme a tarefa.",
  QA: "Testar regras de negócio e jornadas pelos critérios de aceite usando kanasha-flow, conforme o treinamento da equipe. Registrar cenário, resultado esperado e observado, ambiente e evidências. Se a ferramenta ou os insumos não estiverem disponíveis, informar a limitação; não declarar validação sem execução. Encaminhar falhas ao gerente e respeitar o escopo de validação combinado.",
  PO: "Esclarecer objetivos, requisitos e prioridades. Colaborar com desenvolvimento e validação para definir entregas úteis.",
  Marketing: "Planejar comunicação, pesquisar público e preparar conteúdo. Solicitar apoio técnico quando a tarefa exigir implementação de código.",
};

function deliveryProgress(jobs: Assignment[]) {
  const deliveries = jobs.flatMap(job => job.deliveries?.slice(-1) ?? []);
  const pending = deliveries.filter(delivery => !delivery.reviews.length).length;
  const accepted = deliveries.filter(delivery => delivery.reviews[delivery.reviews.length - 1]?.assessment.decision === "accepted").length;
  return deliveries.length ? `${deliveries.length} entregas · ${pending} aguardando conferência · ${accepted} aceitas pelo gerente` : "Entregas não registradas";
}

export function TeamView({ workspace, snapshot, onSnapshot, onOpen }: {
  workspace: Workspace; snapshot: AppSnapshot; onSnapshot: (value: AppSnapshot) => void; onOpen: (id: string) => void;
}) {
  const people = workspace.panels.filter(p => p.member);
  const [selected, setSelected] = useState<string | null>(null);
  const person = people.find(p => p.id === selected) ?? people[0];
  const [editor, setEditor] = useState<{ id?: string; name: string; specialty: string; instructions: string; execution: AgentExecution; manager: boolean } | null>(null);
  const [message, setMessage] = useState("");
  const [memoryOpen, setMemoryOpen] = useState(false);
  const [soulOpen, setSoulOpen] = useState(false);
  const [freshContext, setFreshContext] = useState(false);
  const [taskCode, setTaskCode] = useState("");
  const [stepIds, setStepIds] = useState<string[]>([]);
  const [resetId, setResetId] = useState(() => crypto.randomUUID());
  const [trainingMode, setTrainingMode] = useState(false);
  const training = person?.member?.training ?? "awaiting";
  const isTraining = training !== "ready" || trainingMode;
  const managerReady = people.some(p => p.member?.manager && p.member.training === "ready");
  const [requestId, setRequestId] = useState(() => crypto.randomUUID());
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState("");
  const active = new Set(snapshot.activeTerminalIds);
  const completedTaskCodes = new Set(workspace.tasks?.filter(task => task.state.status === "done").map(task => task.taskCode));
  const taskLinks = new Map<string, Map<string, { person: TerminalPanelModel; jobs: Assignment[] }>>();
  for (const employee of people.filter(p => !p.member!.manager)) for (const job of employee.member!.jobs) {
    if (!job.taskCode || completedTaskCodes.has(job.taskCode)) continue;
    const members = taskLinks.get(job.taskCode) ?? new Map();
    const link = members.get(employee.id) ?? { person: employee, jobs: [] };
    link.jobs.push(job); members.set(employee.id, link); taskLinks.set(job.taskCode, members);
  }
  const selectedTask = workspace.tasks?.find(task => task.taskCode === taskCode.trim().toUpperCase());
  const taskOrder = person?.member?.manager ? [] : (person?.member?.taskQueue ?? [...new Set(person?.member?.jobs.flatMap(j => j.taskCode ? [j.taskCode] : []) ?? [])]).filter(code => workspace.tasks?.find(t => t.taskCode === code)?.state.status !== "done");
  const releasedTasks = person?.member && !person.member.manager ? workspace.tasks?.filter(task => task.state.status !== "done" && !taskOrder.includes(task.taskCode) && person.member!.jobs.slice().reverse().find(job => job.taskCode === task.taskCode)?.status === "done").map(task => task.taskCode) ?? [] : [];
  const pendingJobs = person?.member?.jobs.filter(j => j.status === "queued") ?? [];
  const orderedJobs = [...pendingJobs].sort((a, b) => {
    if (person?.member?.manager) return 0;
    const order = (job: Assignment) => job.taskCode ? taskOrder.indexOf(job.taskCode) : Number.MAX_SAFE_INTEGER;
    return order(a) - order(b);
  });
  const runtime = person ? snapshot.memberStates?.[person.id] : undefined;

  async function action(work: () => Promise<void>) {
    setBusy(true); setError(""); setNotice("");
    try { await work(); } catch (e) { setError(typeof e === "string" ? e : e instanceof Error ? e.message : "Não foi possível concluir a ação."); }
    finally { setBusy(false); }
  }
  function hire() { const manager = !people.some(p => p.member?.manager); setEditor({ name: "", specialty: manager ? "Coordenação" : "Desenvolvedor", instructions: "", manager, execution: executionPresets[manager ? "manager" : "senior"] }); }
  async function save() {
    if (!editor) return;
    const input = { name: editor.name.trim(), specialty: editor.specialty.trim(), instructions: editor.instructions.trim(), execution: editor.execution };
    if (!input.specialty || (editor.id && (!input.name || !input.instructions))) { setError("Preencha nome, especialidade e orientações."); return; }
    await action(async () => {
      if (editor.id) {
        if (active.has(editor.id) || snapshot.memberStates?.[editor.id]?.connected) {
          if (!window.confirm("Encerrar a sessão desta pessoa para aplicar as novas orientações? O histórico do Codex será preservado.")) return;
          onSnapshot(await terminalApi.stopTerminal(editor.id));
        }
        onSnapshot(await terminalApi.editAgent({ terminalId: editor.id, ...input }));
      } else {
        const updated = await terminalApi.hireAgent({ workspaceId: workspace.id, specialty: input.specialty, execution: input.execution });
        const newPerson = updated.areas.flatMap(a => a.workspaces).find(w => w.id === workspace.id)?.panels.find(p => p.member && !people.some(old => old.id === p.id));
        if (newPerson) setSelected(newPerson.id);
        onSnapshot(updated);
      }
      setEditor(null); setNotice("Equipe atualizada. Novas pessoas passam pelo treinamento antes de receber trabalho.");
    });
  }
  async function remove(panel: TerminalPanelModel) {
    if (!window.confirm(`Retirar ${panel.title} da equipe e encerrar sua sessão? O cadastro, a memória e as atribuições serão removidos do KanashaTerminal; arquivos do projeto não serão apagados.`)) return;
    await action(async () => {
      if (active.has(panel.id) || snapshot.memberStates?.[panel.id]?.connected) onSnapshot(await terminalApi.stopTerminal(panel.id));
      onSnapshot(await terminalApi.removeTerminal(panel.id));
    });
  }
  return <section className="team-stage" aria-label={`Equipe de ${workspace.name}`}>
    <header className="team-heading"><div><p className="eyebrow">Sua equipe</p><h1>{workspace.name}</h1><p>Você escolhe a função e o nível. O gerente treina e coordena a equipe. Cada funcionário executa sua fila uma tarefa por vez; várias tarefas podem ficar com a mesma pessoa.</p></div><button disabled={busy} onClick={hire}>+ Contratar {people.length ? "agente" : "gerente"}</button></header>
    {taskLinks.size ? <section aria-label="Tarefas e responsáveis"><h2>Tarefas e responsáveis</h2><table><thead><tr><th>Tarefa Jira</th><th>Responsável</th><th>Andamento das atribuições</th><th>Entregas e conferência</th></tr></thead><tbody>{[...taskLinks].flatMap(([code, members]) => [...members.values()].map(link => <tr key={`${code}-${link.person.id}`}><td>{code}</td><td><button onClick={() => { setSelected(link.person.id); setMessage(""); setTaskCode(code); setStepIds([]); setTrainingMode(false); setFreshContext(false); setRequestId(crypto.randomUUID()); setResetId(crypto.randomUUID()); }}>{link.person.title}</button></td><td>{link.jobs.filter(j => j.status === "done").length} concluídas · {link.jobs.filter(j => j.status === "running").length} em execução · {link.jobs.filter(j => j.status === "queued").length} na fila{link.jobs.some(j => j.status === "uncertain" || j.status === "failed" || j.status === "interrupted") ? " · há atribuições que precisam de revisão" : ""}</td><td>{deliveryProgress(link.jobs)}</td></tr>))}</tbody></table><p className="team-muted">O vínculo fica salvo em cada atribuição. Uma tarefa pode envolver várias pessoas; cada uma mantém sua fila.</p></section> : null}
    {workspace.tasks?.length ? <section aria-label="Steps por tarefa"><h2>Steps por tarefa</h2>{workspace.tasks.map(task => <details key={task.taskCode}><summary>{task.taskCode} · {task.state.status === "done" ? "Concluída" : "Em acompanhamento"}</summary>{task.state.steps?.length ? <ol>{task.state.steps.map(step => { const links = people.filter(p => p.member?.jobs.some(j => j.taskCode === task.taskCode && j.stepIds?.includes(step.id))); return <li key={step.id}>{step.title} · {stepStatusLabels[step.status]} · {links.length ? links.map(p => p.title).join(", ") : "Sem responsável atribuído"}</li>; })}</ol> : <p>O gerente ainda não registrou os steps desta tarefa.</p>}</details>)}</section> : null}
    <div className="team-grid">
      {people.map(p => { const state = snapshot.memberStates?.[p.id]; return <button key={p.id} className={`team-card ${person?.id === p.id ? "selected" : ""}`} aria-label={`Selecionar ${p.title}, ${p.member?.specialty}`} aria-pressed={person?.id === p.id} onClick={() => { setSelected(p.id); setMessage(""); setTrainingMode(false); setFreshContext(false); setTaskCode(""); setStepIds([]); setResetId(crypto.randomUUID()); setMemoryOpen(false); setSoulOpen(false); setRequestId(crypto.randomUUID()); }}>
        <span className="team-avatar" aria-hidden="true">{p.title.slice(0, 1).toUpperCase()}</span><strong>{p.title}</strong><span>{p.member?.manager ? "Gerente · " : ""}{p.member?.specialty}</span><small>{p.member?.paused ? "Interrompido por você" : state?.turnId ? "Trabalhando" : state?.connected ? "Disponível" : "Sessão encerrada"}</small><small>{levelLabels[p.member?.execution?.level ?? (p.member?.manager ? "manager" : "senior")]} · {p.member?.execution?.model ?? "gpt-6-sol"} · {p.member?.execution?.reasoningEffort ?? "xhigh"}</small><span>{trainingLabels[p.member?.training ?? "awaiting"]}</span><p>{state?.activity || "Aguardando uma atribuição"}</p><small>{p.member?.jobs.filter(j => j.status === "queued").length ?? 0} na fila</small>
      </button>; })}
    </div>
    {person?.member ? <section className="team-detail" aria-label={`Detalhes de ${person.title}`}>
      <div className="team-heading"><div><h2>{person.title}</h2><p>{person.member.instructions}</p></div><button disabled={busy} onClick={() => void action(async () => { onSnapshot(await terminalApi.startTerminal(person.id)); onOpen(person.id); })}>Abrir terminal</button></div>
      {person.member.professionalProfile ? <>
        <p>{person.member.personality}</p>
        <details className="team-profile"><summary>Perfil profissional · criado pelo Codex</summary><p className="team-muted">Gerado em {new Date(person.member.professionalProfile.generatedAt * 1000).toLocaleString("pt-BR")}. Este é o perfil inicial. A alma aprovada define a identidade permanente; aprendizados posteriores ficam na memória.</p><pre>{person.member.professionalProfile.content}</pre></details>
      </> : <p>O Codex criará o perfil profissional na primeira conversa ou treinamento, usando a função e o contexto da empresa. O nome atual será preservado.</p>}
      <p>{person.member.soul?.approved ? `Alma aprovada · revisão ${person.member.soul.approved.revision}${person.member.soul.revision !== person.member.soul.approved.revision ? " · nova proposta aguardando revisão" : ""}` : "Alma ainda não aprovada. O treinamento inicial forma sua identidade permanente."}</p>
      <div className="team-actions"><button disabled={busy} onClick={() => setSoulOpen(true)}>Alma e treinamento</button><button disabled={busy} onClick={() => setMemoryOpen(true)}>Memória e treinamento</button>{!person.member.manager && training === "awaiting" ? <button disabled={busy || !managerReady || person.member.paused} onClick={() => void action(async () => { onSnapshot(await terminalApi.requestOnboarding(person.id, requestId)); setRequestId(crypto.randomUUID()); setNotice("Solicitação de treinamento enviada ao gerente."); })}>Solicitar treinamento ao gerente</button> : null}</div>
      {person.member.manager && training === "awaiting" ? <p>Ensine ao gerente como a empresa funciona e indique os documentos relevantes. Depois, revise a alma e a memória e conclua o treinamento para liberar a equipe.</p> : null}
      {!person.member.manager && training === "awaiting" && !managerReady ? <p>Esta pessoa aguarda você revisar e concluir o treinamento do gerente.</p> : null}
      {training === "review" ? <p>A formação está pronta para revisão em <strong>Alma e treinamento</strong>.</p> : null}
      {runtime?.model ? <p className="team-muted">Confirmado ao abrir a sessão: {runtime.model} · {runtime.reasoningEffort}.</p> : null}
      {runtime?.instructionsRevision ? <p className="team-muted">Instruções da área carregadas: <code>{runtime.instructionsRevision.slice(0, 12)}</code>.</p> : null}
      {runtime?.instructionsError ? <p role="alert" className="team-error">{runtime.instructionsError}</p> : null}
      {runtime?.instructionsPending ? <p role="status">Instruções da área alteradas. Encerre a sessão pelo terminal e abra novamente para aplicar a atualização. O histórico será preservado.</p> : null}
      <div className="team-actions"><button disabled={busy} onClick={() => setEditor({ id: person.id, name: person.title, specialty: person.member!.specialty, instructions: person.member!.instructions, manager: person.member!.manager, execution: person.member!.execution ?? executionPresets[person.member!.manager ? "manager" : "senior"] })}>Editar pessoa</button><button disabled={busy || !runtime?.connected || person.member.paused} onClick={() => void action(async () => { onSnapshot(await terminalApi.interruptAgent(person.id)); })}>Interromper trabalho</button>{person.member.paused ? <button disabled={busy} onClick={() => void action(async () => { onSnapshot(await terminalApi.startTerminal(person.id)); })}>Disponibilizar novamente</button> : null}<button disabled={busy || (person.member.manager && people.length > 1)} title={person.member.manager && people.length > 1 ? "Retire os membros antes de retirar o gerente" : undefined} onClick={() => void remove(person)}>Retirar da equipe</button></div>
      {runtime?.request ? <AgentQuestion key={String(runtime.request.id)} request={runtime.request} disabled={busy} onAnswer={result => void action(async () => { onSnapshot(await terminalApi.answerAgent(person.id, runtime.request!.id, result)); })} /> : null}
      <form className="team-message" onSubmit={e => { e.preventDefault(); if (!message.trim()) return; void action(async () => { onSnapshot(await terminalApi.assignAgent({ terminalId: person.id, requestId, message, training: isTraining, freshContext: !isTraining && !person.member!.manager && !taskCode.trim() && freshContext, taskCode: !isTraining && taskCode.trim() ? taskCode.trim().toUpperCase() : undefined, stepIds: !isTraining ? stepIds : [] })); setMessage(""); setRequestId(crypto.randomUUID()); setNotice(isTraining ? "Treinamento registrado. Confira a memória ao finalizar." : "Atribuição adicionada à fila. Será executada após as anteriores, com resultado abaixo."); }); }}>
        {!isTraining ? <><label htmlFor="team-task-code">Código da tarefa (Jira)<input id="team-task-code" maxLength={80} value={taskCode} onChange={e => { setTaskCode(e.target.value); setStepIds([]); setRequestId(crypto.randomUUID()); }} placeholder="Ex.: B2C-1234" /></label><p className="team-muted">Use o mesmo código em analisar, planejar e implementar. Só a troca de tarefa abre uma conversa nova do funcionário.</p></> : null}
        {!isTraining && selectedTask?.state.steps?.length ? <fieldset><legend>Steps desta atribuição</legend>{selectedTask.state.steps.map(step => <label key={step.id}><input type="checkbox" checked={stepIds.includes(step.id)} onChange={e => { setStepIds(e.target.checked ? [...stepIds, step.id] : stepIds.filter(id => id !== step.id)); setRequestId(crypto.randomUUID()); }} />{step.title}</label>)}</fieldset> : null}
        <label htmlFor="team-message">{isTraining ? `Treinamento de ${person.title}` : person.member.manager ? "O que você quer realizar?" : `Orientação para ${person.title}`}</label><textarea id="team-message" required maxLength={8000} value={message} onChange={e => { setMessage(e.target.value); setRequestId(crypto.randomUUID()); }} placeholder={isTraining ? "Explique os processos, suas preferências e os documentos que a pessoa deve consultar…" : "Descreva o objetivo e o contexto necessários…"} />{training === "ready" ? <label><input type="checkbox" checked={trainingMode} onChange={e => { setTrainingMode(e.target.checked); setRequestId(crypto.randomUUID()); }} />Treinamento complementar (exige nova revisão)</label> : null}{!isTraining && !person.member.manager && !taskCode.trim() ? <label><input type="checkbox" checked={freshContext} onChange={e => { setFreshContext(e.target.checked); setRequestId(crypto.randomUUID()); }} />Demanda independente sem código: iniciar em nova conversa (preserva lembranças)</label> : null}<button disabled={busy || person.member.paused || training === "learning" || !message.trim()}>{isTraining ? "Enviar treinamento" : "Enviar atribuição"}</button>
      </form>
      <div className="team-actions"><button disabled={busy || person.member.paused || !!runtime?.turnId || !!runtime?.request || person.member.jobs.some(j => j.status === "running" || j.status === "uncertain")} onClick={() => void action(async () => { onSnapshot(await terminalApi.resetAgentContext(person.id, resetId)); setResetId(crypto.randomUUID()); setNotice("Nova conversa iniciada. Alma, lembranças, arquivos, histórico e fila preservados."); })}>Nova conversa</button><span className="team-muted">Troca manual de demanda. Steps da mesma tarefa mantêm o contexto. Preserva alma, lembranças, arquivos, histórico e fila.</span></div>
      {releasedTasks.length ? <p className="team-muted">Participação liberada em {releasedTasks.join(", ")}. Essas tarefas continuam abertas; a fila pode avançar.</p> : null}
      <h3>Fila de tarefas</h3><p className="team-muted">O gerente pode liberar sua participação após a entrega, enquanto outra pessoa revisa ou testa a tarefa. Correções futuras voltam ao fim da fila. Até 10 atribuições pendentes por pessoa.</p>{taskOrder.length ? <ol className="team-history">{taskOrder.map((code, index) => <li key={code}><strong>{code} · {index === 0 ? "Tarefa atual" : "Aguardando tarefa anterior"}</strong>{person.member!.jobs.find(j => j.taskCode === code && j.status === "running") ? <p>Em execução: {person.member!.jobs.find(j => j.taskCode === code && j.status === "running")!.message}</p> : null}{orderedJobs.some(j => j.taskCode === code) ? <ol>{orderedJobs.filter(j => j.taskCode === code).map(job => <li key={job.id}><p>{job.message}</p>{job.stepIds?.length ? <small>Steps: {job.stepIds.map(id => workspace.tasks?.find(t => t.taskCode === code)?.state.steps?.find(s => s.id === id)?.title ?? id).join(", ")}</small> : null}</li>)}</ol> : <p className="team-muted">Sem steps na fila. Aguarda novos steps, liberação da participação pelo gerente ou conclusão da tarefa.</p>}</li>)}</ol> : <p className="team-muted">Nenhuma tarefa Jira aguardando na fila.</p>}
      {orderedJobs.some(j => !j.taskCode) ? <><h3>Atribuições sem código</h3><ol className="team-history">{orderedJobs.filter(j => !j.taskCode).map(job => <li key={job.id}><p>{job.message}</p><small>{job.freshContext ? "Iniciará em nova conversa" : "Manterá a conversa atual"}</small></li>)}</ol></> : null}
      <h3>Atribuições e resultados</h3>{person.member.jobs.length ? <ol className="team-history">{[...person.member.jobs].reverse().map(job => <li key={job.id}><div className="team-heading"><strong>{job.training ? "Treinamento · " : ""}{statusLabels[job.status]}</strong><small>{new Date(job.updatedAt * 1000).toLocaleString("pt-BR")}</small></div><p>{job.taskCode ? <strong>{job.taskCode} · </strong> : null}{job.message}</p>{job.result ? <pre>{job.result}</pre> : null}<DeliveryHistory assignment={job} /></li>)}</ol> : <p className="team-muted">Nenhuma atribuição registrada. Você também pode conversar no terminal.</p>}
      {person.member.jobs.some(j => j.status === "uncertain") ? <button disabled={busy} onClick={() => { if (window.confirm("Você conferiu o histórico? Encerrar esta execução pendente cancela o trabalho e libera a pessoa para receber uma nova atribuição após disponibilizá-la novamente.")) void action(async () => { onSnapshot(await terminalApi.stopTerminal(person.id)); }); }}>Encerrar execução pendente</button> : null}
    </section> : <p>Contrate um gerente para começar a equipe nesta sala.</p>}
    <p className="team-muted">Codex com sua autenticação local. Atribuições e resultados ficam salvos neste dispositivo; o Codex mantém o histórico das suas sessões.</p>
    {notice ? <p role="status">{notice}</p> : null}{error ? <p role="alert" className="team-error">{error}</p> : null}
    {soulOpen && person ? <AgentSoul key={person.id} person={person} onSnapshot={onSnapshot} onClose={() => setSoulOpen(false)} /> : null}
    {memoryOpen && person ? <AgentMemory key={person.id} person={person} onSnapshot={onSnapshot} onClose={() => setMemoryOpen(false)} /> : null}
    {editor ? <div className="modal-backdrop"><section className="modal team-editor" role="dialog" aria-modal="true" aria-labelledby="team-editor-title"><form onSubmit={e => { e.preventDefault(); void save(); }}><h2 id="team-editor-title">{editor.id ? "Editar pessoa" : "Contratar agente"}</h2>{editor.id ? <label>Nome<input autoFocus required maxLength={120} value={editor.name} onChange={e => setEditor({ ...editor, name: e.target.value })} /></label> : <p>A pessoa receberá um nome ao ser contratada, que você poderá editar. O Codex criará personalidade e perfil profissional na primeira conversa ou treinamento, preservando esse nome. O gerente fará a integração após concluir seu próprio treinamento.</p>}<label>Especialidade<input required list="team-specialties" maxLength={120} value={editor.specialty} onChange={e => setEditor({ ...editor, specialty: e.target.value })} /></label><datalist id="team-specialties">{Object.keys(specialties).map(s => <option key={s} value={s} />)}</datalist>{editor.id ? <><button type="button" onClick={() => { const text = specialties[editor.specialty]; if (text) setEditor({ ...editor, instructions: text }); }}>Usar orientações da especialidade</button><label>Responsabilidades e orientações<textarea required maxLength={8000} value={editor.instructions} onChange={e => setEditor({ ...editor, instructions: e.target.value })} /></label></> : null}<AgentExecutionFields workspaceId={workspace.id} manager={editor.manager} value={editor.execution} onChange={execution => setEditor({ ...editor, execution })} /><p>A especialidade é ajustável. A pessoa pode colaborar em outras atividades compatíveis.</p>{error ? <p role="alert">{error}</p> : null}<footer><button type="button" disabled={busy} onClick={() => setEditor(null)}>Cancelar</button><button disabled={busy}>{busy ? "Salvando…" : "Salvar"}</button></footer></form></section></div> : null}
  </section>;
}

function AgentQuestion({ request, disabled, onAnswer }: { request: NonNullable<MemberRuntime["request"]>; disabled: boolean; onAnswer: (result: unknown) => void }) {
  const [answers, setAnswers] = useState<Record<string, string>>({});
  if (request.method === "item/tool/requestUserInput") return <form className="team-question" onSubmit={e => { e.preventDefault(); onAnswer({ answers: Object.fromEntries(Object.entries(answers).map(([id, answer]) => [id, { answers: [answer] }])) }); }}><h3>A pessoa precisa da sua resposta</h3>{request.params.questions?.map(q => <label key={q.id}>{q.question}{q.options?.map(o => <button type="button" key={o.label} title={o.description} disabled={disabled} onClick={() => setAnswers({ ...answers, [q.id]: o.label })}>{o.label}</button>)}<input required value={answers[q.id] ?? ""} onChange={e => setAnswers({ ...answers, [q.id]: e.target.value })} /></label>)}<button disabled={disabled}>Responder</button></form>;
  return <section className="team-question"><h3>Aprovação solicitada</h3><p>{request.params.reason ?? request.params.command ?? "O Codex solicitou aprovação para continuar."}</p><button disabled={disabled} onClick={() => onAnswer({ decision: "accept" })}>Aprovar</button><button disabled={disabled} onClick={() => onAnswer({ decision: "decline" })}>Recusar</button></section>;
}
