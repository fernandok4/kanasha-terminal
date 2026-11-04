//! Each manager task owns a resumable Codex thread; the empty key is General.
use crate::{models::*, *};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DeleteConversationInput {
    pub terminal_id: String,
    pub task_code: Option<String>,
    pub expected_thread_id: Option<String>,
}

pub(crate) struct BackgroundConversation {
    pub session: Option<Arc<codex::CodexSession>>,
    pub runtime: MemberRuntime,
    pub token: Option<String>,
}

impl AppState {
    fn restore_manager_connection(
        &self,
        terminal_id: &str,
        conversation: BackgroundConversation,
    ) -> Result<(), String> {
        if let Some(session) = conversation.session {
            guard(&self.office_sessions)?.insert(terminal_id.into(), session);
        }
        if let Some(token) = conversation.token {
            guard(&self.member_tokens)?.insert(terminal_id.into(), token);
        }
        guard(&self.agent_states)?.insert(
            terminal_id.into(),
            AgentState {
                status: if conversation.runtime.request.is_some() {
                    AgentStatus::Waiting
                } else if conversation.runtime.turn_id.is_some() {
                    AgentStatus::Working
                } else {
                    AgentStatus::Idle
                },
                current: conversation.runtime.activity.clone(),
                updated_at: conversation.runtime.updated_at,
                ..Default::default()
            },
        );
        guard(&self.member_states)?.insert(terminal_id.into(), conversation.runtime);
        Ok(())
    }

    // Called with terminal_gate and office_gate held. Only the display client is
    // stopped: the server and structured connection keep the old turn alive.
    fn park_manager_conversation(
        &self,
        app: Option<&AppHandle>,
        terminal_id: &str,
        old_code: String,
        next_code: String,
    ) -> Result<(), String> {
        guard(&self.pty_generations)?.remove(terminal_id);
        let session = guard(&self.office_sessions)?.remove(terminal_id);
        let runtime = guard(&self.member_states)?
            .remove(terminal_id)
            .unwrap_or_default();
        let token = guard(&self.member_tokens)?.remove(terminal_id);
        let mut backgrounds = guard(&self.background_conversations)?;
        backgrounds.insert(
            (terminal_id.into(), old_code),
            BackgroundConversation {
                session,
                runtime,
                token,
            },
        );
        let next = backgrounds.remove(&(terminal_id.into(), next_code));
        drop(backgrounds);
        guard(&self.agent_states)?.remove(terminal_id);
        if let Some(app) = app {
            let _ = app.emit(
                "terminal-context-reset",
                TerminalOutput {
                    terminal_id: terminal_id.into(),
                    data: String::new(),
                },
            );
        }
        let stopped = if let Some(mut pty) = guard(&self.sessions)?.remove(terminal_id) {
            pty.stop()
        } else {
            Ok(())
        };
        if let Some(next) = next {
            self.restore_manager_connection(terminal_id, next)?;
        }
        stopped
    }

    pub(crate) fn prepare_delete_manager_conversation(
        &self,
        app: Option<&AppHandle>,
        input: &DeleteConversationInput,
    ) -> Result<(), String> {
        let _terminal_gate = guard(&self.terminal_gate)?;
        let _office_gate = guard(&self.office_gate)?;
        let mut persisted = guard(&self.persisted)?;
        let room = find_terminal_workspace(&persisted, &input.terminal_id)?.1;
        let member = room
            .panel(&input.terminal_id)
            .and_then(|p| p.member.as_ref())
            .filter(|m| m.manager && room.kind == WorkspaceKind::Team)
            .ok_or("Selecione o gerente de uma equipe.")?;
        if member.manager_task_code != input.task_code
            || member.thread_id != input.expected_thread_id
        {
            return Err(
                "A conversa mudou. Confira a seleção antes de confirmar a exclusão novamente."
                    .into(),
            );
        }
        if member.paused
            || member.jobs.iter().any(|j| {
                matches!(
                    j.status,
                    AssignmentStatus::Running | AssignmentStatus::Uncertain
                ) && j.task_code == member.manager_task_code
            })
            || guard(&self.member_states)?
                .get(&input.terminal_id)
                .is_some_and(|r| r.turn_id.is_some() || r.request.is_some())
        {
            return Err("Aguarde o gerente ficar disponível, sem execução ou pergunta pendente, antes de excluir a conversa.".into());
        }
        let mut candidate = persisted.clone();
        let member = find_terminal_mut(&mut candidate, &input.terminal_id)?
            .member
            .as_mut()
            .unwrap();
        member
            .manager_conversations
            .remove(input.task_code.as_deref().unwrap_or(""));
        if let Some(code) = &input.task_code {
            if !member.deleted_manager_conversations.contains(code) {
                member.deleted_manager_conversations.push(code.clone());
            }
        }
        // Return to General without saving the deleted thread back into the map.
        // General itself remains available, with a fresh conversation if deleted.
        let general = member
            .manager_conversations
            .get("")
            .cloned()
            .unwrap_or_default();
        member.thread_id = general.thread_id;
        member.session_instructions_hash = general.instructions_hash;
        member.manager_task_code = None;
        member.context_task_code = None;
        // Keep tasks, assignments and pending results intact.
        self.save(&candidate)?;
        *persisted = candidate;
        drop(persisted);
        self.disconnect_conversation(app, &input.terminal_id)?;
        let general = guard(&self.background_conversations)?
            .remove(&(input.terminal_id.clone(), String::new()));
        if let Some(general) = general {
            self.restore_manager_connection(&input.terminal_id, general)?;
        }
        Ok(())
    }

