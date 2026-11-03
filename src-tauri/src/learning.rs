//! Persistent identity, reviewed training and individual memory for managed people.
use crate::{models::*, *};
use serde_json::{json, Value};

fn seed(id: &str) -> usize {
    id.bytes().fold(0usize, |value, byte| {
        value.wrapping_mul(31).wrapping_add(byte as usize)
    })
}

pub fn generated_name(id: &str, existing: &[String]) -> String {
    let names = [
        "Marina", "Rafael", "Lia", "Caio", "Clara", "Diego", "Sofia", "Bruno", "Helena", "André",
        "Beatriz", "Lucas", "Alice", "Pedro", "Camila", "Davi", "Isabela", "Tiago", "Nina",
        "Vitor",
    ];
    (0..names.len())
        .map(|offset| names[(seed(id) + offset) % names.len()].to_string())
        .find(|name| !existing.iter().any(|used| used.eq_ignore_ascii_case(name)))
        .unwrap_or_else(|| format!("Pessoa {}", &id[..8.min(id.len())]))
}

pub fn role_instructions(specialty: &str, manager: bool) -> String {
    if manager {
        return "Conversar com o usuário, aprender os processos da empresa, treinar a equipe e coordenar entregas.".into();
    }
    match specialty.to_lowercase().as_str() {
        "dev" | "desenvolvedor" => "Desenvolver soluções, investigar problemas e verificar alterações conforme o treinamento da equipe.",
        "qa" => "Testar regras de negócio e jornadas pelos critérios de aceite usando kanasha-flow, conforme o treinamento da equipe. Registrar cenário, resultado esperado e observado, ambiente e evidências. Se a ferramenta ou os insumos não estiverem disponíveis, informar a limitação; não declarar validação sem execução. Encaminhar falhas ao gerente e respeitar o escopo de validação combinado.",
        "po" => "Esclarecer objetivos, requisitos e prioridades, colaborando na definição de entregas úteis.",
        "marketing" => "Pesquisar público, planejar comunicação e preparar conteúdo; solicitar apoio técnico para implementação de código.",
        _ => "Atuar na função contratada, aprender os processos da equipe e colaborar em atividades compatíveis.",
    }.into()
}

pub(crate) fn finish_training(member: &mut TeamMember) {
    let Some(job) = member.jobs.iter().rev().find(|j| j.training) else {
        return;
    };
    if member.training != TrainingState::Learning
        || matches!(
            job.status,
            AssignmentStatus::Running | AssignmentStatus::Queued
        )
    {
        return;
    }
    member.training = if job.status == AssignmentStatus::Done
        && training_recorded(
            member,
            Some(job.memory_revision_before),
            Some(job.soul_revision_before),
        ) {
        TrainingState::Review
    } else {
        TrainingState::Awaiting
    };
}

pub(crate) fn training_recorded(
    member: &TeamMember,
    memory_before: Option<u64>,
    soul_before: Option<u64>,
) -> bool {
    (memory_before.is_some_and(|r| member.memory.revision > r)
        && !member.memory.content.trim().is_empty())
        || (soul_before.is_some_and(|r| member.soul.revision > r)
            && !member.soul.content.trim().is_empty())
}

pub(crate) fn require_reviewable(
    state: &AppState,
    terminal_id: &str,
    member: &TeamMember,
) -> Result<(), String> {
    if member.paused
        || guard(&state.member_states)?
            .get(terminal_id)
            .is_some_and(|r| r.turn_id.is_some())
    {
        return Err("Aguarde o fim da conversa e disponibilize a pessoa antes de aprovar.".into());
    }
    if member.jobs.iter().any(|j| {
        matches!(
            j.status,
            AssignmentStatus::Queued | AssignmentStatus::Running | AssignmentStatus::Uncertain
        )
    }) {
        return Err("Resolva as execuções pendentes antes de aprovar.".into());
    }
    Ok(())
}

