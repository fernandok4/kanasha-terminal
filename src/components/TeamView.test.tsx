import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { TeamView } from "./TeamView";
import type { AppSnapshot, Workspace } from "../types";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
const workspace: Workspace = { id: "room", name: "Produto", layout: null, panels: [
  { id: "manager", title: "Marina", profileId: "codex", member: { training: "ready", manager: true, specialty: "Coordenação", instructions: "Coordenar a equipe", paused: false, threadId: null, jobs: [] } },
  { id: "qa", title: "Lia", profileId: "codex", member: { training: "ready", manager: false, specialty: "QA", instructions: "Validar jornadas", paused: false, threadId: "thread-lia", jobs: [] } },
] };
const snapshot: AppSnapshot = { areas: [{ id: "company", name: "Empresa", rootPath: "/tmp", workspaces: [workspace] }], profiles: [], activeTerminalIds: ["qa"], memberStates: { qa: { connected: true, turnId: "turn-lia", activity: "Validando cadastro", updatedAt: 1 } } };
afterEach(cleanup);

it("oculta tarefas concluídas da tabela e preserva os steps e o histórico da pessoa", () => {
  const room = structuredClone(workspace);
  room.tasks = [
    { taskCode: "B2C-4423", state: { status: "done", steps: [{ id: "investigar", title: "Investigar timeout", status: "done" }] } },
    { taskCode: "B2C-123", state: { status: "in_progress" } },
  ];
  room.panels[1].member!.jobs = [
    { id: "finished", taskCode: "B2C-4423", stepIds: ["investigar"], message: "Investigação concluída", status: "done", result: "Resultado preservado", assignedBy: "manager", createdAt: 1, updatedAt: 2, turnId: null },
    { id: "active", taskCode: "B2C-123", message: "Implementação entregue para revisão", status: "done", result: "Aguardando revisão", assignedBy: "manager", createdAt: 1, updatedAt: 2, turnId: null },
  ];
  const original = structuredClone(room);
  render(<TeamView workspace={room} snapshot={snapshot} onSnapshot={vi.fn()} onOpen={vi.fn()} />);

  const table = within(screen.getByRole("region", { name: "Tarefas e responsáveis" })).getByRole("table");
  expect(within(table).queryByText("B2C-4423")).not.toBeInTheDocument();
  expect(within(table).getByText("B2C-123")).toBeVisible();
  const steps = screen.getByRole("region", { name: "Steps por tarefa" });
  fireEvent.click(within(steps).getByText("B2C-4423 · Concluída"));
  expect(within(steps).getByText("Investigar timeout · Concluído · Lia")).toBeVisible();

  fireEvent.click(screen.getByRole("button", { name: "Selecionar Lia, QA" }));
  const details = screen.getByRole("region", { name: "Detalhes de Lia" });
  expect(within(details).getByText("Investigação concluída")).toBeVisible();
  expect(within(details).getByText("Resultado preservado")).toBeVisible();
  expect(within(details).getByText("B2C-4423 ·")).toBeVisible();
  expect(room).toEqual(original);
  expect(invoke).not.toHaveBeenCalled();
});

it("atualiza a tabela ao concluir e reabrir a única tarefa", () => {
  const room = structuredClone(workspace);
  room.tasks = [{ taskCode: "B2C-4423", state: { status: "in_progress" } }];
  room.panels[1].member!.jobs = [{ id: "work", taskCode: "B2C-4423", message: "Investigar", status: "done", result: "Entrega registrada", assignedBy: "manager", createdAt: 1, updatedAt: 2, turnId: null }];
  const props = { snapshot, onSnapshot: vi.fn(), onOpen: vi.fn() };
  const { rerender } = render(<TeamView workspace={room} {...props} />);
  expect(screen.getByRole("region", { name: "Tarefas e responsáveis" })).toHaveTextContent("B2C-4423");

  const completed = structuredClone(room);
  completed.tasks![0].state.status = "done";
  rerender(<TeamView workspace={completed} {...props} />);
  expect(screen.queryByRole("region", { name: "Tarefas e responsáveis" })).not.toBeInTheDocument();
  expect(screen.getByRole("region", { name: "Steps por tarefa" })).toHaveTextContent("B2C-4423 · Concluída");

  rerender(<TeamView workspace={room} {...props} />);
  expect(screen.getByRole("region", { name: "Tarefas e responsáveis" })).toHaveTextContent("B2C-4423");
  expect(invoke).not.toHaveBeenCalled();
});

