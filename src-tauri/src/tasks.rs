use crate::{models::*, *};
use serde_json::{json, Value};

pub(crate) fn assignments(workspace: &Workspace, code: &str) -> Vec<Value> {
    workspace.panels.iter().flat_map(|panel| panel.member.iter().flat_map(move |member| member.jobs.iter().filter(move |job| job.task_code.as_deref() == Some(code)).map(move |job| json!({"terminalId":panel.id,"name":panel.title,"assignmentId":job.id,"stepIds":job.step_ids,"status":job.status,"delivery":crate::deliveries::summary(job)})))).collect()
}

impl AppState {
    pub(crate) fn issue_tasks_context(&self, room_id: &str, caller: &str) -> Result<Value, String> {
        let p = guard(&self.persisted)?;
        let room = find_workspace(&p, room_id)?;
        if room.panel(caller).is_none() {
            return Err("O chamador não pertence à sala.".into());
        }
        Ok(
            json!({"tasks":room.tasks.iter().map(|task| json!({"taskCode":task.task_code,"status":task.state.status,"summary":task.state.summary.chars().take(240).collect::<String>(),"revision":task.state.revision,"stepCount":task.state.steps.len(),"assignments":assignments(room, &task.task_code)})).collect::<Vec<_>>()}),
        )
    }

    pub(crate) fn issue_task_context(
        &self,
        room_id: &str,
        caller: &str,
        code: &str,
        steps: bool,
    ) -> Result<Value, String> {
        let code = office::task_code(code.into())?;
        let context = self.task_context_for(room_id, caller, Some(&code), None)?;
        let p = guard(&self.persisted)?;
        let room = find_workspace(&p, room_id)?;
        let links = assignments(room, &code);
        if steps {
            Ok(
                json!({"taskCode":code,"steps":context.task.steps.iter().map(|step| json!({"id":step.id,"title":step.title,"status":step.status,"assignments":links.iter().filter(|link| link["stepIds"].as_array().is_some_and(|ids| ids.iter().any(|id| id.as_str() == Some(&step.id)))).collect::<Vec<_>>()})).collect::<Vec<_>>(),"revision":context.task.revision,"updatedAt":context.task.updated_at}),
            )
        } else {
            Ok(
                json!({"taskCode":code,"status":context.task.status,"summary":context.task.summary,"current":context.task.current,"next":context.task.next,"blocker":context.task.blocker,"revision":context.task.revision,"updatedAt":context.task.updated_at,"assignments":links}),
            )
        }
    }
}