    pub(crate) fn prepare_manager_conversation(
        &self,
        app: Option<&AppHandle>,
        terminal_id: &str,
        code: Option<String>,
    ) -> Result<bool, String> {
        let code = code.map(office::task_code).transpose()?;
        let _terminal_gate = guard(&self.terminal_gate)?;
        let _office_gate = guard(&self.office_gate)?;
        let mut persisted = guard(&self.persisted)?;
        let room = find_terminal_workspace(&persisted, terminal_id)?.1;
        let member = room
            .panel(terminal_id)
            .and_then(|p| p.member.as_ref())
            .filter(|m| m.manager)
            .ok_or("Selecione o terminal do gerente.")?;
        if room.kind != WorkspaceKind::Team {
            return Err("Conversa por tarefa disponível somente em equipes.".into());
        }
        if member.manager_task_code == code {
            return Ok(false);
        }
        let room_id = room.id.clone();
        let mut candidate = persisted.clone();
        let room = find_workspace_mut(&mut candidate, &room_id)?;
        if let Some(code) = &code {
            if !room.tasks.iter().any(|t| &t.task_code == code) {
                room.tasks.push(OfficeTask {
                    task_code: code.clone(),
                    state: TaskState::default(),
                });
            }
        }
        let member = room
            .panel_mut(terminal_id)
            .unwrap()
            .member
            .as_mut()
            .unwrap();
        let current_key = member.manager_task_code.clone().unwrap_or_default();
        member.manager_conversations.insert(
            current_key.clone(),
            ManagerConversation {
                thread_id: member.thread_id.take(),
                instructions_hash: member.session_instructions_hash.take(),
                paused: member.paused,
            },
        );
        let next = member
            .manager_conversations
            .get(code.as_deref().unwrap_or(""))
            .cloned()
            .unwrap_or_default();
        member.thread_id = next.thread_id;
        member.paused = next.paused;
        member.session_instructions_hash = next.instructions_hash;
        member.context_task_code = code.clone();
        member
            .deleted_manager_conversations
            .retain(|deleted| Some(deleted) != code.as_ref());
        let next_key = code.clone().unwrap_or_default();
        member.manager_task_code = code;
        self.save(&candidate)?;
        *persisted = candidate;
        drop(persisted);
        self.park_manager_conversation(app, terminal_id, current_key, next_key)?;
        Ok(true)
    }
}

#[tauri::command]
pub(crate) fn delete_manager_conversation(
    app: AppHandle,
    state: State<'_, AppState>,
    input: DeleteConversationInput,
) -> Result<AppSnapshot, String> {
    state.prepare_delete_manager_conversation(Some(&app), &input)?;
    let result = state.start_terminal_with_mode(app.clone(), &input.terminal_id, false);
    let _ = app.emit("task-state-changed", &input.terminal_id);
    result?;
    state.wake_member(&app, &input.terminal_id);
    state.snapshot()
}

#[tauri::command]
pub(crate) fn select_manager_conversation(
    app: AppHandle,
    state: State<'_, AppState>,
    terminal_id: String,
    task_code: Option<String>,
) -> Result<AppSnapshot, String> {
    let changed = state.prepare_manager_conversation(Some(&app), &terminal_id, task_code);
    if changed.is_err() {
        let _ = app.emit("task-state-changed", &terminal_id);
    }
    changed?;
    let result = state.start_terminal_with_mode(app.clone(), &terminal_id, true);
    let _ = app.emit("task-state-changed", &terminal_id);
    result?;
    state.wake_member(&app, &terminal_id);
    state.snapshot()
}
