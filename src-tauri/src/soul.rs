//! A stable, reviewed identity; operational memory never changes it implicitly.
use crate::{models::*, *};

pub(crate) fn review(
    member: &mut TeamMember,
    expected_revision: u64,
    reviewer: &str,
) -> Result<(), String> {
    if member.soul.revision != expected_revision {
        return Err("A alma mudou. Reabra e revise a versão atual antes de aprovar.".into());
    }
    if member.soul.content.trim().is_empty() {
        return Err("Consolide a alma a partir do treinamento inicial e revise seu conteúdo antes de aprovar.".into());
    }
    if member
        .soul
        .approved
        .as_ref()
        .is_some_and(|a| a.revision == expected_revision)
    {
        return Ok(());
    }
    member.soul.approved = Some(ApprovedSoul {
        content: member.soul.content.clone(),
        revision: expected_revision,
        approved_at: unix_timestamp()?,
        approved_by: reviewer.into(),
    });
    Ok(())
}

impl AppState {
    pub(crate) fn update_soul(
        &self,
        terminal_id: &str,
        content: String,
        expected_revision: u64,
        author: Option<(&str, &str)>,
    ) -> Result<AgentSoul, String> {
        let content = task_text(content, 12000, "alma")?;
        if content.trim().is_empty() {
            return Err(
                "Registre identidade, missão e responsabilidades aprendidas no treinamento.".into(),
            );
        }
        let _gate = guard(&self.office_gate)?;
        let mut persisted = guard(&self.persisted)?;
        if let Some((workspace_id, caller)) = author {
            // Check identity and room before accepting even an idempotent write.
            let workspace = find_workspace(&persisted, workspace_id)?;
            if caller != terminal_id
                || workspace
                    .panel(caller)
                    .and_then(|p| p.member.as_ref())
                    .is_none()
            {
                return Err("Cada pessoa propõe somente sua própria alma, na sua sala.".into());
            }
        }
        let member = find_terminal_mut(&mut persisted, terminal_id)?
            .member
            .as_ref()
            .ok_or("Pessoa não encontrada.")?;
        if member.soul.content == content {
            return Ok(member.soul.clone());
        }
        if member.soul.revision != expected_revision {
            return Err("A alma mudou. Consulte a versão atual antes de salvar; seu rascunho foi preservado.".into());
        }
        if author.is_some() && member.training == TrainingState::Ready {
            return Err("Demandas e memória não redefinem sua alma. Uma nova proposta exige treinamento complementar explícito pelo usuário ou gerente.".into());
        }
        let mut candidate = persisted.clone();
        let member = find_terminal_mut(&mut candidate, terminal_id)?
            .member
            .as_mut()
            .unwrap();
        member.soul.content = content;
        member.soul.revision += 1;
        member.soul.updated_at = Some(unix_timestamp()?);
        member.soul.updated_by = Some(author.map(|(_, id)| id).unwrap_or("user").into());
        let result = member.soul.clone();
        self.save(&candidate)?;
        *persisted = candidate;
        Ok(result)
    }

    pub(crate) fn approve_soul(
        &self,
        terminal_id: &str,
        expected_revision: u64,
    ) -> Result<(), String> {
        let _gate = guard(&self.office_gate)?;
        let mut persisted = guard(&self.persisted)?;
        let mut candidate = persisted.clone();
        let member = find_terminal_mut(&mut candidate, terminal_id)?
            .member
            .as_mut()
            .ok_or("Pessoa não encontrada.")?;
        crate::learning::require_reviewable(self, terminal_id, member)?;
        review(member, expected_revision, "user")?;
        self.save(&candidate)?;
        *persisted = candidate;
        Ok(())
    }
}

#[tauri::command]
pub fn set_agent_soul(
    state: State<'_, AppState>,
    app: AppHandle,
    terminal_id: String,
    content: String,
    expected_revision: u64,
) -> Result<AppSnapshot, String> {
    state.update_soul(&terminal_id, content, expected_revision, None)?;
    let _ = app.emit("task-state-changed", &terminal_id);
    state.snapshot()
}

#[tauri::command]
pub fn approve_agent_soul(
    state: State<'_, AppState>,
    app: AppHandle,
    terminal_id: String,
    expected_revision: u64,
) -> Result<AppSnapshot, String> {
    state.approve_soul(&terminal_id, expected_revision)?;
    let _ = app.emit("task-state-changed", &terminal_id);
    state.snapshot()
}