pub fn instructions(member: &TeamMember) -> String {
    format!(
        "## Configuração\n\nNível: {} | Modelo: {} | Raciocínio: {}. Escolha do usuário; nível não comprova experiência.\n\n{}\n\n{}",
        member.execution.level,
        member.execution.model,
        member.execution.reasoning_effort,
        profile_instructions(member),
        learning_instructions(member)
    )
}

fn profile_instructions(member: &TeamMember) -> String {
    if let Some(profile) = &member.professional_profile {
        return format!("# Perfil profissional criado pelo Codex\n\n{}\n\nAbordagem inicial; não comprova experiência. Preserve o perfil ao retomar. Usuário, função, projeto e alma aprovada prevalecem.", profile.content);
    }
    "# Criação do perfil profissional

Consulte get_memory/get_team. Se professionalProfile estiver vazio, crie automaticamente
por set_professional_profile antes do primeiro aprendizado. Preserve exatamente o nome atual
retornado, inclusive automaticName=true; só o usuário renomeia. Não peça personalidade.
Use Markdown curto: missão, atuação, comunicação, qualidade, limites e desenvolvimento.
Descreva a abordagem da função, sem inventar experiência, qualificações ou processos.
Tetos: personality 800, content 6000 caracteres. Perfil não aprova treinamento.
Após resposta incerta, consulte antes de repetir."
        .into()
}

fn learning_instructions(member: &TeamMember) -> String {
    let identity = match &member.soul.approved {
        Some(approved) => format!("# Alma aprovada — revisão {}\n\n{}\n\nIdentidade vigente; prevalece sobre o perfil, respeitando usuário/projeto. Memória não a redefine.", approved.revision, approved.content),
        None => "# Alma ainda não aprovada\n\nPerfil provisório. Registros antigos preservam treinamento; não presuma aprovação da alma.".into(),
    };
    let pending = if member.soul.approved.as_ref().map(|a| a.revision) != Some(member.soul.revision)
        && !member.soul.content.is_empty()
    {
        format!("\n\n# Proposta de alma — revisão {}, aguardando revisão\n\n{}\n\nNão substitui a versão aprovada.", member.soul.revision, member.soul.content)
    } else {
        String::new()
    };
    format!(
        r#"{identity}{pending}

## Treinamento e registros

Personalidade inicial: {personality}
Estado neste registro: {training:?}. Consulte get_memory ao iniciar/retomar para obter o estado vigente.

- Sem Ready, faça apenas treinamento. set_soul propõe identidade durante formação
  inicial/complementar explícita; demandas normais não alteram alma. Salvar não aprova.
  Só o usuário aprova o gerente. Gerente treinado revisa alma/memória do membro após
  o fim da conversa e usa set_training_complete com as duas revisões lidas.
- Alma: princípios realmente ensinados em `## Missão`, `## Atuação e critérios`,
  `## Limites`, `## Origem e dúvidas`. Prefira até 2000 caracteres (teto 12000).
  Preserve a identidade aprovada até revisão; não inclua andamento de tarefas.
- Memória: índice de experiências próprias. Use `- **AAAA-MM-DD — Assunto:**
  aprendizado. Origem, resultado confirmado e pendência relevante. [Detalhes](caminho-real)`.
  Uma a três frases por lembrança; omita link inexistente. Prefira até 4000 caracteres
  (teto 16000), sem perder decisões válidas. Não copie alma, AGENTS ou registros alheios.
- Registre treinamento com quem ensinou, assuntos e estado confirmado da revisão;
  realizado não significa aprovado. Nas demandas, salve só aprendizado útil.
- Detalhes ficam na tarefa ou em equipes/<workspace-id>/memorias/<terminal-id>/,
  usando IDs reais da sessão. Confira arquivos antes de referenciar; leia só os
  pertinentes. Condense lembranças antigas preservando decisões, links e pendências.
- set_memory substitui todo o texto: preserve conhecimento válido. Use expectedRevision
  de memory.revision; em set_soul, de soul.revision. Em conflito/resposta incerta,
  releia antes de gravar. Cada pessoa escreve só seus registros; nunca salve segredos.

# Memória individual — revisão {revision}

{memory}"#,
        personality = if member.professional_profile.is_some() {
            &member.personality
        } else {
            "A definir no perfil."
        },
        training = member.training,
        revision = member.memory.revision,
        memory = if member.memory.content.is_empty() {
            "Sem lembranças registradas."
        } else {
            &member.memory.content
        },
    )
}

