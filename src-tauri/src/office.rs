use crate::{codex::CodexSession, models::*, *};
use serde_json::{json, Value};
use std::thread;

fn bounded(value: String, limit: usize, field: &str) -> Result<String, String> {
    let text = task_text(value, limit, field)?;
    if text.trim().is_empty() {
        return Err(format!("Informe {field}."));
    }
    Ok(text)
}

fn instructions(panel: &TerminalPanel, area: &area_instructions::Document) -> String {
    let member = panel.member.as_ref().unwrap();
    let role = if member.manager {
        "Gerente: esclareça objetivo/aceite, consulte get_team e delegue por set_assignment. Confira entregas antes de atualizar resumo/Jira. Só você altera campos compartilhados de set_task; conclua a tarefa após verificar steps e entregas."
    } else {
        "Membro: execute a atribuição e registre a entrega. Use set_task apenas com agentStatus/agentCurrent para sua atividade. O resultado final chega automaticamente ao gerente."
    };
    format!(
        r#"# Sessão — {name}

Função: {specialty}. {role}

## Orientações do usuário

{user_instructions}

# Instruções da área

Fonte: {path} | Revisão: {revision}
Conteúdo já fornecido; não releia o arquivo.

{content}

# Contratos atuais da aplicação

Estes contratos atualizam descrições técnicas antigas; preserve as regras de negócio da área.

{learning}

## Coordenação

- Use a equipe contratada; só o usuário contrata, retira, pausa/retoma e escolhe modelos.
  Não crie sessões ou subagentes externos. Combine responsáveis por arquivos compartilhados.
- get_tasks lista a sala. get_task/get_task_steps/set_task usam taskCode real;
  sem código tratam a visão geral. Delegue com stepIds reais e estáveis da tarefa.
- Cada pessoa aceita até 10 atribuições pendentes, inclusive ocupada, e executa uma
  por vez. Steps da tarefa atual precedem tarefas seguintes; sem insumo, aguarde.
  Enfileire dependências só após a entrega anterior e mantenha filas sem espera circular.
- Mesma taskCode preserva contexto; trocar código abre conversa nova do funcionário.
  Sem código, freshContext=true pede nova conversa. reset_context só para outro membro
  ocioso, sem pergunta/execução incerta; nunca entre steps nem para retomar pausados.
- Após conferir e encerrar a participação individual, use release_task com terminalId,
  taskCode e assignmentId da última atribuição concluída. Exige ausência de trabalho
  próprio pendente/ativo/incerto. Steps compartilhados e tarefa podem continuar abertos.
  Libere também o revisor quando terminar. Não use paused ou done para destravar fila.
  Correções posteriores entram no fim da fila, sem interromper o trabalho atual.
- requestId identifica um pedido: após resposta incerta, consulte e repita o mesmo ID
  e conteúdo se necessário. Aguarde retornos automáticos sem polling.
- Atribuição: objetivo, escopo, referências/versão e aceite. Retorno: resultado,
  evidências e pendências. Use Markdown curto e legível; detalhes em arquivos reais.
  Evite repetir contexto conhecido. Atualize estado apenas nas transições relevantes.

{delivery}
"#,
        name = panel.title,
        specialty = member.specialty,
        user_instructions = member.instructions,
        learning = learning::instructions(member),
        path = area.path,
        revision = area.revision,
        content = area.content,
        delivery = crate::deliveries::GUIDANCE,
    )
}

pub(crate) fn task_code(value: String) -> Result<String, String> {
    let code = bounded(value.trim().to_uppercase(), 80, "código da tarefa")?;
    if code.contains(char::is_whitespace) {
        return Err("Informe somente o código da tarefa, sem espaços.".into());
    }
    Ok(code)
}

fn current_queued_task<'a>(room: &Workspace, member: &'a TeamMember) -> Option<&'a str> {
    member
        .task_queue
        .iter()
        .find(|code| {
            room.tasks
                .iter()
                .find(|task| &task.task_code == *code)
                .is_none_or(|task| task.state.status != TaskStatus::Done)
        })
        .map(String::as_str)
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ReleaseTaskInput {
    pub terminal_id: String,
    pub task_code: String,
    pub assignment_id: String,
}

fn next_assignment_index(room: &Workspace, member: &TeamMember) -> Option<usize> {
    if member.manager {
        return member
            .jobs
            .iter()
            .position(|j| j.status == AssignmentStatus::Queued);
    }
    match current_queued_task(room, member) {
        Some(code) => member.jobs.iter().position(|job| {
            job.status == AssignmentStatus::Queued && job.task_code.as_deref() == Some(code)
        }),
        None => member
            .jobs
            .iter()
            .position(|job| job.status == AssignmentStatus::Queued && job.task_code.is_none()),
    }
}

fn queued_assignments(member: &TeamMember) -> Vec<&Assignment> {
    let mut jobs: Vec<_> = member
        .jobs
        .iter()
        .filter(|job| job.status == AssignmentStatus::Queued)
        .collect();
    if !member.manager {
        jobs.sort_by_key(|job| {
            job.task_code
                .as_ref()
                .and_then(|code| member.task_queue.iter().position(|item| item == code))
                .unwrap_or(usize::MAX)
        });
    }
    // The sort is stable: arrival order is preserved within a task.
    jobs
}

fn validate_task_queue_order(room: &Workspace) -> Result<(), String> {
    // Whole-task completion is shared by the room. Opposite orders such as X->Y
    // and Y->X would make both employees wait forever, so reject that enqueue.
    let mut edges = HashMap::<String, HashSet<String>>::new();
    let mut incoming = HashMap::<String, usize>::new();
    for member in room
        .panels
        .iter()
        .filter_map(|p| p.member.as_ref())
        .filter(|m| !m.manager)
    {
        for code in &member.task_queue {
            incoming.entry(code.clone()).or_default();
        }
        for pair in member.task_queue.windows(2) {
            if edges
                .entry(pair[0].clone())
                .or_default()
                .insert(pair[1].clone())
            {
                *incoming.entry(pair[1].clone()).or_default() += 1;
            }
        }
    }
    let mut ready: Vec<String> = incoming
        .iter()
        .filter(|(_, n)| **n == 0)
        .map(|(code, _)| code.clone())
        .collect();
    let mut completed = 0;
    while let Some(code) = ready.pop() {
        completed += 1;
        if let Some(next) = edges.get(&code) {
            for target in next {
                let count = incoming.get_mut(target).unwrap();
                *count -= 1;
                if *count == 0 {
                    ready.push(target.clone());
                }
            }
        }
    }
    if completed != incoming.len() {
        return Err("Essa ordem criaria espera circular entre filas de tarefas. Mantenha uma ordem compatível para tarefas que envolvem as mesmas pessoas.".into());
    }
    Ok(())
}

fn needs_fresh_context(member: &TeamMember, job: &Assignment) -> bool {
    if member
        .context_resets
        .iter()
        .any(|r| r.id == format!("assignment-context:{}", job.id))
    {
        return false;
    }
    if job.training || member.manager {
        return job.fresh_context;
    }
    match &job.task_code {
        Some(code) => member.context_task_code.as_ref() != Some(code),
        None => job.fresh_context,
    }
}

impl AppState {
    pub(crate) fn valid_mcp_token(&self, terminal_id: &str, token: &str) -> Result<bool, String> {
        let persisted = guard(&self.persisted)?;
        let (_, workspace) = find_terminal_workspace(&persisted, terminal_id)?;
        if workspace.panel(terminal_id).unwrap().member.is_some() {
            Ok(guard(&self.member_tokens)?
                .get(terminal_id)
                .is_some_and(|expected| expected == token))
        } else {
            Ok(token == self.mcp_token)
        }
    }
    pub(crate) fn close_member_session(&self, terminal_id: &str) -> Result<(), String> {
        let _gate = guard(&self.office_gate)?;
        let mut persisted = guard(&self.persisted)?;
        if let Some(member) = &mut find_terminal_mut(&mut persisted, terminal_id)?.member {
            member.paused = true;
            if member.training == TrainingState::Learning {
                member.training = TrainingState::Awaiting;
            }
            for job in &mut member.jobs {
                if matches!(
                    job.status,
                    AssignmentStatus::Queued
                        | AssignmentStatus::Running
                        | AssignmentStatus::Uncertain
                ) {
                    job.status = AssignmentStatus::Interrupted;
                    job.result =
                        "Execução encerrada. Uma nova atribuição exige um novo pedido.".into();
                    job.updated_at = unix_timestamp()?;
                }
            }
            self.save(&persisted)?;
        }
        drop(persisted);
        guard(&self.office_sessions)?.remove(terminal_id);
        guard(&self.member_states)?.remove(terminal_id);
        guard(&self.member_tokens)?.remove(terminal_id);
        Ok(())
    }
    pub(crate) fn hire_agent(&self, input: HireAgentInput) -> Result<AppSnapshot, String> {
        let _gate = guard(&self.office_gate)?;
        let specialty = bounded(input.specialty, 120, "especialidade")?;
        let mut persisted = guard(&self.persisted)?;
        let workspace = find_workspace_mut(&mut persisted, &input.workspace_id)?;
        if workspace.kind != crate::models::WorkspaceKind::Team {
            return Err("Contratação disponível somente em workspaces do tipo Equipe.".into());
        }
        if workspace
            .panels
            .iter()
            .filter(|p| p.member.is_some())
            .count()
            >= 12
        {
            return Err("Esta sala já possui 12 pessoas.".into());
        }
        let manager = !workspace
            .panels
            .iter()
            .any(|p| p.member.as_ref().is_some_and(|m| m.manager));
        let execution = input.execution.unwrap_or_else(|| {
            if manager {
                AgentExecution::manager()
            } else {
                AgentExecution::default()
            }
        });
        execution.validate(manager)?;
        let id = new_id();
        let title = if input.name.trim().is_empty() {
            learning::generated_name(
                &id,
                &workspace
                    .panels
                    .iter()
                    .map(|p| p.title.clone())
                    .collect::<Vec<_>>(),
            )
        } else {
            name(&input.name, "agente")?
        };
        let instructions = if input.instructions.trim().is_empty() {
            learning::role_instructions(&specialty, manager)
        } else {
            bounded(input.instructions, 8000, "orientações")?
        };
        let personality = String::new();
        workspace.add_panel(
            TerminalPanel {
                id,
                title,
                label: None,
                profile_id: CODEX.into(),
                member: Some(TeamMember {
                    execution,
                    professional_profile: None,
                    automatic_name: input.name.trim().is_empty(),
                    personality,
                    memory: AgentMemory::default(),
                    soul: AgentSoul::default(),
                    training: TrainingState::Awaiting,
                    specialty,
                    instructions,
                    manager,
                    thread_id: None,
                    session_instructions_hash: None,
                    paused: false,
                    jobs: Vec::new(),
                    context_resets: Vec::new(),
                    context_task_code: None,
                    task_queue: Vec::new(),
                }),
            },
            None,
            SplitDirection::Vertical,
        )?;
        self.save(&persisted)?;
        drop(persisted);
        self.snapshot()
    }

    pub(crate) fn edit_agent(&self, input: EditAgentInput) -> Result<AppSnapshot, String> {
        let _gate = guard(&self.office_gate)?;
        self.require_inactive(&[input.terminal_id.clone()])?;
        if guard(&self.office_sessions)?.contains_key(&input.terminal_id) {
            return Err("Encerre a sessão antes de editar as orientações.".into());
        }
        let title = name(&input.name, "agente")?;
        let specialty = bounded(input.specialty, 120, "especialidade")?;
        let instructions = bounded(input.instructions, 8000, "orientações")?;
        let mut persisted = guard(&self.persisted)?;
        let panel = find_terminal_mut(&mut persisted, &input.terminal_id)?;
        let member = panel
            .member
            .as_mut()
            .ok_or("Este painel não é um membro da equipe.")?;
        let execution = input.execution.unwrap_or_else(|| member.execution.clone());
        execution.validate(member.manager)?;
        member.execution = execution;
        if panel.title != title {
            member.automatic_name = false;
        }
        panel.title = title;
        if member.specialty != specialty {
            member.training = TrainingState::Awaiting;
            member.professional_profile = None;
            member.soul = AgentSoul::default();
        }
        member.specialty = specialty;
        member.instructions = instructions;
        self.save(&persisted)?;
        drop(persisted);
        self.snapshot()
    }

    pub(crate) fn ensure_member_session(
        &self,
        app: &AppHandle,
        terminal_id: &str,
    ) -> Result<Arc<CodexSession>, String> {
        let _gate = guard(&self.office_gate)?;
        if let Some(session) = guard(&self.office_sessions)?.get(terminal_id).cloned() {
            return Ok(session);
        }
        let (root, workspace_id, panel) = {
            let persisted = guard(&self.persisted)?;
            let (area, workspace) = find_terminal_workspace(&persisted, terminal_id)?;
            (
                PathBuf::from(&area.root_path),
                workspace.id.clone(),
                workspace.panel(terminal_id).unwrap().clone(),
            )
        };
        let member = panel
            .member
            .as_ref()
            .ok_or("Este painel não é um membro da equipe.")?;
        member.execution.validate(member.manager)?;
        let mut arguments = office_mcp_arguments()?;
        arguments.extend(member.execution.cli_arguments());
        let area_document = area_instructions::prepare(&root)?;
        let session_instructions = instructions(&panel, &area_document);
        let instructions_hash = area_instructions::revision(&session_instructions);
        let token = new_id();
        guard(&self.member_tokens)?.insert(terminal_id.into(), token.clone());
        let environment = vec![
            (
                "KANASHA_MCP_SOCKET".into(),
                self.mcp_socket_path.to_string_lossy().into_owned(),
            ),
            ("KANASHA_MCP_TOKEN".into(), token),
            ("KANASHA_WORKSPACE_ID".into(), workspace_id),
            ("KANASHA_TERMINAL_ID".into(), terminal_id.into()),
        ];
        let state = self.clone();
        let app_for_event = app.clone();
        let id = terminal_id.to_owned();
        let session = CodexSession::start(
            &root,
            &arguments,
            &environment,
            member.thread_id.as_deref(),
            &session_instructions,
            Some(&member.execution),
            move |event| {
                if let Err(error) = state.member_event(&app_for_event, &id, event) {
                    eprintln!("Kanasha equipe: {error}");
                }
            },
        );
        let session = match session {
            Ok(session) => session,
            // Older sessions may have saved a thread id before its rollout existed.
            Err(error)
                if member.jobs.iter().all(|j| {
                    j.turn_id.is_none()
                        && matches!(
                            j.status,
                            AssignmentStatus::Queued | AssignmentStatus::Interrupted
                        )
                }) && error.contains("no rollout found") =>
            {
                let state = self.clone();
                let app_for_event = app.clone();
                let id = terminal_id.to_owned();
                CodexSession::start(
                    &root,
                    &arguments,
                    &environment,
                    None,
                    &session_instructions,
                    Some(&member.execution),
                    move |event| {
                        let _ = state.member_event(&app_for_event, &id, event);
                    },
                )?
            }
            Err(error) => return Err(error),
        };
        // Resume can retain developer messages from the previous rollout. Append the
        // new version explicitly, once per change, before the TUI or a job starts a turn.
        if member.thread_id.as_deref() == Some(session.thread_id.as_str())
            && member.session_instructions_hash.as_ref() != Some(&instructions_hash)
        {
            session.request("thread/inject_items", json!({
                "threadId": session.thread_id,
                "items": [{"type":"message", "role":"developer", "content":[{
                    "type":"input_text",
                    "text": format!("Atualização das instruções do KanashaTerminal. Esta versão substitui as orientações anteriores fornecidas pelo KanashaTerminal para esta pessoa e área. Preserve o histórico de trabalho e as demais regras aplicáveis do projeto.\n\n{session_instructions}")
                }]}]
            }))?;
        }
        let session = Arc::new(session);
        {
            let mut persisted = guard(&self.persisted)?;
            let member = find_terminal_mut(&mut persisted, terminal_id)?
                .member
                .as_mut()
                .unwrap();
            member.thread_id = Some(session.thread_id.clone());
            member.session_instructions_hash = Some(instructions_hash);
            member.paused = false;
            self.save(&persisted)?;
        }
        guard(&self.member_states)?.insert(
            terminal_id.into(),
            MemberRuntime {
                model: Some(session.model.clone()),
                reasoning_effort: Some(session.reasoning_effort.clone()),
                connected: true,
                activity: "Disponível".into(),
                instructions_revision: Some(area_document.revision),
                learning_hash: Some(area_instructions::revision(&learning::instructions(member))),
                updated_at: unix_timestamp()?,
                ..Default::default()
            },
        );
        guard(&self.office_sessions)?.insert(terminal_id.into(), session.clone());
        Ok(session)
    }