it("mostra a entrega pendente por tarefa e seus detalhes na pessoa selecionada", () => {
  const room = structuredClone(workspace);
  room.tasks = [{ taskCode: "B2C-4423", state: { status: "in_progress" } }];
  room.panels[1].member!.jobs = [{
    id: "work", taskCode: "B2C-4423", message: "Investigar", status: "done", result: "Entrega registrada", assignedBy: "manager", createdAt: 1, updatedAt: 2, turnId: null,
    deliveries: [{ revision: 1, submittedBy: room.panels[1].id, submittedAt: 2, declaration: { kind: "investigation", summary: "Referência ainda não conferida no GitLab.", limitations: "", contextIds: [], evidence: [] }, repositories: [], reviews: [] }],
  }];
  render(<TeamView workspace={room} snapshot={snapshot} onSnapshot={vi.fn()} onOpen={vi.fn()} />);
  expect(screen.getByRole("columnheader", { name: "Entregas e conferência" })).toBeVisible();
  expect(screen.getByText("1 entregas · 1 aguardando conferência · 0 aceitas pelo gerente")).toBeVisible();
  fireEvent.click(screen.getByRole("button", { name: "Selecionar Lia, QA" }));
  expect(screen.getByText("Entrega v1 · Aguardando conferência")).toBeVisible();
  expect(screen.getByText("Referência ainda não conferida no GitLab.")).toBeVisible();
});
it("mostra a participação liberada sem concluir a tarefa compartilhada", () => {
  const room = structuredClone(workspace);
  room.tasks = [{ taskCode: "B2C-X", state: { status: "in_progress", steps: [{ id: "review", title: "Revisar", status: "not_started" }] } }];
  room.panels[1].member!.taskQueue = [];
  room.panels[1].member!.jobs = [{ id: "delivery", message: "Implementação entregue", taskCode: "B2C-X", status: "done", result: "Evidências", assignedBy: "manager", createdAt: 1, updatedAt: 2, turnId: null }];
  render(<TeamView workspace={room} snapshot={snapshot} onSnapshot={vi.fn()} onOpen={vi.fn()} />);
  fireEvent.click(screen.getByRole("button", { name: "Selecionar Lia, QA" }));
  expect(screen.getByText(/Participação liberada em B2C-X/)).toHaveTextContent("Essas tarefas continuam abertas");
  expect(screen.getByText("Nenhuma tarefa Jira aguardando na fila.")).toBeVisible();
  expect(screen.getByText("B2C-X · Em acompanhamento")).toBeVisible();
});
it("mostra a versão carregada e a atualização pendente sem reiniciar o agente", () => {
  invoke.mockClear();
  const runtime = { connected: true, turnId: null, activity: "Disponível", updatedAt: 1, instructionsRevision: "abc123456789def", instructionsPending: true };
  render(<TeamView workspace={workspace} snapshot={{ ...snapshot, memberStates: { [workspace.panels[0].id]: runtime } }} onSnapshot={vi.fn()} onOpen={vi.fn()} />);
  expect(screen.getByText("abc123456789")).toBeInTheDocument();
  expect(screen.getByRole("status")).toHaveTextContent("Instruções da área alteradas");
  expect(invoke).not.toHaveBeenCalled();
});
const models = ["gpt-6-sol", "gpt-6-luna", "gpt-6-astra", "gpt-5.6-terra", "gpt-6.1-sol"].map(model => ({ model, displayName: model, defaultReasoningEffort: "medium", supportedReasoningEfforts: ["low", "medium", "high", "xhigh", "max", "ultra"].map(reasoningEffort => ({ reasoningEffort, description: "" })) }));
beforeEach(() => { invoke.mockReset().mockImplementation(async (command: string) => command === "list_codex_models" ? models : snapshot); });