fn authorize<'a>(
    persisted: &'a PersistedState,
    workspace_id: &str,
    caller: &str,
    target: &str,
) -> Result<&'a TeamMember, String> {
    let workspace = find_workspace(persisted, workspace_id)?;
    let source = workspace
        .panel(caller)
        .and_then(|p| p.member.as_ref())
        .ok_or("O chamador não faz parte da equipe.")?;
    if caller != target && !source.manager {
        return Err("Somente o gerente pode consultar a memória de outro membro.".into());
    }
    workspace
        .panel(target)
        .and_then(|p| p.member.as_ref())
        .ok_or("Pessoa não encontrada nesta sala.".into())
}

impl AppState {
    pub(crate) fn set_professional_profile(
        &self,
        workspace_id: &str,
        caller: &str,
        input: ProfessionalProfileInput,
    ) -> Result<ProfessionalProfile, String> {
        let profile_name = name(&input.name, "pessoa")?;
        let personality = task_text(input.personality, 800, "personalidade")?;
        let content = task_text(input.content, 6000, "perfil profissional")?;
        if personality.trim().is_empty() || content.trim().is_empty() {
            return Err("Informe a personalidade e o perfil profissional.".into());
        }
        let _gate = guard(&self.office_gate)?;
        let mut persisted = guard(&self.persisted)?;
        let member = authorize(&persisted, workspace_id, caller, caller)?;
        if let Some(existing) = &member.professional_profile {
            if existing.name == profile_name
                && existing.personality == personality
                && existing.content == content
            {
                return Ok(existing.clone());
            }
            return Err(
                "O perfil já foi criado. Consulte get_memory e preserve a identidade existente."
                    .into(),
            );
        }
        let room = find_workspace(&persisted, workspace_id)?;
        let panel = room.panel(caller).unwrap();
        if panel.title != profile_name {
            return Err(format!(
                "Preserve o nome atual: {}. Somente o usuário pode alterá-lo pela interface.",
                panel.title
            ));
        }
        let profile = ProfessionalProfile {
            name: profile_name.clone(),
            personality: personality.clone(),
            content,
            specialty: member.specialty.clone(),
            generated_at: unix_timestamp()?,
        };
        // Save a candidate first: failed persistence must not look like a successful creation.
        let mut candidate = persisted.clone();
        let panel = find_terminal_mut(&mut candidate, caller)?;
        let member = panel.member.as_mut().unwrap();
        member.personality = personality;
        member.professional_profile = Some(profile.clone());
        member.automatic_name = false;
        self.save(&candidate)?;
        *persisted = candidate;
        Ok(profile)
    }

    pub(crate) fn memory_context(
        &self,
        workspace_id: &str,
        caller: &str,
        target: Option<&str>,
    ) -> Result<Value, String> {
        let persisted = guard(&self.persisted)?;
        let target = target.unwrap_or(caller);
        let member = authorize(&persisted, workspace_id, caller, target)?;
        let (area, room) = find_terminal_workspace(&persisted, target)?;
        Ok(json!({"memory":member.memory,"training":member.training,
            "professionalProfile":member.professional_profile,"soul":member.soul,
            "name":room.panel(target).unwrap().title,"automaticName":member.automatic_name,
            "specialty":member.specialty,"company":area.name,"workspace":room.name}))
    }