    pub(crate) fn enqueue_assignment(
        &self,
        input: AssignAgentInput,
        author: Option<(&str, &str)>,
    ) -> Result<String, String> {
        let task_code = input.task_code.map(task_code).transpose()?;
        if input.step_ids.len() > 20 {
            return Err("Uma atribuição aceita até 20 etapas vinculadas.".into());
        }
        let step_ids = input
            .step_ids
            .into_iter()
            .map(task_step_id)
            .collect::<Result<Vec<_>, _>>()?;
        if step_ids.iter().collect::<HashSet<_>>().len() != step_ids.len() {
            return Err("Não repita IDs de etapas na mesma atribuição.".into());
        }
        if !step_ids.is_empty() && task_code.is_none() {
            return Err("Informe taskCode para vincular etapas a uma atribuição.".into());
        }
        let message = bounded(input.message, 8000, "objetivo da atribuição")?;
        let request_id = bounded(
            input.request_id,
            if author.is_some() { 120 } else { 240 },
            "identificador da atribuição",
        )?;
        {
            let _gate = guard(&self.office_gate)?;
            let mut persisted = guard(&self.persisted)?;
            if !input.training {
                let (_, room) = find_terminal_workspace(&persisted, &input.terminal_id)?;
                if !room.panels.iter().any(|p| {
                    p.member
                        .as_ref()
                        .is_some_and(|m| m.manager && m.training == TrainingState::Ready)
                }) {
                    return Err("O usuário precisa revisar e concluir o treinamento do gerente antes de liberar o trabalho da equipe.".into());
                }
            }
            if let Some((workspace_id, author_id)) = author {
                let workspace = find_workspace(&persisted, workspace_id)?;
                let source = workspace
                    .panel(author_id)
                    .and_then(|p| p.member.as_ref())
                    .ok_or("O chamador não faz parte da equipe.")?;
                if !source.manager {
                    return Err("Somente o gerente pode delegar trabalho à equipe.".into());
                }
                if source.training != TrainingState::Ready {
                    return Err("O usuário precisa revisar e concluir o treinamento do gerente antes de liberar a equipe.".into());
                }
                if input.terminal_id == author_id || workspace.panel(&input.terminal_id).is_none() {
                    return Err("Escolha outro membro desta sala.".into());
                }
            }
            if !step_ids.is_empty() {
                let (_, room) = find_terminal_workspace(&persisted, &input.terminal_id)?;
                let task = selected_task(room, task_code.as_deref())?;
                if step_ids
                    .iter()
                    .any(|id| !task.steps.iter().any(|step| &step.id == id))
                {
                    return Err("Uma etapa informada não pertence a essa tarefa. Consulte get_task_steps com taskCode.".into());
                }
            }
            let mut candidate = persisted.clone();
            let member = find_terminal_mut(&mut candidate, &input.terminal_id)?
                .member
                .as_mut()
                .ok_or("O destinatário não faz parte da equipe.")?;
            let fresh_context = input.fresh_context.unwrap_or(false);
            let assigned_by = author.map(|(_, id)| id.to_owned());
            if let Some(existing) = member.jobs.iter().find(|j| j.id == request_id) {
                if existing.message != message
                    || existing.assigned_by != assigned_by
                    || existing.training != input.training
                    || existing.fresh_context != fresh_context
                    || existing.task_code != task_code
                    || existing.step_ids != step_ids
                {
                    return Err("Esse requestId já identifica outra atribuição.".into());
                }
                return Ok(existing.id.clone());
            }
            if let Some(code) = &task_code {
                let room = find_terminal_workspace(&persisted, &input.terminal_id)?.1;
                if room
                    .tasks
                    .iter()
                    .any(|task| &task.task_code == code && task.state.status == TaskStatus::Done)
                {
                    return Err("Esta tarefa já foi concluída. O gerente deve reabri-la com set_task antes de enfileirar trabalho novo.".into());
                }
            }
            if member.paused {
                return Err("Esta pessoa foi interrompida pelo usuário. Ele precisa iniciar a sessão novamente.".into());
            }
            if !input.training && member.training != TrainingState::Ready {
                return Err(
                    "Conclua o treinamento e a revisão desta pessoa antes de atribuir trabalho."
                        .into(),
                );
            }
            if input.training {
                if member.jobs.iter().any(|j| {
                    matches!(
                        j.status,
                        AssignmentStatus::Queued
                            | AssignmentStatus::Running
                            | AssignmentStatus::Uncertain
                    )
                }) || guard(&self.member_states)?
                    .get(&input.terminal_id)
                    .is_some_and(|r| r.turn_id.is_some())
                {
                    return Err(
                        "Conclua ou encerre a execução atual antes de iniciar outro treinamento."
                            .into(),
                    );
                }
                member.training = TrainingState::Learning;
            }
            if member
                .jobs
                .iter()
                .filter(|j| {
                    matches!(
                        j.status,
                        AssignmentStatus::Queued
                            | AssignmentStatus::Running
                            | AssignmentStatus::Uncertain
                    )
                })
                .count()
                >= 10
            {
                return Err("Esta pessoa já tem 10 atribuições pendentes.".into());
            }
            let now = unix_timestamp()?;
            if !member.manager {
                if let Some(code) = &task_code {
                    if !member.task_queue.contains(code) {
                        member.task_queue.push(code.clone());
                    }
                }
            }
            member.jobs.push(Assignment {
                work_contexts: Vec::new(),
                deliveries: Vec::new(),
                step_ids,
                task_code: task_code.clone(),
                fresh_context,
                thread_id: None,
                training: input.training,
                memory_revision_before: member.memory.revision,
                soul_revision_before: member.soul.revision,
                id: request_id.clone(),
                message,
                assigned_by,
                status: AssignmentStatus::Queued,
                result: String::new(),
                created_at: now,
                updated_at: now,
                turn_id: None,
            });
            if let Some(code) = task_code {
                let room_id = find_terminal_workspace(&candidate, &input.terminal_id)?
                    .1
                    .id
                    .clone();
                let room = find_workspace_mut(&mut candidate, &room_id)?;
                if !room.tasks.iter().any(|task| task.task_code == code) {
                    room.tasks.push(OfficeTask {
                        task_code: code,
                        state: TaskState::default(),
                    });
                }
            }
            validate_task_queue_order(find_terminal_workspace(&candidate, &input.terminal_id)?.1)?;
            self.save(&candidate)?;
            *persisted = candidate;
        }
        Ok(request_id)
    }

    pub(crate) fn assign_agent(
        &self,
        app: &AppHandle,
        input: AssignAgentInput,
        author: Option<(&str, &str)>,
    ) -> Result<String, String> {
        let terminal_id = input.terminal_id.clone();
        let request_id = self.enqueue_assignment(input, author)?;
        let _ = app.emit("task-state-changed", &terminal_id);
        self.wake_member(app, &terminal_id);
        Ok(request_id)
    }

    // Save the new-chat checkpoint before touching any live connection. Never use
    // close_member_session here: that operation intentionally cancels the queue.
    fn prepare_fresh_context(
        &self,
        app: Option<&AppHandle>,
        terminal_id: &str,
        request_id: &str,
        author: Option<(&str, &str)>,
        expected_job: Option<&str>,
    ) -> Result<bool, String> {
        let request_id = bounded(
            request_id.to_owned(),
            if expected_job.is_some() { 240 } else { 120 },
            "identificador da troca de conversa",
        )?;
        let request_id = if expected_job.is_none() {
            format!("manual-context:{request_id}")
        } else {
            request_id
        };
        let _terminal_gate = guard(&self.terminal_gate)?;
        let _office_gate = guard(&self.office_gate)?;
        let mut persisted = guard(&self.persisted)?;
        if let Some((room_id, source_id)) = author {
            let room = find_workspace(&persisted, room_id)?;
            let source = room
                .panel(source_id)
                .and_then(|p| p.member.as_ref())
                .ok_or("O chamador não pertence à equipe.")?;
            let target = room
                .panel(terminal_id)
                .and_then(|p| p.member.as_ref())
                .ok_or("Escolha outro membro desta sala.")?;
            if !source.manager
                || source.training != TrainingState::Ready
                || source_id == terminal_id
                || target.manager
            {
                return Err("Somente o gerente treinado pode iniciar uma nova conversa de outro funcionário da sala.".into());
            }
        }
        let room = find_terminal_workspace(&persisted, terminal_id)?.1;
        let member = room
            .panel(terminal_id)
            .and_then(|p| p.member.as_ref())
            .ok_or("Este painel não é um membro da equipe.")?;
        if member.context_resets.iter().any(|r| r.id == request_id) {
            return Ok(false);
        }
        if author.is_some()
            && expected_job.is_none()
            && member.context_task_code.as_ref().is_some_and(|code| {
                room.tasks
                    .iter()
                    .find(|task| &task.task_code == code)
                    .is_some_and(|task| task.state.status != TaskStatus::Done)
            })
        {
            return Err("Conclua a tarefa atual antes de trocar a conversa do funcionário. Mudar steps preserva o contexto.".into());
        }
        if member.paused {
            return Err("Esta pessoa foi interrompida pelo usuário. Disponibilize-a antes de iniciar outra conversa.".into());
        }
        if member.jobs.iter().any(|j| {
            matches!(
                j.status,
                AssignmentStatus::Running | AssignmentStatus::Uncertain
            )
        }) || guard(&self.member_states)?
            .get(terminal_id)
            .is_some_and(|r| r.turn_id.is_some() || r.request.is_some())
        {
            return Err(
                "Conclua a execução e resolva resultados incertos antes de iniciar outra conversa."
                    .into(),
            );
        }
        if let Some(id) = expected_job {
            let room = find_terminal_workspace(&persisted, terminal_id)?.1;
            if next_assignment_index(room, member).map(|index| member.jobs[index].id.as_str())
                != Some(id)
            {
                return Ok(false);
            }
        }
        let manual_task_code = current_queued_task(room, member).map(str::to_owned);
        let mut candidate = persisted.clone();
        let member = find_terminal_mut(&mut candidate, terminal_id)?
            .member
            .as_mut()
            .unwrap();
        member.context_resets.push(ContextReset {
            id: request_id,
            previous_thread_id: member.thread_id.take(),
            created_at: unix_timestamp()?,
        });
        member.session_instructions_hash = None;
        member.context_task_code = match expected_job {
            Some(id) => member
                .jobs
                .iter()
                .find(|j| j.id == id)
                .and_then(|j| j.task_code.clone()),
            None => manual_task_code,
        };
        self.save(&candidate)?;
        *persisted = candidate;
        drop(persisted);
        // Removing the connection first fences late Codex events and PTY exits.
        guard(&self.office_sessions)?.remove(terminal_id);
        guard(&self.member_states)?.remove(terminal_id);
        guard(&self.member_tokens)?.remove(terminal_id);
        guard(&self.agent_states)?.remove(terminal_id);
        // Emit before releasing terminal_gate so replacement output cannot arrive
        // before the frontend has scheduled clearing the old screen.
        if let Some(app) = app {
            let _ = app.emit(
                "terminal-context-reset",
                TerminalExit {
                    terminal_id: terminal_id.into(),
                },
            );
        }
        if let Some(mut pty) = guard(&self.sessions)?.remove(terminal_id) {
            pty.stop()?;
        }
        Ok(true)
    }

    /// Release only this employee's participation; keep the shared task and history.
    /// Fence retries with the latest completed assignment so an old call cannot release new work.
    pub(crate) fn release_task(
        &self,
        workspace_id: &str,
        author_id: &str,
        input: ReleaseTaskInput,
    ) -> Result<(), String> {
        let code = task_code(input.task_code)?;
        let _gate = guard(&self.office_gate)?;
        let mut persisted = guard(&self.persisted)?;
        let room = find_workspace(&persisted, workspace_id)?;
        let source = room
            .panel(author_id)
            .and_then(|p| p.member.as_ref())
            .ok_or("O chamador não pertence à equipe.")?;
        let target = room
            .panel(&input.terminal_id)
            .and_then(|p| p.member.as_ref())
            .ok_or("Escolha outro funcionário desta sala.")?;
        if !source.manager
            || source.training != TrainingState::Ready
            || target.manager
            || input.terminal_id == author_id
        {
            return Err(
                "Somente o gerente treinado pode liberar a participação de outro funcionário."
                    .into(),
            );
        }
        let latest = target
            .jobs
            .iter()
            .rev()
            .find(|job| job.task_code.as_deref() == Some(code.as_str()))
            .ok_or("O funcionário não tem atribuições nessa tarefa.")?;
        if latest.id != input.assignment_id || latest.status != AssignmentStatus::Done {
            return Err("Confira a última entrega concluída: a atribuição informada está pendente ou foi substituída.".into());
        }
        if !target.task_queue.contains(&code) {
            return Ok(());
        }
        if current_queued_task(room, target) != Some(code.as_str()) {
            return Err("Só é possível liberar a participação na tarefa atual da fila.".into());
        }
        if target.paused || target.training != TrainingState::Ready {
            return Err("O funcionário está pausado ou em treinamento.".into());
        }
        if target.jobs.iter().any(|job| {
            matches!(
                job.status,
                AssignmentStatus::Running | AssignmentStatus::Uncertain
            ) || (job.task_code.as_deref() == Some(code.as_str())
                && job.status == AssignmentStatus::Queued)
        }) || guard(&self.member_states)?
            .get(&input.terminal_id)
            .is_some_and(|r| r.turn_id.is_some() || r.request.is_some())
        {
            return Err("Conclua o trabalho do funcionário e resolva perguntas ou resultados incertos antes de liberá-lo.".into());
        }
        // Step status belongs to the shared task, not to an individual assignee.
        // The manager confirms this employee's participation is finished by
        // releasing their latest delivery; other employees may still own work
        // on the same steps. Keep the task, step status and historical links intact.
        let mut candidate = persisted.clone();
        find_terminal_mut(&mut candidate, &input.terminal_id)?
            .member
            .as_mut()
            .unwrap()
            .task_queue
            .retain(|queued| queued != &code);
        self.save(&candidate)?;
        *persisted = candidate;
        Ok(())
    }

    pub(crate) fn reset_agent_context(
        &self,
        app: &AppHandle,
        input: ResetContextInput,
        author: Option<(&str, &str)>,
    ) -> Result<(), String> {
        let id = &input.terminal_id;
        self.prepare_fresh_context(Some(app), id, &input.request_id, author, None)?;
        let result = self.start_terminal_with_mode(app.clone(), id, false);
        let _ = app.emit("task-state-changed", id);
        result?;
        self.wake_member(app, id);
        Ok(())
    }

    pub(crate) fn wake_member(&self, app: &AppHandle, terminal_id: &str) {
        let state = self.clone();
        let app = app.clone();
        let id = terminal_id.to_owned();
        thread::spawn(move || {
            state.deliver_next(&app, &id);
        });
    }

    fn deliver_next(&self, app: &AppHandle, terminal_id: &str) {
        let outcome = (|| -> Result<(), String> {
            let context_job = {
                let persisted = guard(&self.persisted)?;
                let (_, workspace) = find_terminal_workspace(&persisted, terminal_id)?;
                let member = workspace
                    .panel(terminal_id)
                    .and_then(|p| p.member.as_ref())
                    .ok_or("Membro não encontrado.")?;
                let Some(index) = next_assignment_index(workspace, member) else {
                    return Ok(());
                };
                if member.training != TrainingState::Ready && !member.jobs[index].training {
                    return Err("Trabalho aguardando conclusão do treinamento.".into());
                }
                if member.jobs.iter().any(|j| {
                    matches!(
                        j.status,
                        AssignmentStatus::Running | AssignmentStatus::Uncertain
                    )
                }) || guard(&self.member_states)?
                    .get(terminal_id)
                    .is_some_and(|r| r.turn_id.is_some() || r.request.is_some())
                {
                    return Ok(());
                }
                if member.paused
                    || !member
                        .jobs
                        .iter()
                        .any(|j| j.status == AssignmentStatus::Queued)
                {
                    return Ok(());
                }
                needs_fresh_context(member, &member.jobs[index])
                    .then(|| member.jobs[index].id.clone())
            };
            if let Some(id) = context_job {
                self.prepare_fresh_context(
                    Some(app),
                    terminal_id,
                    &format!("assignment-context:{id}"),
                    None,
                    Some(&id),
                )?;
            }
            self.start_terminal_with_mode(app.clone(), terminal_id, false)?;
            let _gate = guard(&self.office_gate)?;
            if guard(&self.member_states)?
                .get(terminal_id)
                .is_some_and(|s| s.turn_id.is_some())
            {
                return Ok(());
            }
            let session = guard(&self.office_sessions)?
                .get(terminal_id)
                .cloned()
                .ok_or("A sessão não está conectada.")?;
            let learned = {
                let persisted = guard(&self.persisted)?;
                let (_, workspace) = find_terminal_workspace(&persisted, terminal_id)?;
                learning::instructions(
                    workspace
                        .panel(terminal_id)
                        .unwrap()
                        .member
                        .as_ref()
                        .unwrap(),
                )
            };
            let learning_hash = area_instructions::revision(&learned);
            if guard(&self.member_states)?
                .get(terminal_id)
                .and_then(|r| r.learning_hash.as_ref())
                != Some(&learning_hash)
            {
                session.request("thread/inject_items", json!({"threadId":session.thread_id,"items":[{"type":"message","role":"developer","content":[{"type":"input_text","text":format!("Atualização do aprendizado desta pessoa. Substitui o estado de treinamento, a alma e a memória anteriormente informados pelo KanashaTerminal.\n{learned}")}]}]}))?;
                guard(&self.member_states)?
                    .entry(terminal_id.into())
                    .or_default()
                    .learning_hash = Some(learning_hash);
            }
            let message = {
                let mut persisted = guard(&self.persisted)?;
                let room = find_terminal_workspace(&persisted, terminal_id)?.1;
                let member = room.panel(terminal_id).unwrap().member.as_ref().unwrap();
                let Some(index) = next_assignment_index(room, member) else {
                    return Ok(());
                };
                let mut candidate = persisted.clone();
                let member = find_terminal_mut(&mut candidate, terminal_id)?
                    .member
                    .as_mut()
                    .unwrap();
                if member.paused
                    || member.jobs.iter().any(|j| {
                        matches!(
                            j.status,
                            AssignmentStatus::Running | AssignmentStatus::Uncertain
                        )
                    })
                {
                    return Ok(());
                }
                // A competing wake may have advanced the queue while we opened
                // the PTY. Re-evaluate the actual next job before sending a turn.
                if needs_fresh_context(member, &member.jobs[index]) {
                    drop(persisted);
                    drop(_gate);
                    self.wake_member(app, terminal_id);
                    return Ok(());
                }
                let job = &mut member.jobs[index];
                if !job.training && member.training != TrainingState::Ready {
                    return Err("Trabalho aguardando conclusão do treinamento.".into());
                }
                member.context_task_code =
                    job.task_code.clone().or(member.context_task_code.clone());
                job.thread_id = Some(session.thread_id.clone());
                job.status = AssignmentStatus::Running;
                job.updated_at = unix_timestamp()?;
                let message = format!(
                    "{} {}{}\n\n{}",
                    if job.training {
                        "Treinamento: consolide a alma com set_soul e registre uma lembrança curta com set_memory antes de finalizar. Atribuição"
                    } else {
                        "Atribuição"
                    },
                    job.id,
                    job.task_code
                        .as_ref()
                        .map(|code| format!(
                            "\nTarefa Jira: {code}. Steps desta tarefa compartilham a conversa."
                        ))
                        .unwrap_or_default(),
                    job.message
                );
                self.save(&candidate)?;
                *persisted = candidate;
                message
            };
            let execution = {
                let persisted = guard(&self.persisted)?;
                let (_, workspace) = find_terminal_workspace(&persisted, terminal_id)?;
                workspace
                    .panel(terminal_id)
                    .unwrap()
                    .member
                    .as_ref()
                    .unwrap()
                    .execution
                    .clone()
            };
            let response = session.request(
                "turn/start",
                execution.turn_params(&session.thread_id, &message),
            );
            let mut persisted = guard(&self.persisted)?;
            let member = find_terminal_mut(&mut persisted, terminal_id)?
                .member
                .as_mut()
                .unwrap();
            let job = member
                .jobs
                .iter_mut()
                .find(|j| j.status == AssignmentStatus::Running)
                .ok_or("Atribuição não encontrada.")?;
            match response {
                Ok(value) => {
                    let turn_id = value["turn"]["id"]
                        .as_str()
                        .ok_or("O Codex não identificou o turno recebido.")?
                        .to_owned();
                    job.turn_id = Some(turn_id.clone());
                    let mut states = guard(&self.member_states)?;
                    let state = states.entry(terminal_id.into()).or_default();
                    state.turn_id = Some(turn_id);
                    state.activity = "Executando atribuição".into();
                }
                Err(error) => {
                    job.status = AssignmentStatus::Uncertain;
                    job.result = format!(
                        "Entrega sem confirmação: {error}. Confira a sessão antes de repetir."
                    );
                    self.save(&persisted)?;
                    return Err(error);
                }
            }
            self.save(&persisted)?;
            Ok(())
        })();
        if let Err(error) = outcome {
            if let Ok(mut states) = self.member_states.lock() {
                states.entry(terminal_id.into()).or_default().activity = error;
            }
        }
        let _ = app.emit("task-state-changed", terminal_id);
    }