it("distingue perfil pendente do perfil criado pelo Codex sem confundir com treinamento", () => {
  const room = structuredClone(workspace);
  room.panels[0].member!.automaticName = true;
  const { rerender } = render(<TeamView workspace={room} snapshot={snapshot} onSnapshot={vi.fn()} onOpen={vi.fn()} />);
  expect(screen.getByText(/O nome atual será preservado/)).toBeInTheDocument();
  room.panels[0].member!.professionalProfile = { name: "Marina", personality: "Investigativa", specialty: "Coordenação", generatedAt: 1, content: "## Missão\nCoordenar com critérios claros." };
  room.panels[0].member!.training = "awaiting";
  rerender(<TeamView workspace={room} snapshot={snapshot} onSnapshot={vi.fn()} onOpen={vi.fn()} />);
  expect(screen.queryByText(/O nome atual é provisório/)).not.toBeInTheDocument();
  expect(screen.getByText("Perfil profissional · criado pelo Codex")).toBeInTheDocument();
  expect(screen.getByText(/Coordenar com critérios claros/)).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Enviar treinamento" })).toBeInTheDocument();
  expect(invoke).not.toHaveBeenCalled();
});

it("contrata escolhendo somente a função, sem solicitar nome ou personalidade", async () => {
  render(<TeamView workspace={workspace} snapshot={snapshot} onSnapshot={vi.fn()} onOpen={vi.fn()} />);
  fireEvent.click(screen.getByRole("button", { name: "+ Contratar agente" }));
  const dialog = screen.getByRole("dialog");
  expect(within(dialog).queryByLabelText("Nome")).not.toBeInTheDocument();
  fireEvent.change(within(dialog).getByLabelText("Especialidade"), { target: { value: "DEV e investigação" } });
  expect(within(dialog).queryByLabelText("Responsabilidades e orientações")).not.toBeInTheDocument();
  fireEvent.click(within(dialog).getByRole("button", { name: "Salvar" }));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("hire_agent", { input: { workspaceId: "room", specialty: "DEV e investigação", execution: { level: "senior", model: "gpt-6-sol", reasoningEffort: "xhigh" } } }));
  expect(invoke).not.toHaveBeenCalledWith("start_terminal", expect.anything());
});

it("interrompe somente a pessoa selecionada e abre seu terminal", async () => {
  const onOpen = vi.fn();
  render(<TeamView workspace={workspace} snapshot={snapshot} onSnapshot={vi.fn()} onOpen={onOpen} />);
  fireEvent.click(screen.getByRole("button", { name: "Selecionar Lia, QA" }));
  expect(screen.getByText("Validando cadastro")).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Interromper trabalho" }));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("interrupt_agent", { terminalId: "qa" }));
  await waitFor(() => expect(screen.getByRole("button", { name: "Abrir terminal" })).toBeEnabled());
  fireEvent.click(screen.getByRole("button", { name: "Abrir terminal" }));
  await waitFor(() => expect(onOpen).toHaveBeenCalledWith("qa"));
});

it("aplica o preset escolhido na contratação", async () => {
  render(<TeamView workspace={workspace} snapshot={snapshot} onSnapshot={vi.fn()} onOpen={vi.fn()} />);
  fireEvent.click(screen.getByRole("button", { name: "+ Contratar agente" }));
  fireEvent.change(screen.getByLabelText("Nível"), { target: { value: "specialist" } });
  expect(screen.getByLabelText("Modelo")).toHaveValue("gpt-6-astra");
  expect(screen.getByLabelText("Raciocínio")).toHaveValue("high");
  fireEvent.click(screen.getByRole("button", { name: "Salvar" }));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("hire_agent", { input: { workspaceId: "room", specialty: "Desenvolvedor", execution: { level: "specialist", model: "gpt-6-astra", reasoningEffort: "high" } } }));
});

it("permite escolher modelo e raciocínio do gerente", async () => {
  render(<TeamView workspace={workspace} snapshot={snapshot} onSnapshot={vi.fn()} onOpen={vi.fn()} />);
  fireEvent.click(screen.getByRole("button", { name: "Editar pessoa" }));
  expect(screen.getByLabelText("Modelo")).toHaveValue("gpt-6-sol");
  await waitFor(() => expect(screen.getByLabelText("Modelo")).toBeEnabled());
  fireEvent.change(screen.getByLabelText("Modelo"), { target: { value: "gpt-6.1-sol" } });
  fireEvent.change(screen.getByLabelText("Raciocínio"), { target: { value: "high" } });
  fireEvent.click(screen.getByRole("button", { name: "Salvar" }));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("edit_agent", { input: { terminalId: "manager", name: "Marina", specialty: "Coordenação", instructions: "Coordenar a equipe", execution: { level: "manager", model: "gpt-6.1-sol", reasoningEffort: "high" } } }));
});