    pub(crate) fn update_memory(
        &self,
        terminal_id: &str,
        content: String,
        expected_revision: u64,
        author: Option<(&str, &str)>,
    ) -> Result<AgentMemory, String> {
        let content = task_text(content, 16000, "memória")?;
        if content.trim().is_empty() {
            return Err("Registre o aprendizado antes de salvar a memória.".into());
        }
        let _gate = guard(&self.office_gate)?;
        let mut persisted = guard(&self.persisted)?;
        if let Some((workspace_id, caller)) = author {
            authorize(&persisted, workspace_id, caller, terminal_id)?;
            if caller != terminal_id {
                return Err("Cada pessoa escreve somente sua própria memória.".into());
            }
        }
        let member = find_terminal_mut(&mut persisted, terminal_id)?
            .member
            .as_mut()
            .ok_or("Pessoa não encontrada.")?;
        if author.is_some() && member.professional_profile.is_none() {
            return Err("Crie seu perfil com set_professional_profile antes de registrar aprendizado. Consulte get_memory para obter o contexto.".into());
        }
        if expected_revision != member.memory.revision {
            if member.memory.content == content {
                return Ok(member.memory.clone());
            }
            return Err(
                "A memória mudou desde sua leitura. Consulte a versão atual antes de salvar."
                    .into(),
            );
        }
        member.memory = AgentMemory {
            content,
            revision: member.memory.revision + 1,
            updated_at: Some(unix_timestamp()?),
            updated_by: Some(author.map(|(_, id)| id).unwrap_or("user").into()),
        };
        let memory = member.memory.clone();
        self.save(&persisted)?;
        Ok(memory)
    }

    pub(crate) fn approve_training(
        &self,
        terminal_id: &str,
        expected_revision: u64,
        expected_soul_revision: u64,
        author: Option<(&str, &str)>,
    ) -> Result<String, String> {
        let _gate = guard(&self.office_gate)?;
        let mut persisted = guard(&self.persisted)?;
        let (_, workspace) = find_terminal_workspace(&persisted, terminal_id)?;
        let workspace_id = workspace.id.clone();
        if let Some((room, caller)) = author {
            let target = authorize(&persisted, room, caller, terminal_id)?;
            let source = find_workspace(&persisted, room)?
                .panel(caller)
                .unwrap()
                .member
                .as_ref()
                .unwrap();
            if caller == terminal_id
                || target.manager
                || !source.manager
                || source.training != TrainingState::Ready
            {
                return Err("O usuário aprova o gerente; um gerente treinado aprova os outros membros da sua sala.".into());
            }
        }
        let member = find_terminal_mut(&mut persisted, terminal_id)?
            .member
            .as_mut()
            .ok_or("Pessoa não encontrada.")?;
        if member.memory.revision != expected_revision {
            return Err(
                "A memória mudou. Reabra e revise a versão atual antes de concluir o treinamento."
                    .into(),
            );
        }
        require_reviewable(self, terminal_id, member)?;
        if member.soul.revision != expected_soul_revision {
            return Err(
                "A alma mudou. Reabra e revise a versão atual antes de concluir o treinamento."
                    .into(),
            );
        }
        if member.training == TrainingState::Ready {
            if member
                .soul
                .approved
                .as_ref()
                .is_some_and(|a| a.revision == expected_soul_revision)
            {
                return Ok(workspace_id);
            }
            return Err(
                "Revise e aprove a alma pela interface; esta pessoa já está treinada.".into(),
            );
        }
        if member.training != TrainingState::Review {
            return Err("Aguarde a conclusão de uma sessão de treinamento com alma ou memória registrada antes de aprovar.".into());
        }
        // Persist approval of both the reviewed identity and training atomically.
        let mut candidate = persisted.clone();
        let member = find_terminal_mut(&mut candidate, terminal_id)?
            .member
            .as_mut()
            .unwrap();
        crate::soul::review(
            member,
            expected_soul_revision,
            author.map(|(_, id)| id).unwrap_or("user"),
        )?;
        member.training = TrainingState::Ready;
        self.save(&candidate)?;
        *persisted = candidate;
        Ok(workspace_id)
    }