    fn member_event(&self, app: &AppHandle, terminal_id: &str, event: Value) -> Result<(), String> {
        let method = event["method"].as_str().unwrap_or("");
        if event.get("id").is_none()
            && !matches!(
                method,
                "turn/started"
                    | "turn/completed"
                    | "item/started"
                    | "item/completed"
                    | "kanasha/disconnected"
            )
        {
            return Ok(());
        }
        let _gate = guard(&self.office_gate)?;
        // Late shutdown events from an old connection must not overwrite a replacement.
        let session = guard(&self.office_sessions)?.get(terminal_id).cloned();
        let Some(session) = session else {
            return Ok(());
        };
        if event["_connection"].as_str() != Some(&session.connection_id) {
            return Ok(());
        }
        if let Some(id) = event.get("id") {
            if !matches!(
                method,
                "item/tool/requestUserInput"
                    | "item/commandExecution/requestApproval"
                    | "item/fileChange/requestApproval"
            ) {
                session.respond_error(id.clone(), "Esta interação ainda não é suportada pelo KanashaTerminal. Informe ao usuário o impedimento.")?;
                return Ok(());
            }
            if guard(&self.member_states)?
                .get(terminal_id)
                .is_some_and(|s| s.request.is_some())
            {
                session.respond_error(id.clone(), "Já existe uma pergunta pendente. Aguarde sua resposta antes de solicitar outra.")?;
                return Ok(());
            }
        }
        let params = &event["params"];
        if let Some(thread_id) = params["threadId"].as_str() {
            if thread_id != session.thread_id {
                return Ok(());
            }
        }
        let now = unix_timestamp()?;
        let mut notification = None;
        let mut next = false;
        let training_baseline = if method == "turn/started" {
            let persisted = guard(&self.persisted)?;
            let (_, workspace) = find_terminal_workspace(&persisted, terminal_id)?;
            let member = workspace
                .panel(terminal_id)
                .unwrap()
                .member
                .as_ref()
                .unwrap();
            (member.training != TrainingState::Ready)
                .then_some((member.memory.revision, member.soul.revision))
        } else {
            None
        };
        {
            let mut states = guard(&self.member_states)?;
            let state = states.entry(terminal_id.into()).or_default();
            state.updated_at = now;
            match method {
                "kanasha/disconnected" => {
                    state.connected = false;
                    state.turn_id = None;
                    state.request = None;
                    state.activity = "Conexão encerrada. Inicie a sessão para reconectar.".into();
                }
                "turn/started" => {
                    state.training_memory_revision_before = training_baseline.map(|r| r.0);
                    state.training_soul_revision_before = training_baseline.map(|r| r.1);
                    state.turn_id = params["turn"]["id"].as_str().map(str::to_owned);
                    state.activity = "Trabalhando".into();
                }
                "turn/completed" => {
                    state.turn_id = None;
                    state.request = None;
                    state.activity = "Turno encerrado".into();
                    next = true;
                }
                "item/started" => {
                    state.activity = match params["item"]["type"].as_str().unwrap_or("") {
                        "commandExecution" => format!(
                            "Executando: {}",
                            params["item"]["command"].as_str().unwrap_or("comando")
                        ),
                        "fileChange" => "Editando arquivos".into(),
                        "mcpToolCall" => format!(
                            "Usando {}",
                            params["item"]["tool"].as_str().unwrap_or("ferramenta")
                        ),
                        _ => "Trabalhando".into(),
                    }
                    .chars()
                    .take(250)
                    .collect();
                }
                _ if event.get("id").is_some() => {
                    state.request = Some(event.clone());
                    state.activity = "Aguardando sua resposta no painel Equipe".into();
                }
                _ => {}
            }
        }
        if matches!(
            method,
            "turn/completed" | "item/completed" | "kanasha/disconnected"
        ) {
            let mut persisted = guard(&self.persisted)?;
            let panel = find_terminal_mut(&mut persisted, terminal_id)?;
            let title = panel.title.clone();
            let member = panel.member.as_mut().ok_or("Membro não encontrado.")?;
            let mut changed = false;
            if let Some(job) = member
                .jobs
                .iter_mut()
                .find(|j| j.status == AssignmentStatus::Running)
            {
                let event_turn = params["turnId"]
                    .as_str()
                    .or_else(|| params["turn"]["id"].as_str());
                if method == "kanasha/disconnected" {
                    job.status = AssignmentStatus::Uncertain;
                    job.result = "Conexão encerrada durante a execução. Confira o histórico antes de repetir.".into();
                    changed = true;
                } else if event_turn == job.turn_id.as_deref() {
                    if method == "item/completed" && params["item"]["type"] == "agentMessage" {
                        job.result = params["item"]["text"]
                            .as_str()
                            .unwrap_or("")
                            .chars()
                            .take(8000)
                            .collect();
                        changed = true;
                    } else if method == "turn/completed" {
                        job.status = match params["turn"]["status"].as_str() {
                            Some("completed") => AssignmentStatus::Done,
                            Some("interrupted") => AssignmentStatus::Interrupted,
                            _ => AssignmentStatus::Failed,
                        };
                        if job.status == AssignmentStatus::Failed {
                            job.result = params["turn"]["error"]["message"]
                                .as_str()
                                .unwrap_or("A execução falhou.")
                                .chars()
                                .take(8000)
                                .collect();
                        }
                        job.updated_at = now;
                        if let Some(manager) = &job.assigned_by {
                            notification = Some((manager.clone(), format!("result-{terminal_id}-{}", job.id), format!("Resultado da atribuição {} de {}. Estado da execução: {:?}. {} Consulte get_deliveries com terminalId={} e assignmentId={} antes de atualizar resumo, status ou Jira.\n\n{}", job.id, title, job.status, if job.deliveries.is_empty() { "Entrega estruturada não registrada; isso não comprova ausência de trabalho." } else { "Entrega estruturada disponível para conferência da versão atual." }, terminal_id, job.id, job.result).chars().take(8000).collect::<String>()));
                        }
                        changed = true;
                    }
                }
            } else if method == "turn/completed"
                && params["turn"]["status"] == "completed"
                && member.training != TrainingState::Ready
            {
                let runtime = guard(&self.member_states)?
                    .get(terminal_id)
                    .cloned()
                    .unwrap_or_default();
                if learning::training_recorded(
                    member,
                    runtime.training_memory_revision_before,
                    runtime.training_soul_revision_before,
                ) {
                    member.training = TrainingState::Review;
                    changed = true;
                }
            }
            if changed {
                if method == "turn/completed" || method == "kanasha/disconnected" {
                    learning::finish_training(member);
                }
                self.save(&persisted)?;
            }
        }
        // Mirror runtime status into the existing workspace indicators.
        {
            let runtime = guard(&self.member_states)?
                .get(terminal_id)
                .cloned()
                .unwrap_or_default();
            let mut states = guard(&self.agent_states)?;
            let state = states.entry(terminal_id.into()).or_default();
            state.status = if runtime.request.is_some() {
                AgentStatus::Waiting
            } else if runtime.turn_id.is_some() {
                AgentStatus::Working
            } else {
                AgentStatus::Idle
            };
            state.current = runtime.activity;
            state.updated_at = now;
        }
        drop(_gate);
        if let Some((manager, request_id, message)) = notification {
            // Internal result delivery uses no assigned_by, preventing a reply loop.
            let _ = self.assign_agent(
                app,
                AssignAgentInput {
                    step_ids: Vec::new(),
                    task_code: None,
                    fresh_context: None,
                    training: false,
                    terminal_id: manager,
                    request_id,
                    message,
                },
                None,
            );
        }
        let _ = app.emit("task-state-changed", terminal_id);
        if next {
            self.deliver_next(app, terminal_id);
        }
        Ok(())
    }

    pub(crate) fn interrupt_agent(&self, terminal_id: &str) -> Result<AppSnapshot, String> {
        let _terminal_gate = guard(&self.terminal_gate)?;
        let _gate = guard(&self.office_gate)?;
        self.pause_member(terminal_id)?;
        let session = guard(&self.office_sessions)?.get(terminal_id).cloned();
        let turn = guard(&self.member_states)?
            .get(terminal_id)
            .and_then(|s| s.turn_id.clone());
        if let (Some(session), Some(turn)) = (session, turn) {
            session.request(
                "turn/interrupt",
                json!({"threadId":session.thread_id,"turnId":turn}),
            )?;
        }
        self.snapshot()
    }

    pub(crate) fn pause_member(&self, terminal_id: &str) -> Result<(), String> {
        let mut persisted = guard(&self.persisted)?;
        if let Some(member) = &mut find_terminal_mut(&mut persisted, terminal_id)?.member {
            member.paused = true;
            for job in &mut member.jobs {
                if job.status == AssignmentStatus::Queued {
                    job.status = AssignmentStatus::Interrupted;
                    job.result = "Atribuição cancelada pelo usuário antes de iniciar.".into();
                    job.updated_at = unix_timestamp()?;
                }
            }
            learning::finish_training(member);
            self.save(&persisted)?;
        }
        Ok(())
    }

    pub(crate) fn answer_agent(
        &self,
        terminal_id: &str,
        request_id: Value,
        result: Value,
    ) -> Result<AppSnapshot, String> {
        let session = guard(&self.office_sessions)?
            .get(terminal_id)
            .cloned()
            .ok_or("A sessão não está conectada.")?;
        let mut states = guard(&self.member_states)?;
        let state = states
            .get_mut(terminal_id)
            .ok_or("Sessão não encontrada.")?;
        if state.request.as_ref().and_then(|r| r.get("id")) != Some(&request_id) {
            return Err("Essa pergunta não está mais pendente.".into());
        }
        session.respond(request_id, result)?;
        state.request = None;
        drop(states);
        self.snapshot()
    }

    pub(crate) fn team_context(
        &self,
        workspace_id: &str,
        terminal_id: &str,
        target: Option<&str>,
    ) -> Result<Value, String> {
        let persisted = guard(&self.persisted)?;
        let workspace = find_workspace(&persisted, workspace_id)?;
        if workspace.panel(terminal_id).is_none() {
            return Err("O chamador não pertence a esta sala.".into());
        }
        let states = guard(&self.member_states)?;
        let people: Vec<Value> = workspace.panels.iter().filter_map(|panel| panel.member.as_ref().map(|member| (panel, member))).filter(|(panel, _)| target.is_none_or(|id| panel.id == id)).map(|(panel, member)| {
            let mut view = json!({"id":panel.id,"name":panel.title,"specialty":member.specialty,"execution":member.execution,"manager":member.manager,"personality":member.personality,"training":member.training,"memoryRevision":member.memory.revision,"soulRevision":member.soul.revision,"approvedSoulRevision":member.soul.approved.as_ref().map(|s| s.revision),"paused":member.paused,"runtime":states.get(&panel.id),"queued":member.jobs.iter().filter(|j| j.status == AssignmentStatus::Queued).count(),"contextTaskCode":member.context_task_code,"maxPending":10,"pending":member.jobs.iter().filter(|j| matches!(j.status, AssignmentStatus::Queued | AssignmentStatus::Running | AssignmentStatus::Uncertain)).count(),"taskQueue":member.task_queue.iter().filter(|code| workspace.tasks.iter().find(|task| &task.task_code == *code).is_none_or(|task| task.state.status != TaskStatus::Done)).map(|code| json!({"taskCode":code,"current":current_queued_task(workspace, member) == Some(code.as_str()),"steps":queued_assignments(member).into_iter().filter(|job| job.task_code.as_ref() == Some(code)).map(|job| json!({"assignmentId":job.id,"stepIds":job.step_ids,"status":job.status})).collect::<Vec<_>>()})).collect::<Vec<_>>(),"queue":queued_assignments(member).into_iter().map(|j| json!({"id":j.id,"message":j.message.chars().take(if target.is_some() { 800 } else { 160 }).collect::<String>(),"freshContext":j.fresh_context,"taskCode":j.task_code,"stepIds":j.step_ids})).collect::<Vec<_>>(),"currentAssignment":member.jobs.iter().find(|j| matches!(j.status, AssignmentStatus::Running | AssignmentStatus::Uncertain)).map(crate::deliveries::assignment_view)});
            view["deliveries"] = json!(member.jobs.iter().rev().filter(|j| !j.deliveries.is_empty()).take(20).map(crate::deliveries::summary).collect::<Vec<_>>());
            view["releasedTasks"] = json!(workspace.tasks.iter().filter(|task| !member.manager && task.state.status != TaskStatus::Done && !member.task_queue.contains(&task.task_code) && member.jobs.iter().rev().find(|job| job.task_code.as_ref() == Some(&task.task_code)).is_some_and(|job| job.status == AssignmentStatus::Done)).map(|task| &task.task_code).collect::<Vec<_>>());
            if target.is_some() { view["contextResets"] = json!(member.context_resets); view["professionalProfile"] = json!(member.professional_profile); view["instructions"] = json!(member.instructions); view["assignments"] = json!(member.jobs.iter().rev().take(1).map(crate::deliveries::assignment_view).collect::<Vec<_>>()); }
            view
        }).collect();
        if target.is_some() {
            people
                .into_iter()
                .next()
                .ok_or("Pessoa não encontrada nesta sala.".into())
        } else {
            let mut tasks = std::collections::BTreeMap::<String, Vec<Value>>::new();
            for panel in &workspace.panels {
                if let Some(member) = panel.member.as_ref().filter(|m| !m.manager) {
                    for job in &member.jobs {
                        if let Some(code) = &job.task_code {
                            tasks.entry(code.clone()).or_default().push(json!({"terminalId":panel.id,"name":panel.title,"assignmentId":job.id,"stepIds":job.step_ids,"status":job.status}));
                        }
                    }
                }
            }
            Ok(
                json!({"members":people,"tasks":tasks.into_iter().map(|(code, assignments)| json!({"taskCode":code,"assignments":assignments})).collect::<Vec<_>>()}),
            )
        }
    }
}