it("troca o nível para personalizado ao mudar o modelo de um contratado", async () => {
  render(<TeamView workspace={workspace} snapshot={snapshot} onSnapshot={vi.fn()} onOpen={vi.fn()} />);
  fireEvent.click(screen.getByRole("button", { name: "+ Contratar agente" }));
  await waitFor(() => expect(screen.getByLabelText("Modelo")).toBeEnabled());
  fireEvent.change(screen.getByLabelText("Modelo"), { target: { value: "gpt-6-luna" } });
  expect(screen.getByLabelText("Nível")).toHaveValue("custom");
  expect(screen.getByLabelText("Raciocínio")).toHaveValue("xhigh");
});

it("repete o recibo de envio quando a resposta falha, para não duplicar trabalho", async () => {
  invoke.mockRejectedValue("Conexão interrompida");
  render(<TeamView workspace={workspace} snapshot={snapshot} onSnapshot={vi.fn()} onOpen={vi.fn()} />);
  fireEvent.change(screen.getByLabelText("O que você quer realizar?"), { target: { value: "Validar o cadastro" } });
  fireEvent.click(screen.getByRole("button", { name: "Enviar atribuição" }));
  await screen.findByRole("alert");
  const first = invoke.mock.calls[0][1];
  fireEvent.click(screen.getByRole("button", { name: "Enviar atribuição" }));
  await waitFor(() => expect(invoke).toHaveBeenCalledTimes(2));
  expect(invoke.mock.calls[1][1]).toEqual(first);
});

it.each([false, true])("envia a mensagem do gerente para a conversa selecionada (treinamento=%s)", async training => {
  const room = structuredClone(workspace);
  room.panels[0].member!.managerTaskCode = "B2C-X";
  render(<TeamView workspace={room} snapshot={snapshot} onSnapshot={vi.fn()} onOpen={vi.fn()} />);
  if (training) fireEvent.click(screen.getByLabelText("Treinamento complementar (exige nova revisão)"));
  fireEvent.change(screen.getByLabelText(training ? "Treinamento de Marina" : "O que você quer realizar?"), { target: { value: "Conferir os critérios" } });
  fireEvent.click(screen.getByRole("button", { name: training ? "Enviar treinamento" : "Enviar atribuição" }));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("assign_agent", {
    input: expect.objectContaining({ terminalId: "manager", taskCode: "B2C-X", training, message: "Conferir os critérios" }),
  }));
});


it("enfileira um step com código Jira enquanto a pessoa trabalha, sem permitir reset manual", async () => {
  render(<TeamView workspace={workspace} snapshot={snapshot} onSnapshot={vi.fn()} onOpen={vi.fn()} />);
  fireEvent.click(screen.getByRole("button", { name: "Selecionar Lia, QA" }));
  expect(screen.getByRole("button", { name: "Nova conversa" })).toBeDisabled();
  fireEvent.change(screen.getByLabelText("Código da tarefa (Jira)"), { target: { value: "b2c-123" } });
  fireEvent.change(screen.getByLabelText("Orientação para Lia"), { target: { value: "Step 2: criar plano" } });
  expect(screen.getByRole("button", { name: "Enviar atribuição" })).toBeEnabled();
  fireEvent.click(screen.getByRole("button", { name: "Enviar atribuição" }));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("assign_agent", { input: expect.objectContaining({ terminalId: "qa", taskCode: "B2C-123", freshContext: false, message: "Step 2: criar plano" }) }));
  expect(invoke).not.toHaveBeenCalledWith("reset_agent_context", expect.anything());
});