    pub(crate) fn onboard(
        &self,
        app: &AppHandle,
        terminal_id: &str,
        request_id: &str,
    ) -> Result<(), String> {
        let (manager_id, message) = {
            let persisted = guard(&self.persisted)?;
            let (_, workspace) = find_terminal_workspace(&persisted, terminal_id)?;
            let target = workspace.panel(terminal_id).unwrap();
            let member = target.member.as_ref().ok_or("Pessoa não encontrada.")?;
            if member.manager || member.training != TrainingState::Awaiting {
                return Ok(());
            }
            let manager = workspace
                .panels
                .iter()
                .find(|p| p.member.as_ref().is_some_and(|m| m.manager))
                .ok_or("Gerente não encontrado.")?;
            if manager.member.as_ref().unwrap().training != TrainingState::Ready {
                return Ok(());
            }
            (manager.id.clone(), format!("Integre {} ({}), terminalId: {}.\n\n**Orientações da função:** {}\n\nPrepare treinamento com processos, ferramentas, aceite, limites e referências pertinentes da área e de sua alma aprovada. Envie set_assignment(training=true, requestId único). O membro consolida a própria alma por set_soul e salva uma lembrança datada por set_memory; não copie seus registros nem execute trabalho operacional. Aguarde retorno automático, leia get_memory do membro e revise alma/memória. Se correto, aprove por set_training_complete com expectedRevision da memória e expectedSoulRevision da alma; caso contrário, complemente a formação.", target.title, member.specialty, target.id, member.instructions))
        };
        self.assign_agent(
            app,
            AssignAgentInput {
                step_ids: Vec::new(),
                task_code: None,
                fresh_context: None,
                terminal_id: manager_id,
                request_id: request_id.into(),
                message,
                training: false,
            },
            None,
        )?;
        Ok(())
    }

    pub(crate) fn onboard_waiting(
        &self,
        app: &AppHandle,
        workspace_id: &str,
    ) -> Result<(), String> {
        let ids = {
            let persisted = guard(&self.persisted)?;
            find_workspace(&persisted, workspace_id)?
                .panels
                .iter()
                .filter(|p| {
                    p.member.as_ref().is_some_and(|m| {
                        !m.manager && m.training == TrainingState::Awaiting && m.jobs.is_empty()
                    })
                })
                .map(|p| p.id.clone())
                .collect::<Vec<_>>()
        };
        for id in ids {
            // A paused/busy manager keeps the hire intact; the UI offers an explicit retry.
            if let Err(error) = self.onboard(app, &id, &format!("onboard-{id}")) {
                if let Ok(mut states) = self.member_states.lock() {
                    states.entry(id).or_default().activity =
                        format!("Treinamento aguardando gerente: {error}");
                }
            }
        }
        Ok(())
    }
}

#[tauri::command]
pub fn set_agent_memory(
    state: State<'_, AppState>,
    app: AppHandle,
    terminal_id: String,
    content: String,
    expected_revision: u64,
) -> Result<AppSnapshot, String> {
    state.update_memory(&terminal_id, content, expected_revision, None)?;
    let _ = app.emit("task-state-changed", &terminal_id);
    state.snapshot()
}

#[tauri::command]
pub fn complete_training(
    state: State<'_, AppState>,
    app: AppHandle,
    terminal_id: String,
    expected_revision: u64,
    expected_soul_revision: u64,
) -> Result<AppSnapshot, String> {
    let workspace = state.approve_training(
        &terminal_id,
        expected_revision,
        expected_soul_revision,
        None,
    )?;
    state.onboard_waiting(&app, &workspace)?;
    let _ = app.emit("task-state-changed", &terminal_id);
    state.snapshot()
}

#[tauri::command]
pub fn request_onboarding(
    state: State<'_, AppState>,
    app: AppHandle,
    terminal_id: String,
    request_id: String,
) -> Result<AppSnapshot, String> {
    state.onboard(&app, &terminal_id, &request_id)?;
    state.snapshot()
}