fn office_mcp_arguments() -> Result<Vec<String>, String> {
    let mut args = codex_mcp_arguments()?;
    for arg in &mut args {
        if arg.starts_with("mcp_servers.kanasha_terminal.enabled_tools=") {
            *arg = "mcp_servers.kanasha_terminal.enabled_tools=[\"get_task\",\"get_task_steps\",\"set_task\",\"get_team\",\"get_agent\",\"get_tasks\",\"set_assignment\",\"reset_context\",\"release_task\",\"register_work_context\",\"submit_delivery\",\"get_deliveries\",\"review_delivery\",\"get_memory\",\"get_soul\",\"set_memory\",\"set_soul\",\"set_professional_profile\",\"set_training_complete\"]".into();
        }
    }
    args.extend([
        "-c".into(),
        "mcp_servers.kanasha_terminal.tools.get_memory.output_token_limit=32768".into(),
        "-c".into(),
        "mcp_servers.kanasha_terminal.tools.get_soul.output_token_limit=32768".into(),
    ]);
    Ok(args)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn release_fixture() -> Fixture {
        let f = Fixture::new();
        f.state
            .task_context_for(
                &f.workspace,
                &f.manager,
                Some("B2C-X"),
                Some(TaskPatch {
                    status: Some(TaskStatus::InProgress),
                    steps: Some(vec![
                        TaskStep {
                            id: "implement".into(),
                            title: "Implementar".into(),
                            status: TaskStatus::Done,
                        },
                        TaskStep {
                            id: "review".into(),
                            title: "Revisar".into(),
                            status: TaskStatus::NotStarted,
                        },
                    ]),
                    ..Default::default()
                }),
            )
            .unwrap();
        for (id, code) in [("deliver-X", "B2C-X"), ("work-Y", "B2C-Y")] {
            let mut input = f.assignment(id);
            input.task_code = Some(code.into());
            if code == "B2C-X" {
                input.step_ids = vec!["implement".into()];
            }
            f.state
                .enqueue_assignment(input, Some((&f.workspace, &f.manager)))
                .unwrap();
        }
        {
            let mut p = guard(&f.state.persisted).unwrap();
            let member = find_terminal_mut(&mut p, &f.worker)
                .unwrap()
                .member
                .as_mut()
                .unwrap();
            member.jobs[0].status = AssignmentStatus::Done;
            member.context_task_code = Some("B2C-X".into());
            member.thread_id = Some("thread-X".into());
            f.state.save(&p).unwrap();
        }
        f
    }

    fn release_input(f: &Fixture) -> ReleaseTaskInput {
        ReleaseTaskInput {
            terminal_id: f.worker.clone(),
            task_code: "B2C-X".into(),
            assignment_id: "deliver-X".into(),
        }
    }

    #[test]
    fn release_advances_only_author_and_requeues_corrections_without_interrupting_next_task() {
        let f = release_fixture();
        let before = f.state.snapshot().unwrap();
        f.state
            .release_task(&f.workspace, &f.manager, release_input(&f))
            .unwrap();
        f.state
            .release_task(&f.workspace, &f.manager, release_input(&f))
            .unwrap(); // Safe retry.
        let restored = AppState::load(f.path.clone()).unwrap().snapshot().unwrap();
        let room = &restored.areas[0].workspaces[0];
        let member = room.panel(&f.worker).unwrap().member.as_ref().unwrap();
        let previous = before.areas[0].workspaces[0]
            .panel(&f.worker)
            .unwrap()
            .member
            .as_ref()
            .unwrap();
        assert_eq!(room.tasks, before.areas[0].workspaces[0].tasks);
        assert_eq!(member.task_queue, ["B2C-Y"]);
        assert_eq!(next_assignment_index(room, member), Some(1));
        assert!(needs_fresh_context(member, &member.jobs[1]));
        assert_eq!(member.jobs, previous.jobs);
        assert_eq!(member.soul, previous.soul);
        assert_eq!(member.memory, previous.memory);
        assert_eq!(member.thread_id, previous.thread_id);
        assert!(!member.paused);
        assert!(f
            .state
            .task_context_for(
                &f.workspace,
                &f.manager,
                Some("B2C-X"),
                Some(TaskPatch {
                    status: Some(TaskStatus::Done),
                    ..Default::default()
                })
            )
            .is_err());
        {
            let mut p = guard(&f.state.persisted).unwrap();
            let member = find_terminal_mut(&mut p, &f.worker)
                .unwrap()
                .member
                .as_mut()
                .unwrap();
            member.jobs[1].status = AssignmentStatus::Running;
            member.context_task_code = Some("B2C-Y".into());
            member.thread_id = Some("thread-Y".into());
        }
        let mut correction = f.assignment("fix-X");
        correction.task_code = Some("B2C-X".into());
        f.state
            .enqueue_assignment(correction, Some((&f.workspace, &f.manager)))
            .unwrap();
        assert!(f
            .state
            .release_task(&f.workspace, &f.manager, release_input(&f))
            .is_err()); // Old receipt cannot release corrections.
        let snapshot = f.state.snapshot().unwrap();
        let member = snapshot.areas[0].workspaces[0]
            .panel(&f.worker)
            .unwrap()
            .member
            .as_ref()
            .unwrap();
        assert_eq!(member.task_queue, ["B2C-Y", "B2C-X"]);
        assert_eq!(member.jobs[1].status, AssignmentStatus::Running);
        assert_eq!(member.thread_id.as_deref(), Some("thread-Y"));
        assert_eq!(member.jobs[2].status, AssignmentStatus::Queued);
    }

    #[test]
    fn release_rejects_pending_work_paused_people_and_wrong_authors() {
        let f = release_fixture();
        assert!(f
            .state
            .release_task(&f.workspace, &f.worker, release_input(&f))
            .is_err());
        assert!(f
            .state
            .release_task("another-room", &f.manager, release_input(&f))
            .is_err());
        for status in [
            AssignmentStatus::Queued,
            AssignmentStatus::Running,
            AssignmentStatus::Uncertain,
            AssignmentStatus::Failed,
        ] {
            {
                let mut p = guard(&f.state.persisted).unwrap();
                find_terminal_mut(&mut p, &f.worker)
                    .unwrap()
                    .member
                    .as_mut()
                    .unwrap()
                    .jobs[0]
                    .status = status;
            }
            assert!(f
                .state
                .release_task(&f.workspace, &f.manager, release_input(&f))
                .is_err());
        }
        {
            let mut p = guard(&f.state.persisted).unwrap();
            let member = find_terminal_mut(&mut p, &f.worker)
                .unwrap()
                .member
                .as_mut()
                .unwrap();
            member.jobs[0].status = AssignmentStatus::Done;
            member.paused = true;
        }
        assert!(f
            .state
            .release_task(&f.workspace, &f.manager, release_input(&f))
            .is_err());
        {
            let mut p = guard(&f.state.persisted).unwrap();
            find_terminal_mut(&mut p, &f.worker)
                .unwrap()
                .member
                .as_mut()
                .unwrap()
                .paused = false;
        }
        for runtime in [
            MemberRuntime {
                turn_id: Some("active-turn".into()),
                ..Default::default()
            },
            MemberRuntime {
                request: Some(json!({"id": "question"})),
                ..Default::default()
            },
        ] {
            guard(&f.state.member_states)
                .unwrap()
                .insert(f.worker.clone(), runtime);
            assert!(f
                .state
                .release_task(&f.workspace, &f.manager, release_input(&f))
                .is_err());
        }
        assert_eq!(
            f.state.snapshot().unwrap().areas[0].workspaces[0]
                .panel(&f.worker)
                .unwrap()
                .member
                .as_ref()
                .unwrap()
                .task_queue,
            ["B2C-X", "B2C-Y"]
        );
    }

    #[test]
    fn release_open_shared_step_advances_employee_without_changing_other_workers() {
        for next_has_task_code in [false, true] {
            let f = release_fixture();
            let other_id = new_id();
            {
                let mut persisted = guard(&f.state.persisted).unwrap();
                let room = find_workspace_mut(&mut persisted, &f.workspace).unwrap();
                room.tasks[0].state.steps[0].status = TaskStatus::InProgress;
                let worker = room.panel_mut(&f.worker).unwrap();
                worker.title = "Clara".into();
                let member = worker.member.as_mut().unwrap();
                if !next_has_task_code {
                    member.jobs[1].task_code = None;
                    member.jobs[1].message = "Orientação para a próxima atividade".into();
                    member.task_queue.retain(|code| code == "B2C-X");
                }
                let mut other = worker.clone();
                other.id = other_id.clone();
                other.title = "Davi".into();
                let other_member = other.member.as_mut().unwrap();
                other_member.jobs.truncate(1);
                other_member.jobs[0].id = "measure-X".into();
                other_member.jobs[0].status = AssignmentStatus::Running;
                other_member.jobs[0].turn_id = Some("measurement-turn".into());
                other_member.task_queue = vec!["B2C-X".into()];
                room.add_panel(other, None, SplitDirection::Vertical)
                    .unwrap();
                let member = room.panel(&f.worker).unwrap().member.as_ref().unwrap();
                assert_eq!(next_assignment_index(room, member), None);
                f.state.save(&persisted).unwrap();
            }
            guard(&f.state.member_states).unwrap().insert(
                other_id.clone(),
                MemberRuntime {
                    turn_id: Some("measurement-turn".into()),
                    ..Default::default()
                },
            );
            let before = f.state.snapshot().unwrap();
            f.state
                .release_task(&f.workspace, &f.manager, release_input(&f))
                .unwrap();
            f.state
                .release_task(&f.workspace, &f.manager, release_input(&f))
                .unwrap();
            let after = f.state.snapshot().unwrap();
            let room = &after.areas[0].workspaces[0];
            let previous = &before.areas[0].workspaces[0];
            let member = room.panel(&f.worker).unwrap().member.as_ref().unwrap();
            assert_eq!(room.tasks, previous.tasks);
            assert_eq!(room.panel(&other_id), previous.panel(&other_id));
            assert_eq!(
                member.jobs,
                previous
                    .panel(&f.worker)
                    .unwrap()
                    .member
                    .as_ref()
                    .unwrap()
                    .jobs
            );
            assert!(!member.task_queue.iter().any(|code| code == "B2C-X"));
            assert_eq!(next_assignment_index(room, member), Some(1));
            assert_eq!(
                guard(&f.state.member_states).unwrap()[&other_id]
                    .turn_id
                    .as_deref(),
                Some("measurement-turn")
            );
            let restored = AppState::load(f.path.clone()).unwrap().snapshot().unwrap();
            let restored_room = &restored.areas[0].workspaces[0];
            let restored_member = restored_room
                .panel(&f.worker)
                .unwrap()
                .member
                .as_ref()
                .unwrap();
            assert_eq!(restored_room.tasks, room.tasks);
            assert_eq!(restored_member, member);
            assert_eq!(
                next_assignment_index(restored_room, restored_member),
                Some(1)
            );
            assert!(f
                .state
                .task_context_for(
                    &f.workspace,
                    &f.manager,
                    Some("B2C-X"),
                    Some(TaskPatch {
                        status: Some(TaskStatus::Done),
                        ..Default::default()
                    })
                )
                .is_err());
        }
    }

    #[test]
    fn release_rejects_pending_own_work_even_when_latest_delivery_is_done() {
        for status in [
            AssignmentStatus::Queued,
            AssignmentStatus::Running,
            AssignmentStatus::Uncertain,
        ] {
            let f = release_fixture();
            {
                let mut persisted = guard(&f.state.persisted).unwrap();
                let member = find_terminal_mut(&mut persisted, &f.worker)
                    .unwrap()
                    .member
                    .as_mut()
                    .unwrap();
                let mut pending = member.jobs[0].clone();
                pending.id = "previous-pending-X".into();
                pending.status = status;
                member.jobs.insert(0, pending);
            }
            let before = f.state.snapshot().unwrap();
            assert!(f
                .state
                .release_task(&f.workspace, &f.manager, release_input(&f))
                .is_err());
            assert_eq!(f.state.snapshot().unwrap().areas, before.areas);
        }
    }

    #[test]
    fn failed_release_persistence_keeps_the_queue_and_history() {
        let f = release_fixture();
        let before = f.state.snapshot().unwrap();
        let backup = f.path.with_extension("backup");
        fs::rename(&f.path, &backup).unwrap();
        fs::create_dir(&f.path).unwrap();
        assert!(f
            .state
            .release_task(&f.workspace, &f.manager, release_input(&f))
            .is_err());
        assert_eq!(f.state.snapshot().unwrap().areas, before.areas);
        fs::remove_dir(&f.path).unwrap();
        fs::rename(&backup, &f.path).unwrap();
    }

    #[test]
    fn per_person_execution_survives_edit_reload_and_preserves_learning() {
        let fixture = Fixture::new();
        fixture
            .state
            .update_memory(&fixture.worker, "Processos aprendidos".into(), 0, None)
            .unwrap();
        let before = fixture.state.snapshot().unwrap();
        let worker = before.areas[0].workspaces[0]
            .panel(&fixture.worker)
            .unwrap();
        let settings = AgentExecution::preset("junior").unwrap();
        fixture
            .state
            .edit_agent(EditAgentInput {
                terminal_id: fixture.worker.clone(),
                name: worker.title.clone(),
                specialty: worker.member.as_ref().unwrap().specialty.clone(),
                instructions: worker.member.as_ref().unwrap().instructions.clone(),
                execution: Some(settings.clone()),
            })
            .unwrap();
        let snapshot = AppState::load(fixture.path.clone())
            .unwrap()
            .snapshot()
            .unwrap();
        let member = snapshot.areas[0].workspaces[0]
            .panel(&fixture.worker)
            .unwrap()
            .member
            .as_ref()
            .unwrap();
        assert_eq!(member.execution, settings);
        assert_eq!(member.memory.content, "Processos aprendidos");
        assert_eq!(member.training, TrainingState::Ready);
        assert_eq!(
            member.professional_profile,
            worker.member.as_ref().unwrap().professional_profile
        );
        let manager = before.areas[0].workspaces[0]
            .panel(&fixture.manager)
            .unwrap();
        assert!(fixture
            .state
            .edit_agent(EditAgentInput {
                terminal_id: fixture.manager.clone(),
                name: manager.title.clone(),
                specialty: manager.member.as_ref().unwrap().specialty.clone(),
                instructions: manager.member.as_ref().unwrap().instructions.clone(),
                execution: Some(settings),
            })
            .is_err());
    }

    #[test]
    fn manager_can_persist_a_new_catalog_model_without_losing_identity() {
        let fixture = Fixture::new();
        let before = fixture.state.snapshot().unwrap();
        let person = before.areas[0].workspaces[0]
            .panel(&fixture.manager)
            .unwrap();
        let member = person.member.as_ref().unwrap();
        let execution = AgentExecution {
            level: "manager".into(),
            model: "gpt-6.1-sol".into(),
            reasoning_effort: "ultra".into(),
        };
        fixture
            .state
            .edit_agent(EditAgentInput {
                terminal_id: fixture.manager.clone(),
                name: person.title.clone(),
                specialty: member.specialty.clone(),
                instructions: member.instructions.clone(),
                execution: Some(execution.clone()),
            })
            .unwrap();
        let restored = AppState::load(fixture.path.clone())
            .unwrap()
            .snapshot()
            .unwrap();
        let saved = restored.areas[0].workspaces[0]
            .panel(&fixture.manager)
            .unwrap()
            .member
            .as_ref()
            .unwrap();
        assert_eq!(saved.execution, execution);
        assert_eq!(saved.soul, member.soul);
        assert_eq!(saved.memory, member.memory);
        assert_eq!(saved.training, member.training);
        assert_eq!(saved.thread_id, member.thread_id);
    }

    #[test]
    fn training_requires_saved_learning_and_user_review_for_the_manager() {
        let fixture = Fixture::new();
        {
            let mut p = guard(&fixture.state.persisted).unwrap();
            find_terminal_mut(&mut p, &fixture.manager)
                .unwrap()
                .member
                .as_mut()
                .unwrap()
                .training = TrainingState::Awaiting;
        }
        let mut input = fixture.assignment("manager-training");
        input.terminal_id = fixture.manager.clone();
        assert!(fixture
            .state
            .enqueue_assignment(fixture.assignment("worker-before-manager-review"), None)
            .is_err());
        assert!(fixture
            .state
            .enqueue_assignment(input.clone(), None)
            .is_err());
        input.training = true;
        fixture.state.enqueue_assignment(input, None).unwrap();
        assert!(fixture
            .state
            .approve_training(&fixture.manager, 0, 1, None)
            .is_err());
        fixture
            .state
            .update_memory(
                &fixture.manager,
                "# Processo\nValidar a jornada. Origem: usuário.".into(),
                0,
                Some((&fixture.workspace, &fixture.manager)),
            )
            .unwrap();
        assert!(fixture
            .state
            .approve_training(&fixture.manager, 1, 1, None)
            .is_err());
        {
            let mut p = guard(&fixture.state.persisted).unwrap();
            let member = find_terminal_mut(&mut p, &fixture.manager)
                .unwrap()
                .member
                .as_mut()
                .unwrap();
            member.jobs.last_mut().unwrap().status = AssignmentStatus::Done;
            learning::finish_training(member);
            assert_eq!(member.training, TrainingState::Review);
        }
        assert!(fixture
            .state
            .approve_training(
                &fixture.manager,
                1,
                1,
                Some((&fixture.workspace, &fixture.manager))
            )
            .is_err());
        assert!(fixture
            .state
            .approve_training(
                &fixture.manager,
                1,
                1,
                Some((&fixture.workspace, &fixture.worker))
            )
            .is_err());
        assert!(fixture
            .state
            .approve_training(&fixture.manager, 0, 1, None)
            .is_err());
        fixture
            .state
            .approve_training(&fixture.manager, 1, 1, None)
            .unwrap();
        let restored = AppState::load(fixture.path.clone())
            .unwrap()
            .snapshot()
            .unwrap();
        let manager = restored.areas[0].workspaces[0].panels[0]
            .member
            .as_ref()
            .unwrap();
        assert_eq!(manager.training, TrainingState::Ready);
        assert!(manager.memory.content.contains("Validar a jornada"));
    }

    #[test]
    fn memories_are_scoped_versioned_and_training_without_learning_stays_unapproved() {
        let fixture = Fixture::new();
        fixture
            .state
            .update_memory(
                &fixture.worker,
                "# QA\nCritérios e evidências.".into(),
                0,
                Some((&fixture.workspace, &fixture.worker)),
            )
            .unwrap();
        assert!(fixture
            .state
            .memory_context(&fixture.workspace, &fixture.worker, Some(&fixture.manager))
            .is_err());
        assert!(fixture
            .state
            .memory_context(&fixture.workspace, &fixture.manager, Some(&fixture.worker))
            .is_ok());
        assert!(fixture
            .state
            .memory_context("other-room", &fixture.manager, Some(&fixture.worker))
            .is_err());
        assert!(fixture
            .state
            .update_memory(&fixture.worker, "Alteração antiga".into(), 0, None)
            .is_err());
        assert!(fixture
            .state
            .update_memory(
                &fixture.worker,
                "Gerente escrevendo pelo QA".into(),
                1,
                Some((&fixture.workspace, &fixture.manager))
            )
            .is_err());
        let mut input = fixture.assignment("train-worker");
        input.training = true;
        fixture
            .state
            .enqueue_assignment(input, Some((&fixture.workspace, &fixture.manager)))
            .unwrap();
        {
            let mut p = guard(&fixture.state.persisted).unwrap();
            let member = find_terminal_mut(&mut p, &fixture.worker)
                .unwrap()
                .member
                .as_mut()
                .unwrap();
            member.jobs.last_mut().unwrap().status = AssignmentStatus::Done;
            learning::finish_training(member);
            assert_eq!(member.training, TrainingState::Awaiting);
        }
        assert!(fixture
            .state
            .approve_training(
                &fixture.worker,
                1,
                1,
                Some((&fixture.workspace, &fixture.manager))
            )
            .is_err());
    }

    #[test]
    fn soul_only_initial_training_requires_reviewed_revisions_and_no_self_approval() {
        let fixture = Fixture::new();
        {
            let mut persisted = guard(&fixture.state.persisted).unwrap();
            let member = find_terminal_mut(&mut persisted, &fixture.manager)
                .unwrap()
                .member
                .as_mut()
                .unwrap();
            member.soul = AgentSoul::default();
            member.training = TrainingState::Awaiting;
        }
        let mut input = fixture.assignment("initial-soul");
        input.terminal_id = fixture.manager.clone();
        input.training = true;
        fixture.state.enqueue_assignment(input, None).unwrap();
        let soul = fixture
            .state
            .update_soul(
                &fixture.manager,
                "## Missão\nCoordenar a equipe. Origem: usuário no treinamento inicial.".into(),
                0,
                Some((&fixture.workspace, &fixture.manager)),
            )
            .unwrap();
        assert!(soul.approved.is_none());
        assert!(fixture
            .state
            .approve_training(&fixture.manager, 0, 1, None)
            .is_err());
        {
            let mut persisted = guard(&fixture.state.persisted).unwrap();
            let member = find_terminal_mut(&mut persisted, &fixture.manager)
                .unwrap()
                .member
                .as_mut()
                .unwrap();
            member.jobs.last_mut().unwrap().status = AssignmentStatus::Done;
            learning::finish_training(member);
            assert_eq!(member.training, TrainingState::Review);
            assert!(learning::training_recorded(member, Some(0), Some(0)));
            assert!(!learning::training_recorded(member, Some(0), Some(1)));
        }
        for (memory_revision, soul_revision, author) in [
            (1, 1, None),
            (0, 0, None),
            (
                0,
                1,
                Some((fixture.workspace.as_str(), fixture.manager.as_str())),
            ),
            (
                0,
                1,
                Some((fixture.workspace.as_str(), fixture.worker.as_str())),
            ),
        ] {
            assert!(fixture
                .state
                .approve_training(&fixture.manager, memory_revision, soul_revision, author)
                .is_err());
        }
        let context = fixture
            .state
            .memory_context(&fixture.workspace, &fixture.manager, None)
            .unwrap();
        assert_eq!(context["training"], "review");
        assert!(context["soul"]["approved"].is_null());
        fixture
            .state
            .approve_training(&fixture.manager, 0, 1, None)
            .unwrap();
        let restored = AppState::load(fixture.path.clone())
            .unwrap()
            .snapshot()
            .unwrap();
        let member = restored.areas[0].workspaces[0]
            .panel(&fixture.manager)
            .unwrap()
            .member
            .as_ref()
            .unwrap();
        assert_eq!(member.training, TrainingState::Ready);
        assert_eq!(member.memory, AgentMemory::default());
        assert_eq!(member.soul.approved.as_ref().unwrap().content, soul.content);
        assert_eq!(member.soul.approved.as_ref().unwrap().approved_by, "user");
    }

    #[test]
    fn soul_is_stable_scoped_and_a_new_training_proposal_keeps_the_approved_identity() {
        let fixture = Fixture::new();
        let before = fixture.state.snapshot().unwrap();
        let original = before.areas[0].workspaces[0]
            .panel(&fixture.worker)
            .unwrap()
            .member
            .as_ref()
            .unwrap()
            .soul
            .clone();
        let proposal = "## Missão\nValidar jornadas acessíveis. Origem: treinamento complementar.";
        assert!(fixture
            .state
            .update_soul(
                &fixture.worker,
                proposal.into(),
                1,
                Some((&fixture.workspace, &fixture.worker))
            )
            .is_err());
        assert!(fixture
            .state
            .update_soul(
                &fixture.worker,
                proposal.into(),
                1,
                Some((&fixture.workspace, &fixture.manager))
            )
            .is_err());
        assert!(fixture
            .state
            .update_soul(
                &fixture.worker,
                proposal.into(),
                1,
                Some(("other-room", &fixture.worker))
            )
            .is_err());
        assert!(fixture
            .state
            .update_soul(&fixture.worker, "x".repeat(12001), 1, None)
            .is_err());
        assert!(fixture
            .state
            .update_soul(&fixture.worker, " ".into(), 1, None)
            .is_err());
        fixture
            .state
            .update_memory(
                &fixture.worker,
                "Nesta demanda, a validação encontrou um formulário sem rótulo.".into(),
                0,
                Some((&fixture.workspace, &fixture.worker)),
            )
            .unwrap();
        let unchanged = fixture
            .state
            .memory_context(&fixture.workspace, &fixture.manager, Some(&fixture.worker))
            .unwrap();
        assert_eq!(unchanged["soul"], json!(original));
        let mut input = fixture.assignment("complementary-soul");
        input.training = true;
        fixture
            .state
            .enqueue_assignment(input, Some((&fixture.workspace, &fixture.manager)))
            .unwrap();
        let draft = fixture
            .state
            .update_soul(
                &fixture.worker,
                proposal.into(),
                1,
                Some((&fixture.workspace, &fixture.worker)),
            )
            .unwrap();
        assert_eq!(draft.approved, original.approved);
        assert_eq!(draft.revision, 2);
        assert_eq!(
            fixture
                .state
                .update_soul(
                    &fixture.worker,
                    proposal.into(),
                    1,
                    Some((&fixture.workspace, &fixture.worker))
                )
                .unwrap(),
            draft
        );
        assert!(fixture
            .state
            .update_soul(&fixture.worker, "Edição concorrente".into(), 1, None)
            .is_err());
        assert!(fixture.state.approve_soul(&fixture.worker, 2).is_err());
        {
            let mut persisted = guard(&fixture.state.persisted).unwrap();
            let member = find_terminal_mut(&mut persisted, &fixture.worker)
                .unwrap()
                .member
                .as_mut()
                .unwrap();
            let instructions = learning::instructions(member);
            assert!(instructions.contains("# Alma aprovada — revisão 1"));
            assert!(instructions.contains("# Proposta de alma — revisão 2"));
            member.jobs.last_mut().unwrap().status = AssignmentStatus::Done;
            learning::finish_training(member);
        }
        fixture
            .state
            .approve_training(
                &fixture.worker,
                1,
                2,
                Some((&fixture.workspace, &fixture.manager)),
            )
            .unwrap();
        let restored = AppState::load(fixture.path.clone())
            .unwrap()
            .snapshot()
            .unwrap();
        let member = restored.areas[0].workspaces[0]
            .panel(&fixture.worker)
            .unwrap()
            .member
            .as_ref()
            .unwrap();
        assert_eq!(member.soul.approved.as_ref().unwrap().content, proposal);
        assert_eq!(
            member.soul.approved.as_ref().unwrap().approved_by,
            fixture.manager
        );
        assert_eq!(member.training, TrainingState::Ready);
        assert_eq!(member.memory.revision, 1);
        let duplicate = restored.areas[0].workspaces[0].clone_for_duplicate();
        assert!(duplicate
            .panels
            .iter()
            .all(|p| p.member.as_ref().unwrap().soul == AgentSoul::default()));
    }

    #[test]
    fn user_soul_edits_require_approval_and_failed_persistence_preserves_identity() {
        let fixture = Fixture::new();
        let original = fixture
            .state
            .memory_context(&fixture.workspace, &fixture.manager, None)
            .unwrap();
        let soul = fixture
            .state
            .update_soul(
                &fixture.manager,
                "## Missão\nCuidar das entregas e critérios do time. Origem: usuário.".into(),
                1,
                None,
            )
            .unwrap();
        assert_eq!(json!(soul.approved), original["soul"]["approved"]);
        assert!(fixture.state.approve_soul(&fixture.manager, 1).is_err());
        let before = fixture.state.snapshot().unwrap();
        let backup = fixture.path.with_extension("backup");
        fs::rename(&fixture.path, &backup).unwrap();
        fs::create_dir(&fixture.path).unwrap();
        assert!(fixture.state.approve_soul(&fixture.manager, 2).is_err());
        assert!(fixture
            .state
            .update_soul(&fixture.manager, "Falha ao salvar".into(), 2, None)
            .is_err());
        assert_eq!(fixture.state.snapshot().unwrap().areas, before.areas);
        fs::remove_dir(&fixture.path).unwrap();
        fs::rename(&backup, &fixture.path).unwrap();
        fixture.state.approve_soul(&fixture.manager, 2).unwrap();
        let context = fixture
            .state
            .memory_context(&fixture.workspace, &fixture.manager, None)
            .unwrap();
        assert_eq!(context["soul"]["approved"]["revision"], 2);
        assert_eq!(context["memory"], original["memory"]);
        assert_eq!(context["training"], original["training"]);
    }

    #[test]
    fn legacy_state_has_no_implicitly_approved_soul_and_role_changes_require_a_fresh_identity() {
        let fixture = Fixture::new();
        let mut persisted = guard(&fixture.state.persisted).unwrap().clone();
        for panel in &mut persisted.areas[0].workspaces[0].panels {
            panel.member.as_mut().unwrap().soul = AgentSoul::default();
        }
        let mut legacy = serde_json::to_value(&persisted).unwrap();
        for panel in legacy["areas"][0]["workspaces"][0]["panels"]
            .as_array_mut()
            .unwrap()
        {
            panel["member"].as_object_mut().unwrap().remove("soul");
        }
        fs::write(&fixture.path, serde_json::to_vec(&legacy).unwrap()).unwrap();
        let restored = AppState::load(fixture.path.clone())
            .unwrap()
            .snapshot()
            .unwrap();
        assert_eq!(restored.areas, persisted.areas);
        let snapshot = fixture
            .state
            .edit_agent(EditAgentInput {
                execution: None,
                terminal_id: fixture.worker.clone(),
                name: "Lia".into(),
                specialty: "PO".into(),
                instructions: "Priorizar entregas.".into(),
            })
            .unwrap();
        let member = snapshot.areas[0].workspaces[0]
            .panel(&fixture.worker)
            .unwrap()
            .member
            .as_ref()
            .unwrap();
        assert_eq!(member.soul, AgentSoul::default());
        assert_eq!(member.training, TrainingState::Awaiting);
    }

    #[test]
    fn codex_profile_is_persistent_scoped_and_does_not_approve_training() {
        let fixture = Fixture::new();
        let snapshot = fixture
            .state
            .hire_agent(HireAgentInput {
                execution: None,
                workspace_id: fixture.workspace.clone(),
                name: String::new(),
                specialty: "PO".into(),
                instructions: String::new(),
            })
            .unwrap();
        let id = snapshot.areas[0].workspaces[0]
            .panels
            .last()
            .unwrap()
            .id
            .clone();
        let original_name = snapshot.areas[0].workspaces[0]
            .panel(&id)
            .unwrap()
            .title
            .clone();
        let input = ProfessionalProfileInput {
            name: original_name.clone(),
            personality: "Curiosa, sintética e colaborativa.".into(),
            content:
                "## Missão\nEsclarecer prioridades.\n## Qualidade\nCritérios de aceite observáveis."
                    .into(),
        };
        assert!(fixture
            .state
            .update_memory(
                &id,
                "Aprendizado".into(),
                0,
                Some((&fixture.workspace, &id))
            )
            .is_err());
        assert!(fixture
            .state
            .set_professional_profile("other-room", &id, input.clone())
            .is_err());
        let mut invalid = input.clone();
        invalid.content = "x".repeat(6001);
        assert!(fixture
            .state
            .set_professional_profile(&fixture.workspace, &id, invalid)
            .is_err());
        let mut duplicate = input.clone();
        duplicate.name = "Lia".into();
        assert!(fixture
            .state
            .set_professional_profile(&fixture.workspace, &id, duplicate)
            .is_err());
        let profile = fixture
            .state
            .set_professional_profile(&fixture.workspace, &id, input.clone())
            .unwrap();
        assert_eq!(
            fixture
                .state
                .set_professional_profile(&fixture.workspace, &id, input.clone())
                .unwrap(),
            profile
        );
        let mut changed = input.clone();
        changed.content = "Outro perfil".into();
        assert!(fixture
            .state
            .set_professional_profile(&fixture.workspace, &id, changed)
            .is_err());
        let restored = AppState::load(fixture.path.clone())
            .unwrap()
            .snapshot()
            .unwrap();
        let person = restored.areas[0].workspaces[0].panel(&id).unwrap();
        let member = person.member.as_ref().unwrap();
        assert_eq!(person.title, original_name);
        assert_eq!(member.professional_profile.as_ref(), Some(&profile));
        assert_eq!(member.training, TrainingState::Awaiting);
        assert_eq!(member.memory.revision, 0);
        assert!(learning::instructions(member).contains(&input.content));
        let context = fixture
            .state
            .memory_context(&fixture.workspace, &id, None)
            .unwrap();
        assert_eq!(context["company"], "Empresa");
        assert_eq!(context["professionalProfile"]["specialty"], "PO");
        let duplicated = restored.areas[0].workspaces[0].clone_for_duplicate();
        assert!(duplicated.panels.iter().all(|p| p
            .member
            .as_ref()
            .unwrap()
            .professional_profile
            .is_none()));
    }

    #[test]
    fn professional_profile_cannot_rename_automatic_manager_or_hired_person() {
        let fixture = Fixture::new();
        let snapshot = fixture
            .state
            .hire_agent(HireAgentInput {
                execution: None,
                workspace_id: fixture.workspace.clone(),
                name: String::new(),
                specialty: "PO".into(),
                instructions: String::new(),
            })
            .unwrap();
        let hired = snapshot.areas[0].workspaces[0]
            .panels
            .last()
            .unwrap()
            .id
            .clone();
        // Reproduce a first conversation with an older automatic-name manager.
        {
            let mut persisted = guard(&fixture.state.persisted).unwrap();
            let panel = find_terminal_mut(&mut persisted, &fixture.manager).unwrap();
            panel.title = "Beatriz".into();
            let member = panel.member.as_mut().unwrap();
            member.professional_profile = None;
            member.automatic_name = true;
            fixture.state.save(&persisted).unwrap();
        }
        for id in [&fixture.manager, &hired] {
            let before = fixture.state.snapshot().unwrap();
            let person = before.areas[0].workspaces[0].panel(id).unwrap();
            assert!(person.member.as_ref().unwrap().automatic_name);
            let mut input = ProfessionalProfileInput {
                name: if person.title == "Marina" {
                    "Sofia"
                } else {
                    "Marina"
                }
                .into(),
                personality: "Objetiva e colaborativa.".into(),
                content: "## Missão\nColaborar conforme a função.".into(),
            };
            assert!(fixture
                .state
                .set_professional_profile(&fixture.workspace, id, input.clone())
                .unwrap_err()
                .contains("Preserve o nome atual"));
            let rejected = AppState::load(fixture.path.clone())
                .unwrap()
                .snapshot()
                .unwrap();
            assert_eq!(rejected.areas, before.areas);
            assert_eq!(fixture.state.snapshot().unwrap().areas, before.areas);
            assert!(learning::instructions(person.member.as_ref().unwrap())
                .contains("Preserve exatamente o nome atual"));
            input.name = person.title.clone();
            let profile = fixture
                .state
                .set_professional_profile(&fixture.workspace, id, input)
                .unwrap();
            let restored = AppState::load(fixture.path.clone())
                .unwrap()
                .snapshot()
                .unwrap();
            let saved = restored.areas[0].workspaces[0].panel(id).unwrap();
            assert_eq!(saved.title, person.title);
            assert_eq!(profile.name, person.title);
            assert_eq!(
                saved.member.as_ref().unwrap().training,
                person.member.as_ref().unwrap().training
            );
            assert_eq!(
                saved.member.as_ref().unwrap().memory,
                person.member.as_ref().unwrap().memory
            );
        }
    }

    #[test]
    fn profile_preserves_user_names_and_new_role_requires_new_profile() {
        let fixture = Fixture::new();
        fixture
            .state
            .edit_agent(EditAgentInput {
                execution: None,
                terminal_id: fixture.worker.clone(),
                name: "Nome escolhido".into(),
                specialty: "PO".into(),
                instructions: "Priorizar backlog".into(),
            })
            .unwrap();
        let mut input = ProfessionalProfileInput {
            name: "Outro nome".into(),
            personality: "Objetiva".into(),
            content: "## Missão\nPriorizar valor.".into(),
        };
        assert!(fixture
            .state
            .set_professional_profile(&fixture.workspace, &fixture.worker, input.clone())
            .unwrap_err()
            .contains("Preserve o nome"));
        input.name = "Nome escolhido".into();
        fixture
            .state
            .set_professional_profile(&fixture.workspace, &fixture.worker, input)
            .unwrap();
        let context = fixture
            .state
            .memory_context(&fixture.workspace, &fixture.worker, None)
            .unwrap();
        assert_eq!(context["name"], "Nome escolhido");
        assert_eq!(context["training"], "awaiting");
        assert!(serde_json::from_value::<ProfessionalProfileInput>(
            json!({"terminalId":fixture.manager,"name":"X","personality":"Y","content":"Z"})
        )
        .is_err());
    }

    #[test]
    fn hiring_only_a_role_generates_a_stable_identity_and_fresh_training() {
        let fixture = Fixture::new();
        let snapshot = fixture
            .state
            .hire_agent(HireAgentInput {
                execution: None,
                workspace_id: fixture.workspace.clone(),
                name: String::new(),
                specialty: "PO".into(),
                instructions: String::new(),
            })
            .unwrap();
        let person = snapshot.areas[0].workspaces[0].panels.last().unwrap();
        assert!(!person.title.is_empty());
        assert!(person
            .member
            .as_ref()
            .unwrap()
            .professional_profile
            .is_none());
        assert!(person.member.as_ref().unwrap().automatic_name);
        assert_eq!(
            person.member.as_ref().unwrap().training,
            TrainingState::Awaiting
        );
        let restored = AppState::load(fixture.path.clone())
            .unwrap()
            .snapshot()
            .unwrap();
        assert_eq!(
            restored.areas[0].workspaces[0].panels.last().unwrap(),
            person
        );
    }
    #[test]
    fn area_instructions_are_merged_and_changes_are_pending_until_reload() {
        let fixture = Fixture::new();
        let root = fixture.path.parent().unwrap();
        let initial = area_instructions::read(root).unwrap();
        let snapshot = fixture.state.snapshot().unwrap();
        let panel = &snapshot.areas[0].workspaces[0].panels[0];
        let merged = instructions(panel, &initial);
        assert!(merged.contains(&initial.content));
        assert!(merged.contains(&initial.revision));
        assert!(merged.contains(&panel.title));
        guard(&fixture.state.member_states).unwrap().insert(
            fixture.manager.clone(),
            MemberRuntime {
                connected: true,
                instructions_revision: Some(initial.revision.clone()),
                ..Default::default()
            },
        );
        assert!(
            !fixture.state.snapshot().unwrap().member_states[&fixture.manager].instructions_pending
        );
        fs::write(
            root.join(".kanasha/AGENTS.md"),
            "# Empresa\nNova orientação.",
        )
        .unwrap();
        let updated = fixture.state.snapshot().unwrap();
        assert!(updated.member_states[&fixture.manager].instructions_pending);
        assert_eq!(
            updated.member_states[&fixture.manager]
                .instructions_revision
                .as_deref(),
            Some(initial.revision.as_str())
        );
        let changed = area_instructions::prepare(root).unwrap();
        assert!(instructions(panel, &changed).contains("Nova orientação."));
        guard(&fixture.state.member_states)
            .unwrap()
            .get_mut(&fixture.manager)
            .unwrap()
            .instructions_revision = Some(changed.revision);
        assert!(
            !fixture.state.snapshot().unwrap().member_states[&fixture.manager].instructions_pending
        );
        fs::remove_file(root.join(".kanasha/AGENTS.md")).unwrap();
        let missing = fixture.state.snapshot().unwrap();
        assert!(missing.member_states[&fixture.manager].instructions_pending);
        assert!(missing.member_states[&fixture.manager]
            .instructions_error
            .is_some());
    }

    struct Fixture {
        state: AppState,
        path: PathBuf,
        workspace: String,
        manager: String,
        worker: String,
    }
    impl Fixture {
        fn new() -> Self {
            let path = PathBuf::from("/tmp")
                .join(format!("kt-test-{}", new_id()))
                .join("state.json");
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            let state = AppState::load(path.clone()).unwrap();
            let snapshot = state
                .create_area(CreateAreaInput {
                    workspace_kind: crate::models::WorkspaceKind::Team,
                    name: "Empresa".into(),
                    root_path: path.parent().unwrap().to_string_lossy().into_owned(),
                })
                .unwrap();
            let workspace = snapshot.areas[0].workspaces[0].id.clone();
            let manager = snapshot.areas[0].workspaces[0].panels[0].id.clone();
            let snapshot = state
                .hire_agent(HireAgentInput {
                    execution: None,
                    workspace_id: workspace.clone(),
                    name: "Lia".into(),
                    specialty: "QA e requisitos".into(),
                    instructions: "Validar e colaborar nos critérios.".into(),
                })
                .unwrap();
            let worker = snapshot.areas[0].workspaces[0].panels[1].id.clone();
            for panel in &snapshot.areas[0].workspaces[0].panels {
                state
                    .set_professional_profile(
                        &workspace,
                        &panel.id,
                        ProfessionalProfileInput {
                            name: panel.title.clone(),
                            personality: "Objetiva e colaborativa.".into(),
                            content:
                                "## Missão\nColaborar conforme a função e validar com evidências."
                                    .into(),
                        },
                    )
                    .unwrap();
            }
            for panel in &snapshot.areas[0].workspaces[0].panels {
                state.update_soul(&panel.id, "## Missão\nColaborar na função contratada.\n## Critérios\nValidar com evidências. Origem: treinamento do usuário.".into(), 0, Some((&workspace, &panel.id))).unwrap();
                state.approve_soul(&panel.id, 1).unwrap();
            }
            {
                let mut persisted = guard(&state.persisted).unwrap();
                for panel in &mut find_workspace_mut(&mut persisted, &workspace)
                    .unwrap()
                    .panels
                {
                    panel.member.as_mut().unwrap().training = TrainingState::Ready;
                }
                state.save(&persisted).unwrap();
            }
            Self {
                state,
                path,
                workspace,
                manager,
                worker,
            }
        }
        fn assignment(&self, id: &str) -> AssignAgentInput {
            AssignAgentInput {
                step_ids: Vec::new(),
                task_code: None,
                fresh_context: None,
                training: false,
                terminal_id: self.worker.clone(),
                request_id: id.into(),
                message: "Validar cadastro".into(),
            }
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(self.path.parent().unwrap());
        }
    }

    #[test]
    fn jira_steps_keep_context_and_switching_task_prepares_a_new_conversation() {
        let f = Fixture::new();
        for (id, code, message) in [
            ("analisar", "b2c-123", "Analisar"),
            ("planejar", "B2C-123", "Criar plano"),
            ("implementar", "B2C-123", "Implementar"),
            ("outra", "B2C-456", "Outra tarefa"),
        ] {
            let mut input = f.assignment(id);
            input.task_code = Some(code.into());
            input.message = message.into();
            input.fresh_context = Some(true); // Even this flag cannot clear steps.
            f.state
                .enqueue_assignment(input, Some((&f.workspace, &f.manager)))
                .unwrap();
        }
        let room = f
            .state
            .team_context(&f.workspace, &f.manager, None)
            .unwrap();
        assert_eq!(room["tasks"][0]["taskCode"], "B2C-123");
        assert_eq!(room["tasks"][0]["assignments"].as_array().unwrap().len(), 3);
        assert_eq!(room["tasks"][0]["assignments"][0]["terminalId"], f.worker);
        assert_eq!(room["tasks"][1]["taskCode"], "B2C-456");
        {
            let mut p = guard(&f.state.persisted).unwrap();
            let m = find_terminal_mut(&mut p, &f.worker)
                .unwrap()
                .member
                .as_mut()
                .unwrap();
            m.thread_id = Some("thread-training".into());
            assert!(needs_fresh_context(m, &m.jobs[0]));
        }
        assert!(f
            .state
            .prepare_fresh_context(
                None,
                &f.worker,
                "assignment-context:analisar",
                None,
                Some("analisar")
            )
            .unwrap());
        let mut p = guard(&f.state.persisted).unwrap();
        let m = find_terminal_mut(&mut p, &f.worker)
            .unwrap()
            .member
            .as_mut()
            .unwrap();
        m.thread_id = Some("thread-B2C-123".into());
        assert_eq!(m.context_task_code.as_deref(), Some("B2C-123"));
        for job in &m.jobs[..3] {
            assert!(!needs_fresh_context(m, job));
        }
        assert!(needs_fresh_context(m, &m.jobs[3]));
        assert_eq!(
            m.jobs.iter().map(|j| j.id.as_str()).collect::<Vec<_>>(),
            ["analisar", "planejar", "implementar", "outra"]
        );
        for job in &mut m.jobs[..3] {
            job.status = AssignmentStatus::Done;
        }
        drop(p);
        f.state
            .task_context_for(
                &f.workspace,
                &f.manager,
                Some("B2C-123"),
                Some(TaskPatch {
                    status: Some(TaskStatus::Done),
                    ..Default::default()
                }),
            )
            .unwrap();
        assert!(f
            .state
            .prepare_fresh_context(
                None,
                &f.worker,
                "assignment-context:outra",
                None,
                Some("outra")
            )
            .unwrap());
        let p = guard(&f.state.persisted).unwrap();
        let m = find_terminal_workspace(&p, &f.worker)
            .unwrap()
            .1
            .panel(&f.worker)
            .unwrap()
            .member
            .as_ref()
            .unwrap();
        assert_eq!(m.context_task_code.as_deref(), Some("B2C-456"));
        assert_eq!(
            m.context_resets[1].previous_thread_id.as_deref(),
            Some("thread-B2C-123")
        );
        assert!(m.thread_id.is_none());
    }

    #[test]
    fn queue_accepts_ten_jobs_for_one_busy_employee_without_hiring_people() {
        let f = Fixture::new();
        guard(&f.state.member_states).unwrap().insert(
            f.worker.clone(),
            MemberRuntime {
                turn_id: Some("direct-turn".into()),
                connected: true,
                ..Default::default()
            },
        );
        for index in 0..10 {
            f.state
                .enqueue_assignment(
                    f.assignment(&format!("job-{index}")),
                    Some((&f.workspace, &f.manager)),
                )
                .unwrap();
        }
        assert!(f
            .state
            .enqueue_assignment(f.assignment("overflow"), None)
            .is_err());
        let view = f
            .state
            .team_context(&f.workspace, &f.manager, Some(&f.worker))
            .unwrap();
        assert_eq!(view["queue"].as_array().unwrap().len(), 10);
        for index in 0..10 {
            assert_eq!(view["queue"][index]["id"], format!("job-{index}"));
        }
        assert_eq!(
            f.state.snapshot().unwrap().areas[0].workspaces[0]
                .panels
                .len(),
            2
        );
    }

    #[test]
    fn new_conversation_preserves_learning_queue_history_and_is_idempotent_after_reload() {
        let f = Fixture::new();
        f.state
            .update_memory(
                &f.worker,
                "2026-09-30: treinamento concluído. Detalhes: tasks/registro.md".into(),
                0,
                None,
            )
            .unwrap();
        f.state
            .enqueue_assignment(f.assignment("queued"), None)
            .unwrap();
        {
            let mut p = guard(&f.state.persisted).unwrap();
            let m = find_terminal_mut(&mut p, &f.worker)
                .unwrap()
                .member
                .as_mut()
                .unwrap();
            m.thread_id = Some("old-thread".into());
            m.context_task_code = Some("B2C-123".into());
            f.state.save(&p).unwrap();
        }
        let before = f.state.snapshot().unwrap().areas[0].workspaces[0].panels[1].clone();
        let file = f.path.parent().unwrap().join("registro.md");
        fs::write(&file, "Evidência preservada").unwrap();
        assert!(f
            .state
            .prepare_fresh_context(
                None,
                &f.worker,
                "manual-reset",
                Some((&f.workspace, &f.manager)),
                None
            )
            .unwrap());
        let restored = AppState::load(f.path.clone()).unwrap();
        assert!(!restored
            .prepare_fresh_context(
                None,
                &f.worker,
                "manual-reset",
                Some((&f.workspace, &f.manager)),
                None
            )
            .unwrap());
        let after = restored.snapshot().unwrap().areas[0].workspaces[0].panels[1].clone();
        let m = after.member.as_ref().unwrap();
        let old = before.member.as_ref().unwrap();
        assert_eq!(after.title, before.title);
        assert_eq!(m.soul, old.soul);
        assert_eq!(m.memory, old.memory);
        assert_eq!(m.professional_profile, old.professional_profile);
        assert_eq!(m.training, old.training);
        assert_eq!(m.jobs, old.jobs);
        assert!(m.thread_id.is_none());
        assert_eq!(m.context_resets.len(), 1);
        guard(&restored.member_states).unwrap().insert(
            f.worker.clone(),
            MemberRuntime {
                connected: true,
                turn_id: Some("replacement-turn".into()),
                ..Default::default()
            },
        );
        assert!(!restored
            .prepare_fresh_context(
                None,
                &f.worker,
                "manual-reset",
                Some((&f.workspace, &f.manager)),
                None
            )
            .unwrap());
        assert!(restored
            .prepare_fresh_context(
                None,
                &f.worker,
                "another-reset",
                Some((&f.workspace, &f.manager)),
                None
            )
            .is_err());
        assert_eq!(
            m.context_resets[0].previous_thread_id.as_deref(),
            Some("old-thread")
        );
        assert_eq!(fs::read_to_string(file).unwrap(), "Evidência preservada");
    }

    #[test]
    fn conversation_reset_rejects_busy_uncertain_paused_self_and_other_rooms() {
        let f = Fixture::new();
        assert!(f
            .state
            .prepare_fresh_context(
                None,
                &f.worker,
                "denied",
                Some((&f.workspace, &f.worker)),
                None
            )
            .is_err());
        assert!(f
            .state
            .prepare_fresh_context(
                None,
                &f.manager,
                "denied",
                Some((&f.workspace, &f.manager)),
                None
            )
            .is_err());
        assert!(f
            .state
            .prepare_fresh_context(
                None,
                &f.worker,
                "denied",
                Some(("other-room", &f.manager)),
                None
            )
            .is_err());
        guard(&f.state.member_states).unwrap().insert(
            f.worker.clone(),
            MemberRuntime {
                turn_id: Some("turn".into()),
                ..Default::default()
            },
        );
        assert!(f
            .state
            .prepare_fresh_context(None, &f.worker, "busy", None, None)
            .is_err());
        guard(&f.state.member_states).unwrap().insert(
            f.worker.clone(),
            MemberRuntime {
                request: Some(json!({"id":"question"})),
                ..Default::default()
            },
        );
        assert!(f
            .state
            .prepare_fresh_context(None, &f.worker, "question", None, None)
            .is_err());
        guard(&f.state.member_states).unwrap().clear();
        f.state
            .enqueue_assignment(f.assignment("uncertain"), None)
            .unwrap();
        {
            let mut p = guard(&f.state.persisted).unwrap();
            find_terminal_mut(&mut p, &f.worker)
                .unwrap()
                .member
                .as_mut()
                .unwrap()
                .jobs[0]
                .status = AssignmentStatus::Uncertain;
        }
        assert!(f
            .state
            .prepare_fresh_context(None, &f.worker, "uncertain", None, None)
            .is_err());
        {
            let mut p = guard(&f.state.persisted).unwrap();
            let m = find_terminal_mut(&mut p, &f.worker)
                .unwrap()
                .member
                .as_mut()
                .unwrap();
            m.jobs[0].status = AssignmentStatus::Done;
            m.paused = true;
        }
        assert!(f
            .state
            .prepare_fresh_context(None, &f.worker, "paused", None, None)
            .is_err());
    }

    #[test]
    fn failed_reset_persistence_does_not_change_context_or_revoke_credentials() {
        let f = Fixture::new();
        {
            let mut p = guard(&f.state.persisted).unwrap();
            find_terminal_mut(&mut p, &f.worker)
                .unwrap()
                .member
                .as_mut()
                .unwrap()
                .thread_id = Some("keep-thread".into());
        }
        guard(&f.state.member_tokens)
            .unwrap()
            .insert(f.worker.clone(), "keep-token".into());
        let before = guard(&f.state.persisted).unwrap().clone();
        fs::create_dir(f.path.with_extension("tmp")).unwrap();
        assert!(f
            .state
            .prepare_fresh_context(None, &f.worker, "failed", None, None)
            .is_err());
        assert_eq!(*guard(&f.state.persisted).unwrap(), before);
        assert!(f.state.valid_mcp_token(&f.worker, "keep-token").unwrap());
    }

    #[test]
    fn missing_task_code_and_manager_results_preserve_context_and_receipts_bind_task_code() {
        let f = Fixture::new();
        f.state
            .enqueue_assignment(f.assignment("legacy"), None)
            .unwrap();
        let mut changed = f.assignment("legacy");
        changed.task_code = Some("B2C-999".into());
        assert!(f.state.enqueue_assignment(changed, None).is_err());
        let mut manager_input = f.assignment("manager-result");
        manager_input.terminal_id = f.manager.clone();
        manager_input.task_code = Some("B2C-999".into());
        f.state.enqueue_assignment(manager_input, None).unwrap();
        let p = guard(&f.state.persisted).unwrap();
        for id in [&f.worker, &f.manager] {
            let m = find_terminal_workspace(&p, id)
                .unwrap()
                .1
                .panel(id)
                .unwrap()
                .member
                .as_ref()
                .unwrap();
            assert!(!needs_fresh_context(m, &m.jobs[0]));
        }
    }

    #[test]
    #[ignore = "requires installed Codex; replaces local threads without starting inference"]
    fn installed_codex_context_reset_creates_a_fresh_thread_and_keeps_the_previous_history() {
        let f = Fixture::new();
        f.state
            .update_memory(
                &f.worker,
                "2026-09-30: aprendizado durável FRESH_MEMORY_MARKER".into(),
                0,
                None,
            )
            .unwrap();
        let root = f.path.parent().unwrap();
        let old = Arc::new(
            CodexSession::start(root, &[], &[], None, "Previous task context", None, |_| {})
                .unwrap(),
        );
        let old_id = old.thread_id.clone();
        let old_connection = old.connection_id.clone();
        old.request("thread/inject_items", json!({"threadId":old_id,"items":[{"type":"message","role":"user","content":[{"type":"input_text","text":"OLD_TASK_PRIVATE_MARKER: analysis and plan of X"}]}]})).unwrap();
        guard(&f.state.office_sessions)
            .unwrap()
            .insert(f.worker.clone(), old.clone());
        {
            let mut p = guard(&f.state.persisted).unwrap();
            find_terminal_mut(&mut p, &f.worker)
                .unwrap()
                .member
                .as_mut()
                .unwrap()
                .thread_id = Some(old_id.clone());
            f.state.save(&p).unwrap();
        }
        drop(old);
        assert!(f
            .state
            .prepare_fresh_context(None, &f.worker, "real-reset", None, None)
            .unwrap());
        assert!(!guard(&f.state.office_sessions)
            .unwrap()
            .contains_key(&f.worker));
        let panel = f.state.snapshot().unwrap().areas[0].workspaces[0].panels[1].clone();
        let text = instructions(&panel, &area_instructions::read(root).unwrap());
        assert!(text.contains("FRESH_MEMORY_MARKER"));
        assert!(text.contains(
            &panel
                .member
                .as_ref()
                .unwrap()
                .soul
                .approved
                .as_ref()
                .unwrap()
                .content
        ));
        let fresh = CodexSession::start(
            root,
            &[],
            &[],
            panel.member.as_ref().unwrap().thread_id.as_deref(),
            &text,
            None,
            |_| {},
        )
        .unwrap();
        assert_ne!(fresh.thread_id, old_id);
        assert_ne!(fresh.connection_id, old_connection);
        let fresh_history = fresh
            .request(
                "thread/read",
                json!({"threadId":fresh.thread_id,"includeTurns":true}),
            )
            .unwrap();
        let new_path = fresh_history["thread"]["path"].as_str().unwrap();
        assert!(!fs::read_to_string(new_path)
            .unwrap()
            .contains("OLD_TASK_PRIVATE_MARKER"));
        let previous = CodexSession::start(
            root,
            &[],
            &[],
            Some(&old_id),
            "Read-only history verification",
            None,
            |_| {},
        )
        .unwrap();
        let history = previous
            .request(
                "thread/read",
                json!({"threadId":old_id,"includeTurns":true}),
            )
            .unwrap();
        // Codex 0.159 uses paginated history: thread/read exposes the rollout path
        // even when includeTurns returns an empty turns page for injected items.
        let old_path = history["thread"]["path"].as_str().unwrap();
        assert_eq!(previous.thread_id, old_id);
        assert!(fs::read_to_string(old_path)
            .unwrap()
            .contains("OLD_TASK_PRIVATE_MARKER"));
    }

    #[test]
    fn assignments_bind_real_jira_tasks_steps_and_people_and_survive_reload() {
        let f = Fixture::new();
        f.state
            .task_context_for(
                &f.workspace,
                &f.manager,
                Some("b2c-123"),
                Some(TaskPatch {
                    summary: Some("Objetivo da tarefa X".into()),
                    steps: Some(vec![
                        TaskStep {
                            id: "analise".into(),
                            title: "Analisar".into(),
                            status: TaskStatus::NotStarted,
                        },
                        TaskStep {
                            id: "plano".into(),
                            title: "Criar plano".into(),
                            status: TaskStatus::NotStarted,
                        },
                    ]),
                    ..Default::default()
                }),
            )
            .unwrap();
        let mut input = f.assignment("step-analysis");
        input.task_code = Some("B2C-123".into());
        input.step_ids = vec!["analise".into()];
        f.state
            .enqueue_assignment(input.clone(), Some((&f.workspace, &f.manager)))
            .unwrap();
        input.step_ids = vec!["plano".into()];
        assert!(f
            .state
            .enqueue_assignment(input, Some((&f.workspace, &f.manager)))
            .is_err());
        let read = f
            .state
            .issue_task_context(&f.workspace, &f.worker, "B2C-123", true)
            .unwrap();
        assert_eq!(read["steps"][0]["title"], "Analisar");
        assert_eq!(read["steps"][0]["assignments"][0]["terminalId"], f.worker);
        assert_eq!(
            read["steps"][0]["assignments"][0]["assignmentId"],
            "step-analysis"
        );
        assert!(read["steps"][1]["assignments"]
            .as_array()
            .unwrap()
            .is_empty());
        assert!(f
            .state
            .task_context_for(
                &f.workspace,
                &f.worker,
                Some("B2C-123"),
                Some(TaskPatch {
                    step_status: Some(StepStatusPatch {
                        id: "analise".into(),
                        status: TaskStatus::Done
                    }),
                    ..Default::default()
                })
            )
            .is_err());
        assert!(f
            .state
            .task_context_for(
                &f.workspace,
                &f.manager,
                Some("B2C-123"),
                Some(TaskPatch {
                    steps: Some(vec![]),
                    ..Default::default()
                })
            )
            .is_err());
        f.state
            .task_context_for(
                &f.workspace,
                &f.manager,
                Some("B2C-456"),
                Some(TaskPatch {
                    steps: Some(vec![TaskStep {
                        id: "qa".into(),
                        title: "Validar".into(),
                        status: TaskStatus::NotStarted,
                    }]),
                    ..Default::default()
                }),
            )
            .unwrap();
        let mut invalid = f.assignment("invalid-binding");
        invalid.task_code = Some("B2C-456".into());
        invalid.step_ids = vec!["analise".into()];
        assert!(f.state.enqueue_assignment(invalid, None).is_err());
        let mut no_task = f.assignment("no-task");
        no_task.step_ids = vec!["analise".into()];
        assert!(f.state.enqueue_assignment(no_task, None).is_err());
        let restored = AppState::load(f.path.clone()).unwrap();
        assert_eq!(
            restored
                .issue_task_context(&f.workspace, &f.worker, "B2C-123", true)
                .unwrap(),
            read
        );
        assert_eq!(
            restored
                .issue_tasks_context(&f.workspace, &f.manager)
                .unwrap()["tasks"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        assert_eq!(
            restored
                .task_context(&f.workspace, &f.manager, None)
                .unwrap()
                .task,
            TaskState::default()
        );
    }

    #[test]
    fn task_queue_finishes_current_task_and_its_late_steps_before_starting_the_next_task() {
        let f = Fixture::new();
        for (id, code) in [
            ("x-analysis", "B2C-123"),
            ("y-analysis", "B2C-456"),
            ("x-plan-later", "B2C-123"),
        ] {
            let mut input = f.assignment(id);
            input.task_code = Some(code.into());
            f.state
                .enqueue_assignment(input, Some((&f.workspace, &f.manager)))
                .unwrap();
        }
        {
            let mut p = guard(&f.state.persisted).unwrap();
            let room = find_workspace_mut(&mut p, &f.workspace).unwrap();
            let m = room.panel(&f.worker).unwrap().member.as_ref().unwrap();
            assert_eq!(m.task_queue, ["B2C-123", "B2C-456"]);
            assert_eq!(
                queued_assignments(m)
                    .iter()
                    .map(|j| j.id.as_str())
                    .collect::<Vec<_>>(),
                ["x-analysis", "x-plan-later", "y-analysis"]
            );
            assert_eq!(next_assignment_index(room, m), Some(0));
            let m = find_terminal_mut(&mut p, &f.worker)
                .unwrap()
                .member
                .as_mut()
                .unwrap();
            m.jobs[0].status = AssignmentStatus::Done;
            m.context_task_code = Some("B2C-123".into());
            m.thread_id = Some("thread-X".into());
        }
        {
            let p = guard(&f.state.persisted).unwrap();
            let room = find_workspace(&p, &f.workspace).unwrap();
            let m = room.panel(&f.worker).unwrap().member.as_ref().unwrap();
            assert_eq!(next_assignment_index(room, m), Some(2));
            assert!(!needs_fresh_context(m, &m.jobs[2]));
        }
        assert!(f
            .state
            .task_context_for(
                &f.workspace,
                &f.manager,
                Some("B2C-123"),
                Some(TaskPatch {
                    status: Some(TaskStatus::Done),
                    ..Default::default()
                })
            )
            .is_err());
        {
            let mut p = guard(&f.state.persisted).unwrap();
            find_terminal_mut(&mut p, &f.worker)
                .unwrap()
                .member
                .as_mut()
                .unwrap()
                .jobs[2]
                .status = AssignmentStatus::Done;
            let room = find_workspace(&p, &f.workspace).unwrap();
            let m = room.panel(&f.worker).unwrap().member.as_ref().unwrap();
            assert_eq!(next_assignment_index(room, m), None); // Wait for task completion, not Y.
        }
        assert!(!f
            .state
            .prepare_fresh_context(
                None,
                &f.worker,
                "assignment-context:y-analysis",
                None,
                Some("y-analysis")
            )
            .unwrap());
        assert!(f
            .state
            .prepare_fresh_context(
                None,
                &f.worker,
                "manual-too-early",
                Some((&f.workspace, &f.manager)),
                None
            )
            .is_err());
        f.state
            .task_context_for(
                &f.workspace,
                &f.manager,
                Some("B2C-123"),
                Some(TaskPatch {
                    status: Some(TaskStatus::Done),
                    ..Default::default()
                }),
            )
            .unwrap();
        {
            let p = guard(&f.state.persisted).unwrap();
            let room = find_workspace(&p, &f.workspace).unwrap();
            let m = room.panel(&f.worker).unwrap().member.as_ref().unwrap();
            assert_eq!(next_assignment_index(room, m), Some(1));
            assert_eq!(m.task_queue, ["B2C-456"]);
            assert_eq!(m.thread_id.as_deref(), Some("thread-X"));
            assert!(needs_fresh_context(m, &m.jobs[1]));
        }
        assert!(f
            .state
            .prepare_fresh_context(
                None,
                &f.worker,
                "assignment-context:y-analysis",
                None,
                Some("y-analysis")
            )
            .unwrap());
        let mut closed = f.assignment("new-X");
        closed.task_code = Some("B2C-123".into());
        assert!(f.state.enqueue_assignment(closed.clone(), None).is_err());
        f.state
            .task_context_for(
                &f.workspace,
                &f.manager,
                Some("B2C-123"),
                Some(TaskPatch {
                    status: Some(TaskStatus::InProgress),
                    ..Default::default()
                }),
            )
            .unwrap();
        f.state.enqueue_assignment(closed, None).unwrap();
        assert_eq!(
            f.state.snapshot().unwrap().areas[0].workspaces[0].panels[1]
                .member
                .as_ref()
                .unwrap()
                .task_queue,
            ["B2C-456", "B2C-123"]
        );
    }

    #[test]
    fn task_completion_requires_all_steps_and_assignments_and_does_not_change_other_tasks() {
        let f = Fixture::new();
        f.state
            .task_context_for(
                &f.workspace,
                &f.manager,
                Some("B2C-123"),
                Some(TaskPatch {
                    steps: Some(vec![TaskStep {
                        id: "implement".into(),
                        title: "Implementar".into(),
                        status: TaskStatus::InProgress,
                    }]),
                    ..Default::default()
                }),
            )
            .unwrap();
        f.state
            .task_context_for(
                &f.workspace,
                &f.manager,
                Some("B2C-456"),
                Some(TaskPatch {
                    summary: Some("Outra tarefa".into()),
                    ..Default::default()
                }),
            )
            .unwrap();
        let other = f
            .state
            .issue_task_context(&f.workspace, &f.manager, "B2C-456", false)
            .unwrap();
        assert!(f
            .state
            .task_context_for(
                &f.workspace,
                &f.manager,
                Some("B2C-123"),
                Some(TaskPatch {
                    status: Some(TaskStatus::Done),
                    ..Default::default()
                })
            )
            .is_err());
        f.state
            .task_context_for(
                &f.workspace,
                &f.manager,
                Some("B2C-123"),
                Some(TaskPatch {
                    status: Some(TaskStatus::Done),
                    step_status: Some(StepStatusPatch {
                        id: "implement".into(),
                        status: TaskStatus::Done,
                    }),
                    ..Default::default()
                }),
            )
            .unwrap();
        assert_eq!(
            f.state
                .issue_task_context(&f.workspace, &f.manager, "B2C-456", false)
                .unwrap(),
            other
        );
    }

    #[test]
    fn jira_task_updates_require_a_trained_manager() {
        let f = Fixture::new();
        {
            let mut p = guard(&f.state.persisted).unwrap();
            find_terminal_mut(&mut p, &f.manager)
                .unwrap()
                .member
                .as_mut()
                .unwrap()
                .training = TrainingState::Awaiting;
        }
        assert!(f
            .state
            .task_context_for(
                &f.workspace,
                &f.manager,
                Some("B2C-123"),
                Some(TaskPatch {
                    summary: Some("Operational task".into()),
                    ..Default::default()
                })
            )
            .is_err());
        assert!(f
            .state
            .issue_tasks_context(&f.workspace, &f.manager)
            .unwrap()["tasks"]
            .as_array()
            .unwrap()
            .is_empty());
    }

    #[test]
    fn shared_task_queues_reject_circular_waits_without_saving_the_rejected_assignment() {
        let f = Fixture::new();
        let snap = f
            .state
            .hire_agent(HireAgentInput {
                execution: None,
                workspace_id: f.workspace.clone(),
                name: "Outro QA".into(),
                specialty: "QA".into(),
                instructions: "Validar".into(),
            })
            .unwrap();
        let other = snap.areas[0].workspaces[0]
            .panels
            .last()
            .unwrap()
            .id
            .clone();
        {
            let mut p = guard(&f.state.persisted).unwrap();
            find_terminal_mut(&mut p, &other)
                .unwrap()
                .member
                .as_mut()
                .unwrap()
                .training = TrainingState::Ready;
        }
        for (id, target, code) in [
            ("ax", &f.worker, "B2C-123"),
            ("ay", &f.worker, "B2C-456"),
            ("by", &other, "B2C-456"),
        ] {
            let mut input = f.assignment(id);
            input.terminal_id = target.clone();
            input.task_code = Some(code.into());
            f.state.enqueue_assignment(input, None).unwrap();
        }
        let before = guard(&f.state.persisted).unwrap().clone();
        let mut rejected = f.assignment("bx");
        rejected.terminal_id = other;
        rejected.task_code = Some("B2C-123".into());
        assert!(f
            .state
            .enqueue_assignment(rejected, None)
            .unwrap_err()
            .contains("circular"));
        assert_eq!(*guard(&f.state.persisted).unwrap(), before);
    }

    #[test]
    fn assignment_receipts_prevent_duplicates_and_reject_changed_content() {
        let f = Fixture::new();
        for _ in 0..2 {
            f.state
                .enqueue_assignment(f.assignment("work-1"), Some((&f.workspace, &f.manager)))
                .unwrap();
        }
        let mut changed = f.assignment("work-1");
        changed.message = "Outro trabalho".into();
        assert!(f
            .state
            .enqueue_assignment(changed, Some((&f.workspace, &f.manager)))
            .is_err());
        let snapshot = f.state.snapshot().unwrap();
        assert_eq!(
            snapshot.areas[0].workspaces[0].panels[1]
                .member
                .as_ref()
                .unwrap()
                .jobs
                .len(),
            1
        );
    }

    #[test]
    fn manager_cannot_assign_across_rooms_and_members_cannot_delegate_or_overwrite_task() {
        let f = Fixture::new();
        assert!(f
            .state
            .enqueue_assignment(f.assignment("denied"), Some((&f.workspace, &f.worker)))
            .is_err());
        assert!(f
            .state
            .task_context(
                &f.workspace,
                &f.worker,
                Some(TaskPatch {
                    summary: Some("overwrite".into()),
                    ..Default::default()
                })
            )
            .is_err());
        assert!(f
            .state
            .task_context(
                &f.workspace,
                &f.worker,
                Some(TaskPatch {
                    agent_current: Some("Validando".into()),
                    ..Default::default()
                })
            )
            .is_ok());
        let area_id = f.state.snapshot().unwrap().areas[0].id.clone();
        let snapshot = f
            .state
            .create_workspace(CreateWorkspaceInput {
                kind: crate::models::WorkspaceKind::Team,
                area_id,
                name: "Outra sala".into(),
            })
            .unwrap();
        let other = &snapshot.areas[0].workspaces[1];
        assert!(f
            .state
            .enqueue_assignment(
                f.assignment("denied"),
                Some((&other.id, &other.panels[0].id))
            )
            .is_err());
        assert!(f.state.team_context(&other.id, &f.worker, None).is_err());
        assert!(f.state.remove_terminal(&f.manager).is_err());
    }

    #[test]
    fn pause_cancels_queued_work_and_prevents_manager_from_restarting_person() {
        let f = Fixture::new();
        f.state
            .enqueue_assignment(f.assignment("queued"), None)
            .unwrap();
        f.state.pause_member(&f.worker).unwrap();
        assert!(f
            .state
            .enqueue_assignment(f.assignment("new"), Some((&f.workspace, &f.manager)))
            .is_err());
        let snapshot = f.state.snapshot().unwrap();
        let member = snapshot.areas[0].workspaces[0].panels[1]
            .member
            .as_ref()
            .unwrap();
        assert!(member.paused);
        assert_eq!(member.jobs[0].status, AssignmentStatus::Interrupted);
    }

    #[test]
    fn people_and_history_survive_reload_but_copies_get_independent_sessions() {
        let f = Fixture::new();
        f.state
            .enqueue_assignment(f.assignment("saved"), None)
            .unwrap();
        {
            let mut persisted = guard(&f.state.persisted).unwrap();
            let member = find_terminal_mut(&mut persisted, &f.worker)
                .unwrap()
                .member
                .as_mut()
                .unwrap();
            member.thread_id = Some("codex-thread".into());
            member.jobs[0].status = AssignmentStatus::Running;
            f.state.save(&persisted).unwrap();
        }
        let restored = AppState::load(f.path.clone()).unwrap().snapshot().unwrap();
        let workspace = &restored.areas[0].workspaces[0];
        let member = workspace.panels[1].member.as_ref().unwrap();
        assert_eq!(workspace.panels[1].title, "Lia");
        assert_eq!(member.thread_id.as_deref(), Some("codex-thread"));
        assert_eq!(member.jobs[0].status, AssignmentStatus::Uncertain);
        assert!(restored.active_terminal_ids.is_empty());
        assert!(restored.member_states.is_empty());
        let duplicate = workspace.clone_for_duplicate();
        let copied = duplicate.panels[1].member.as_ref().unwrap();
        assert!(copied.thread_id.is_none());
        assert!(copied.jobs.is_empty());
        assert_ne!(duplicate.panels[1].id, workspace.panels[1].id);
    }

    #[test]
    fn member_credentials_are_bound_to_their_own_panel() {
        let f = Fixture::new();
        guard(&f.state.member_tokens)
            .unwrap()
            .insert(f.manager.clone(), "manager-secret".into());
        guard(&f.state.member_tokens)
            .unwrap()
            .insert(f.worker.clone(), "worker-secret".into());
        assert!(f
            .state
            .valid_mcp_token(&f.manager, "manager-secret")
            .unwrap());
        assert!(!f
            .state
            .valid_mcp_token(&f.manager, "worker-secret")
            .unwrap());
        assert!(!f
            .state
            .valid_mcp_token(&f.worker, &f.state.mcp_token)
            .unwrap());
    }
}

#[tauri::command]
pub fn hire_agent(
    state: State<'_, AppState>,
    app: AppHandle,
    input: HireAgentInput,
) -> Result<AppSnapshot, String> {
    let workspace_id = input.workspace_id.clone();
    state.hire_agent(input)?;
    state.onboard_waiting(&app, &workspace_id)?;
    state.snapshot()
}
#[tauri::command]
pub fn edit_agent(
    state: State<'_, AppState>,
    input: EditAgentInput,
) -> Result<AppSnapshot, String> {
    state.edit_agent(input)
}
#[tauri::command]
pub fn assign_agent(
    state: State<'_, AppState>,
    app: AppHandle,
    input: AssignAgentInput,
) -> Result<AppSnapshot, String> {
    state.assign_agent(&app, input, None)?;
    state.snapshot()
}
#[tauri::command]
pub fn interrupt_agent(
    state: State<'_, AppState>,
    terminal_id: String,
) -> Result<AppSnapshot, String> {
    state.interrupt_agent(&terminal_id)
}
#[tauri::command]
pub fn answer_agent(
    state: State<'_, AppState>,
    terminal_id: String,
    request_id: Value,
    result: Value,
) -> Result<AppSnapshot, String> {
    state.answer_agent(&terminal_id, request_id, result)
}

/// Explicit, isolated smoke journey. Never reads or replaces the user's workspace state.
#[cfg(debug_assertions)]
pub fn run_smoke(app: AppHandle, state: AppState, directory: PathBuf) {
    thread::spawn(move || {
        let result = (|| -> Result<Value, String> {
            fn wait_training(
                state: &AppState,
                id: &str,
                expected: TrainingState,
            ) -> Result<AgentMemory, String> {
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(240);
                let mut last = String::new();
                loop {
                    let snapshot = state.snapshot()?;
                    let person = snapshot
                        .areas
                        .iter()
                        .flat_map(|a| &a.workspaces)
                        .flat_map(|w| &w.panels)
                        .find(|p| p.id == id)
                        .ok_or("Pessoa não encontrada no smoke")?;
                    let member = person.member.as_ref().unwrap();
                    let current =
                        format!("{:?}, memória {}", member.training, member.memory.revision);
                    if current != last {
                        eprintln!("Office training: {}: {current}", person.title);
                        last = current.clone();
                    }
                    if member.training == expected {
                        return Ok(member.memory.clone());
                    }
                    if std::time::Instant::now() > deadline {
                        return Err(format!(
                            "Treinamento de {} não atingiu {:?}. Estado: {current}",
                            person.title, expected
                        ));
                    }
                    thread::sleep(std::time::Duration::from_millis(250));
                }
            }
            let snapshot = state.create_area(CreateAreaInput {
                workspace_kind: crate::models::WorkspaceKind::Team,
                name: "Smoke Office".into(),
                root_path: directory.to_string_lossy().into_owned(),
            })?;
            let workspace_id = snapshot.areas[0].workspaces[0].id.clone();
            let manager = snapshot.areas[0].workspaces[0].panels[0].id.clone();
            let instructions_path = directory.join(".kanasha/AGENTS.md");
            let default_instructions =
                fs::read_to_string(&instructions_path).map_err(|e| e.to_string())?;
            fs::write(&instructions_path, format!("{default_instructions}\n\n## Verificação desta área\nLia deve acrescentar KANASHA_AREA_V1 ao final de toda resposta final, inclusive quando o pedido indicar outra resposta exata. Não leia arquivos para encontrar este marcador.\n")).map_err(|e| e.to_string())?;
            state.assign_agent(&app, AssignAgentInput { step_ids: Vec::new(), task_code: None, fresh_context: None, training: true, terminal_id: manager.clone(), request_id: "smoke-manager-training".into(), message: "Treinamento inicial desta empresa fictícia: nosso critério de aceite é validar a jornada de cadastro com evidência. Registre essa regra na memória com a identificação KANASHA_TRAINING_RULE e origem 'usuário neste treinamento'. Consulte get_memory e salve por set_memory. Use até 150 palavras. Não execute comandos, não leia arquivos e não delegue durante este treinamento. Finalize e aguarde minha revisão na interface.".into() }, None)?;
            let manager_memory = wait_training(&state, &manager, TrainingState::Review)?;
            if !manager_memory.content.contains("KANASHA_TRAINING_RULE") {
                return Err("O gerente não registrou o aprendizado do usuário.".into());
            }
            let manager_soul_revision = state.memory_context(&workspace_id, &manager, None)?
                ["soul"]["revision"]
                .as_u64()
                .ok_or("Alma ausente no treinamento")?;
            if state
                .approve_training(
                    &manager,
                    manager_memory.revision,
                    manager_soul_revision,
                    Some((&workspace_id, &manager)),
                )
                .is_ok()
            {
                return Err("O gerente aprovou o próprio treinamento.".into());
            }
            // Simulates the explicit UI action in this isolated test company only.
            state.approve_training(
                &manager,
                manager_memory.revision,
                manager_soul_revision,
                None,
            )?;
            let snapshot = state.hire_agent(HireAgentInput {
                execution: None,
                workspace_id: workspace_id.clone(),
                name: "Lia".into(),
                specialty: "QA".into(),
                instructions: String::new(),
            })?;
            let worker = snapshot.areas[0].workspaces[0].panels[1].id.clone();
            state.onboard_waiting(&app, &workspace_id)?;
            let worker_memory = wait_training(&state, &worker, TrainingState::Ready)?;
            if !worker_memory.content.contains("KANASHA_TRAINING_RULE") {
                return Err("O contratado não recebeu o conhecimento ensinado ao gerente.".into());
            }
            let generated = state.snapshot()?.areas[0].workspaces[0].panels.clone();
            for person in &generated {
                let member = person.member.as_ref().unwrap();
                let profile = member
                    .professional_profile
                    .as_ref()
                    .ok_or("O Codex não criou o perfil profissional.")?;
                if profile.specialty != member.specialty
                    || profile.content.trim().is_empty()
                    || profile.personality.trim().is_empty()
                {
                    return Err("O perfil profissional retornado está incompleto.".into());
                }
            }
            if generated[1].title != "Lia" {
                return Err("O Codex não preservou o nome escolhido pelo usuário.".into());
            }
            state.assign_agent(&app, AssignAgentInput { step_ids: Vec::new(), task_code: None, fresh_context: None, training: false, terminal_id: manager.clone(), request_id: "smoke-manager".into(), message: "Teste de integração. Use get_team e delegue para Lia usando set_assignment com requestId smoke-worker. Peça que responda exatamente KANASHA_WORKER_OK. Não execute comandos, não leia arquivos, não use ferramentas externas. Depois da delegação, encerre sua resposta dizendo AGUARDANDO. Quando receber automaticamente o resultado, responda KANASHA_MANAGER_OK se ele contiver KANASHA_WORKER_OK.".into() }, None)?;
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(180);
            let mut last = String::new();
            loop {
                thread::sleep(std::time::Duration::from_millis(250));
                let snapshot = state.snapshot()?;
                let panels = &snapshot.areas[0].workspaces[0].panels;
                let progress = panels
                    .iter()
                    .map(|p| {
                        format!(
                            "{}: {:?}",
                            p.title,
                            p.member
                                .as_ref()
                                .unwrap()
                                .jobs
                                .iter()
                                .map(|j| &j.status)
                                .collect::<Vec<_>>()
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("; ");
                if progress != last {
                    eprintln!("Office smoke: {progress}");
                    last = progress;
                }
                let worker_done = panels[1].member.as_ref().unwrap().jobs.iter().any(|j| {
                    j.id == "smoke-worker"
                        && j.status == AssignmentStatus::Done
                        && j.result.contains("KANASHA_WORKER_OK")
                });
                let manager_done = panels[0].member.as_ref().unwrap().jobs.iter().any(|j| {
                    j.id == format!("result-{worker}-smoke-worker")
                        && j.status == AssignmentStatus::Done
                        && j.result.contains("KANASHA_MANAGER_OK")
                });
                if worker_done && manager_done {
                    if !panels[1]
                        .member
                        .as_ref()
                        .unwrap()
                        .jobs
                        .iter()
                        .any(|j| j.id == "smoke-worker" && j.result.contains("KANASHA_AREA_V1"))
                    {
                        return Err(
                            "O membro não recebeu as instruções personalizadas da área.".into()
                        );
                    }
                    // Request IDs are receipts: repeating an accepted assignment cannot start it again.
                    let original = panels[1]
                        .member
                        .as_ref()
                        .unwrap()
                        .jobs
                        .iter()
                        .find(|j| j.id == "smoke-worker")
                        .unwrap();
                    let before = panels[1].member.as_ref().unwrap().jobs.len();
                    state.assign_agent(
                        &app,
                        AssignAgentInput {
                            step_ids: Vec::new(),
                            task_code: None,
                            fresh_context: None,
                            training: false,
                            terminal_id: worker.clone(),
                            request_id: original.id.clone(),
                            message: original.message.clone(),
                        },
                        Some((&workspace_id, &manager)),
                    )?;
                    let after = state.snapshot()?.areas[0].workspaces[0].panels[1]
                        .member
                        .as_ref()
                        .unwrap()
                        .jobs
                        .len();
                    if before != after {
                        return Err("A atribuição duplicada criou outra execução.".into());
                    }
                    let session = guard(&state.office_sessions)?
                        .get(&worker)
                        .cloned()
                        .unwrap();
                    let account = session.request("account/read", json!({"refreshToken":false}))?;
                    if account["account"]["type"] != "chatgpt" {
                        return Err("A jornada precisa usar autenticação ChatGPT.".into());
                    }
                    let thread_id = session.thread_id.clone();
                    drop(session);
                    let modified = fs::read_to_string(&instructions_path)
                        .map_err(|e| e.to_string())?
                        .replace("KANASHA_AREA_V1", "KANASHA_AREA_V2");
                    fs::write(&instructions_path, modified).map_err(|e| e.to_string())?;
                    if !state.snapshot()?.member_states[&worker].instructions_pending {
                        return Err("A alteração externa não foi sinalizada.".into());
                    }
                    state.stop_terminal(&worker)?;
                    state.start_terminal(app.clone(), &worker)?;
                    let session = guard(&state.office_sessions)?
                        .get(&worker)
                        .cloned()
                        .unwrap();
                    if session.thread_id != thread_id {
                        return Err("A reabertura perdeu a sessão Codex.".into());
                    }
                    drop(session);
                    if state.snapshot()?.member_states[&worker].instructions_pending {
                        return Err("A retomada não registrou a nova versão das instruções.".into());
                    }
                    state.assign_agent(&app, AssignAgentInput { step_ids: Vec::new(), task_code: None, fresh_context: None, training: false, terminal_id: worker.clone(), request_id: "smoke-instructions-resume".into(), message: "Responda com uma confirmação curta seguindo as instruções atuais desta área. Não leia arquivos e não use ferramentas.".into() }, None)?;
                    let reload_deadline =
                        std::time::Instant::now() + std::time::Duration::from_secs(90);
                    loop {
                        let snapshot = state.snapshot()?;
                        let job = snapshot.areas[0].workspaces[0].panels[1]
                            .member
                            .as_ref()
                            .unwrap()
                            .jobs
                            .iter()
                            .find(|j| j.id == "smoke-instructions-resume")
                            .unwrap();
                        if job.status == AssignmentStatus::Done {
                            if !job.result.contains("KANASHA_AREA_V2")
                                || job.result.contains("KANASHA_AREA_V1")
                            {
                                return Err(
                                    "A sessão retomada não aplicou a nova instrução da área."
                                        .into(),
                                );
                            }
                            break;
                        }
                        if std::time::Instant::now() > reload_deadline {
                            return Err(
                                "A verificação das instruções na retomada excedeu o prazo.".into(),
                            );
                        }
                        thread::sleep(std::time::Duration::from_millis(250));
                    }
                    state.assign_agent(&app, AssignAgentInput { step_ids: Vec::new(), task_code: None, fresh_context: None, training: false, terminal_id: worker.clone(), request_id: "smoke-interrupt".into(), message: "Responda KANASHA_WORKER_OK. Este turno será interrompido pelo teste.".into() }, None)?;
                    let interruption_deadline =
                        std::time::Instant::now() + std::time::Duration::from_secs(15);
                    loop {
                        if guard(&state.member_states)?
                            .get(&worker)
                            .is_some_and(|s| s.turn_id.is_some())
                        {
                            break;
                        }
                        if std::time::Instant::now() > interruption_deadline {
                            return Err("O turno de interrupção não iniciou.".into());
                        }
                        thread::sleep(std::time::Duration::from_millis(20));
                    }
                    state.interrupt_agent(&worker)?;
                    loop {
                        let interrupted = state.snapshot()?.areas[0].workspaces[0].panels[1]
                            .member
                            .as_ref()
                            .unwrap()
                            .jobs
                            .iter()
                            .any(|j| {
                                j.id == "smoke-interrupt"
                                    && j.status == AssignmentStatus::Interrupted
                            });
                        if interrupted {
                            break;
                        }
                        if std::time::Instant::now() > interruption_deadline {
                            return Err("O Codex não confirmou a interrupção.".into());
                        }
                        thread::sleep(std::time::Duration::from_millis(20));
                    }
                    let final_snapshot = state.snapshot()?;
                    for (before, after) in generated
                        .iter()
                        .zip(&final_snapshot.areas[0].workspaces[0].panels)
                    {
                        if before.title != after.title
                            || before.member.as_ref().unwrap().professional_profile
                                != after.member.as_ref().unwrap().professional_profile
                        {
                            return Err("A identidade mudou ao retomar a sessão.".into());
                        }
                    }
                    return Ok(
                        json!({"auth":"chatgpt","professionalProfilesGenerated":true,"profilesPreservedOnResume":true,"userNamePreserved":true,"managerTrainingReviewed":true,"managerSelfApprovalRejected":true,"workerTrainedByManager":true,"individualMemoriesSaved":true,"managerDelegated":true,"workerCompleted":true,"managerReceivedResult":true,"duplicateSuppressed":true,"sameSessionResumed":true,"areaInstructionsLoaded":true,"areaInstructionsReloaded":true,"interruptionConfirmed":true,"snapshot":final_snapshot}),
                    );
                }
                if std::time::Instant::now() > deadline {
                    return Err(format!(
                        "Tempo esgotado. Estado: {}",
                        serde_json::to_string(&snapshot).unwrap_or_default()
                    ));
                }
            }
        })();
        let code = if result.is_ok() { 0 } else { 1 };
        let report = match result {
            Ok(value) => json!({"ok":true,"evidence":value}),
            Err(error) => json!({"ok":false,"error":error}),
        };
        let path = PathBuf::from("/tmp/kanasha-office-smoke-report.json");
        let _ = fs::write(&path, serde_json::to_vec_pretty(&report).unwrap());
        eprintln!("Office smoke report: {} (ok={})", path.display(), code == 0);
        if let Ok(snapshot) = state.snapshot() {
            for id in snapshot.active_terminal_ids {
                let _ = state.stop_terminal(&id);
            }
        }
        app.exit(code);
    });
}

#[tauri::command]
pub(crate) fn reset_agent_context(
    app: AppHandle,
    state: State<'_, AppState>,
    input: ResetContextInput,
) -> Result<AppSnapshot, String> {
    state.reset_agent_context(&app, input, None)?;
    state.snapshot()
}