it("mostra a fila em ordem e bloqueia conversa nova com resultado incerto", () => {
  const room = structuredClone(workspace);
  room.panels[1].member!.jobs = [
    { id: "x", taskCode: "B2C-123", status: "queued", message: "Primeira demanda", result: "", createdAt: 1, updatedAt: 1, turnId: null, assignedBy: "manager" },
    { id: "y", taskCode: "B2C-456", status: "queued", message: "Segunda demanda", result: "", createdAt: 2, updatedAt: 2, turnId: null, assignedBy: "manager" },
    { id: "pending", status: "uncertain", message: "Conferir resultado", result: "", createdAt: 0, updatedAt: 0, turnId: null, assignedBy: "manager" },
  ];
  render(<TeamView workspace={room} snapshot={{ ...snapshot, memberStates: {} }} onSnapshot={vi.fn()} onOpen={vi.fn()} />);
  fireEvent.click(screen.getByRole("button", { name: "Selecionar Lia, QA" }));
  const tasks = screen.getByRole("region", { name: "Tarefas e responsáveis" });
  expect(within(tasks).getByRole("table")).toHaveTextContent("B2C-123Lia");
  expect(within(tasks).getByRole("table")).toHaveTextContent("B2C-456Lia");
  const list = screen.getAllByRole("list")[0];
  const items = list.querySelectorAll(":scope > li");
  expect(items[0]).toHaveTextContent("B2C-123 · Tarefa atualPrimeira demanda");
  expect(items[1]).toHaveTextContent("B2C-456 · Aguardando tarefa anteriorSegunda demanda");
  expect(screen.getByRole("button", { name: "Nova conversa" })).toBeDisabled();
});

it("repete o mesmo recibo de nova conversa quando a resposta é incerta", async () => {
  invoke.mockRejectedValue("Resposta indisponível");
  render(<TeamView workspace={workspace} snapshot={{ ...snapshot, memberStates: {} }} onSnapshot={vi.fn()} onOpen={vi.fn()} />);
  fireEvent.click(screen.getByRole("button", { name: "Nova conversa" }));
  await screen.findByRole("alert");
  const receipt = invoke.mock.calls[0][1];
  fireEvent.click(screen.getByRole("button", { name: "Nova conversa" }));
  await waitFor(() => expect(invoke).toHaveBeenCalledTimes(2));
  expect(invoke.mock.calls[1]).toEqual(["reset_agent_context", receipt]);
});


it("liga atribuição a steps reais e agrupa um step tardio antes da tarefa seguinte", async () => {
  const room = structuredClone(workspace);
  room.tasks = [{ taskCode: "B2C-123", state: { status: "in_progress", steps: [{ id: "analisar", title: "Analisar", status: "done" }, { id: "plano", title: "Criar plano", status: "not_started" }] } }];
  const job = (id: string, taskCode: string, stepIds: string[], message: string) => ({ id, taskCode, stepIds, status: "queued" as const, message, result: "", createdAt: 1, updatedAt: 1, turnId: null, assignedBy: "manager" });
  room.panels[1].member!.taskQueue = ["B2C-123", "B2C-456"];
  room.panels[1].member!.jobs = [job("other", "B2C-456", [], "Próxima tarefa"), job("plan", "B2C-123", ["plano"], "Continuar o plano atual")];
  render(<TeamView workspace={room} snapshot={snapshot} onSnapshot={vi.fn()} onOpen={vi.fn()} />);
  fireEvent.click(screen.getByRole("button", { name: "Selecionar Lia, QA" }));
  const taskList = screen.getByRole("heading", { name: "Fila de tarefas" }).parentElement!.querySelector("ol.team-history")!;
  expect(taskList.querySelector(":scope > li")).toHaveTextContent("B2C-123 · Tarefa atualContinuar o plano atualSteps: Criar plano");
  fireEvent.change(screen.getByLabelText("Código da tarefa (Jira)"), { target: { value: "B2C-123" } });
  fireEvent.click(screen.getByRole("checkbox", { name: "Criar plano" }));
  fireEvent.change(screen.getByLabelText("Orientação para Lia"), { target: { value: "Atualizar plano" } });
  fireEvent.click(screen.getByRole("button", { name: "Enviar atribuição" }));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("assign_agent", { input: expect.objectContaining({ taskCode: "B2C-123", stepIds: ["plano"], terminalId: "qa", freshContext: false }) }));
});
