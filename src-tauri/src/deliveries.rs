//! Versioned declarations and manager reviews. Execution and queue state are independent.
use crate::{
    delivery_git::{self, GitComparison, GitSnapshot},
    models::*,
    *,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RegisterInput {
    pub assignment_id: String,
    pub request_id: String,
    pub checkout_path: String,
    pub note: String,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkContext {
    pub id: String,
    pub registered_by: String,
    pub request: RegisterInput,
    pub baseline: GitSnapshot,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceKind {
    Code,
    Gitlab,
    MergeRequest,
    Test,
    Review,
    Deployment,
    Investigation,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Evidence {
    pub id: String,
    pub kind: EvidenceKind,
    pub reference: String,
    pub version: String,
    pub details: String,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryKind {
    CodeChange,
    Investigation,
    Validation,
    Documentation,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SubmitInput {
    pub assignment_id: String,
    pub request_id: String,
    pub expected_revision: u64,
    pub kind: DeliveryKind,
    pub summary: String,
    pub limitations: String,
    pub context_ids: Vec<String>,
    pub evidence: Vec<Evidence>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryObservation {
    pub context_id: String,
    pub final_snapshot: GitSnapshot,
    pub comparison: GitComparison,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Delivery {
    pub revision: u64,
    pub submitted_by: String,
    pub submitted_at: u64,
    pub declaration: SubmitInput,
    pub repositories: Vec<RepositoryObservation>,
    pub reviews: Vec<DeliveryReview>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum CheckState {
    Verified,
    Unavailable,
    Divergent,
    NotChecked,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EvidenceCheck {
    pub evidence_id: String,
    pub state: CheckState,
    pub source: String,
    pub reference: String,
    pub version: String,
    pub note: String,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    Accepted,
    ChangesRequested,
    Inconclusive,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewInput {
    pub terminal_id: String,
    pub assignment_id: String,
    pub delivery_revision: u64,
    pub expected_review_revision: u64,
    pub request_id: String,
    pub decision: Decision,
    pub summary: String,
    pub checks: Vec<EvidenceCheck>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DeliveryReview {
    pub revision: u64,
    pub reviewed_by: String,
    pub reviewed_at: u64,
    pub assessment: ReviewInput,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeliveryQuery {
    pub task_code: Option<String>,
    pub terminal_id: Option<String>,
    pub assignment_id: Option<String>,
    pub revision: Option<u64>,
    pub review_revision: Option<u64>,
    #[serde(default)]
    pub offset: usize,
    pub limit: Option<usize>,
}

fn text_limit(value: &str, limit: usize, required: bool) -> Result<(), String> {
    if value.contains('\0')
        || value.chars().count() > limit
        || (required && value.trim().is_empty())
    {
        return Err(format!("Texto inválido: limite de {limit} caracteres; campos obrigatórios não podem ficar vazios."));
    }
    Ok(())
}
fn size_limit<T: Serialize>(value: &T, limit: usize) -> Result<(), String> {
    if serde_json::to_vec(value).map_err(|e| e.to_string())?.len() > limit {
        return Err("Registro extenso demais; use referências para artefatos detalhados.".into());
    }
    Ok(())
}
fn member<'a>(room: &'a Workspace, caller: &str) -> Result<&'a TeamMember, String> {
    room.panel(caller)
        .and_then(|p| p.member.as_ref())
        .ok_or_else(|| "Pessoa não pertence a esta sala.".into())
}
fn assignment<'a>(room: &'a Workspace, owner: &str, id: &str) -> Result<&'a Assignment, String> {
    member(room, owner)?
        .jobs
        .iter()
        .find(|j| j.id == id)
        .ok_or_else(|| "Atribuição não pertence a esta pessoa nesta sala.".into())
}
fn writable(room: &Workspace, owner: &str, id: &str) -> Result<(), String> {
    let person = member(room, owner)?;
    let job = assignment(room, owner, id)?;
    if person.training != TrainingState::Ready
        || job.training
        || job.status == AssignmentStatus::Queued
    {
        return Err("Registre entregas de atribuições já iniciadas, fora de treinamento.".into());
    }
    Ok(())
}
fn assignment_mut<'a>(
    p: &'a mut PersistedState,
    room_id: &str,
    owner: &str,
    id: &str,
) -> Result<&'a mut Assignment, String> {
    find_workspace_mut(p, room_id)?
        .panel_mut(owner)
        .and_then(|p| p.member.as_mut())
        .and_then(|m| m.jobs.iter_mut().find(|j| j.id == id))
        .ok_or_else(|| "Atribuição não encontrada nesta sala.".into())
}
fn retry<T: PartialEq>(original: &T, input: &T) -> Result<(), String> {
    if original == input {
        Ok(())
    } else {
        Err(
            "requestId já utilizado com outro conteúdo. Consulte a entrega antes de repetir."
                .into(),
        )
    }
}
pub fn assignment_view(job: &Assignment) -> Value {
    let mut value = json!(job);
    let object = value.as_object_mut().unwrap();
    object.remove("workContexts");
    object.remove("deliveries");
    object.insert("delivery".into(), summary(job));
    value
}

pub fn summary(job: &Assignment) -> Value {
    let latest = job.deliveries.last();
    json!({"assignmentId":job.id,"taskCode":job.task_code,"stepIds":job.step_ids,
        "executionStatus":job.status,"deliveryRevision":latest.map(|d|d.revision),
        "summary":latest.map(|d|d.declaration.summary.chars().take(300).collect::<String>()),
        "reviewDecision":latest.and_then(|d| d.reviews.last()).map(|r| &r.assessment.decision),
        "reviewRevision":latest.and_then(|d|d.reviews.last()).map(|r|r.revision),
        "contextCount":job.work_contexts.len()})
}

impl AppState {
    pub(crate) fn register_work_context(
        &self,
        room_id: &str,
        caller: &str,
        input: RegisterInput,
    ) -> Result<Value, String> {
        text_limit(&input.request_id, 120, true)?;
        text_limit(&input.checkout_path, 2000, true)?;
        text_limit(&input.note, 1500, true)?;
        if !Path::new(&input.checkout_path).is_absolute() {
            return Err("Informe o caminho absoluto do checkout.".into());
        }
        {
            let p = guard(&self.persisted)?;
            let room = find_workspace(&p, room_id)?;
            let job = assignment(room, caller, &input.assignment_id)?;
            if let Some(existing) = job
                .work_contexts
                .iter()
                .find(|c| c.request.request_id == input.request_id)
            {
                retry(&existing.request, &input)?;
                return Ok(json!(existing));
            }
            writable(room, caller, &input.assignment_id)?;
            if job.work_contexts.len() >= 4 {
                return Err("Limite de quatro checkouts por atribuição.".into());
            }
        }
        // Never hold application locks while invoking Git.
        let context = WorkContext {
            id: new_id(),
            registered_by: caller.into(),
            baseline: delivery_git::capture(&input.checkout_path),
            request: input.clone(),
        };
        size_limit(&context, 24 * 1024)?;
        let _gate = guard(&self.office_gate)?;
        let mut p = guard(&self.persisted)?;
        let room = find_workspace(&p, room_id)?;
        let job = assignment(room, caller, &input.assignment_id)?;
        if let Some(existing) = job
            .work_contexts
            .iter()
            .find(|c| c.request.request_id == input.request_id)
        {
            retry(&existing.request, &input)?;
            return Ok(json!(existing));
        }
        writable(room, caller, &input.assignment_id)?;
        if job.work_contexts.len() >= 4 {
            return Err("Limite de quatro checkouts por atribuição.".into());
        }
        let mut candidate = p.clone();
        assignment_mut(&mut candidate, room_id, caller, &input.assignment_id)?
            .work_contexts
            .push(context.clone());
        self.save(&candidate)?;
        *p = candidate;
        Ok(json!(context))
    }

    pub(crate) fn submit_delivery(
        &self,
        room_id: &str,
        caller: &str,
        input: SubmitInput,
    ) -> Result<Value, String> {
        text_limit(&input.request_id, 120, true)?;
        text_limit(&input.summary, 4000, true)?;
        text_limit(&input.limitations, 2000, false)?;
        if input.context_ids.len() > 4 || input.evidence.len() > 12 {
            return Err("Use até quatro checkouts e doze evidências.".into());
        }
        let mut ids = HashSet::new();
        for evidence in &input.evidence {
            text_limit(&evidence.id, 80, true)?;
            text_limit(&evidence.reference, 1000, true)?;
            text_limit(&evidence.version, 200, false)?;
            text_limit(&evidence.details, 2000, true)?;
            if !ids.insert(&evidence.id) {
                return Err("IDs de evidência devem ser únicos.".into());
            }
        }
        let contexts = {
            let p = guard(&self.persisted)?;
            let room = find_workspace(&p, room_id)?;
            let job = assignment(room, caller, &input.assignment_id)?;
            if let Some(existing) = job
                .deliveries
                .iter()
                .find(|d| d.declaration.request_id == input.request_id)
            {
                retry(&existing.declaration, &input)?;
                return Ok(json!({"revision":existing.revision,"recorded":true}));
            }
            writable(room, caller, &input.assignment_id)?;
            if input.expected_revision != job.deliveries.len() as u64 {
                return Err(
                    "A entrega mudou. Consulte get_deliveries e use a revisão atual.".into(),
                );
            }
            let mut selected = Vec::new();
            let mut seen = HashSet::new();
            for id in &input.context_ids {
                if !seen.insert(id) {
                    return Err("Checkout repetido.".into());
                }
                selected.push(
                    job.work_contexts
                        .iter()
                        .find(|c| &c.id == id)
                        .ok_or("Checkout não registrado nesta atribuição.")?
                        .clone(),
                );
            }
            selected
        };
        if input.kind == DeliveryKind::CodeChange && contexts.is_empty() {
            return Err("Entregas de código precisam de um checkout registrado. Investigações e validações podem ser entregues sem alteração de código.".into());
        }
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(24);
        let repositories = contexts
            .iter()
            .map(|c| {
                let final_snapshot =
                    delivery_git::capture_until(&c.request.checkout_path, deadline);
                let comparison =
                    delivery_git::compare_until(&c.baseline, &final_snapshot, deadline);
                RepositoryObservation {
                    context_id: c.id.clone(),
                    final_snapshot,
                    comparison,
                }
            })
            .collect();
        let delivery = Delivery {
            revision: input.expected_revision + 1,
            submitted_by: caller.into(),
            submitted_at: unix_timestamp()?,
            declaration: input.clone(),
            repositories,
            reviews: Vec::new(),
        };
        size_limit(&delivery, 80 * 1024)?;
        let _gate = guard(&self.office_gate)?;
        let mut p = guard(&self.persisted)?;
        let room = find_workspace(&p, room_id)?;
        let job = assignment(room, caller, &input.assignment_id)?;
        if let Some(existing) = job
            .deliveries
            .iter()
            .find(|d| d.declaration.request_id == input.request_id)
        {
            retry(&existing.declaration, &input)?;
            return Ok(json!({"revision":existing.revision,"recorded":true}));
        }
        writable(room, caller, &input.assignment_id)?;
        if input.expected_revision != job.deliveries.len() as u64 {
            return Err("A entrega mudou durante a coleta. Consulte a revisão atual.".into());
        }
        let mut candidate = p.clone();
        assignment_mut(&mut candidate, room_id, caller, &input.assignment_id)?
            .deliveries
            .push(delivery.clone());
        self.save(&candidate)?;
        *p = candidate;
        Ok(json!({"revision":delivery.revision,"recorded":true,"reviewState":"pending"}))
    }

    pub(crate) fn review_delivery(
        &self,
        room_id: &str,
        caller: &str,
        input: ReviewInput,
    ) -> Result<Value, String> {
        text_limit(&input.request_id, 120, true)?;
        text_limit(&input.summary, 4000, true)?;
        if input.checks.len() > 12 {
            return Err("Use até doze conferências.".into());
        }
        let _gate = guard(&self.office_gate)?;
        let mut p = guard(&self.persisted)?;
        let room = find_workspace(&p, room_id)?;
        let manager = member(room, caller)?;
        if !manager.manager
            || manager.training != TrainingState::Ready
            || caller == input.terminal_id
        {
            return Err(
                "Somente o gerente treinado pode conferir a entrega de outro funcionário.".into(),
            );
        }
        let job = assignment(room, &input.terminal_id, &input.assignment_id)?;
        for delivery in &job.deliveries {
            if let Some(existing) = delivery
                .reviews
                .iter()
                .find(|r| r.assessment.request_id == input.request_id)
            {
                retry(&existing.assessment, &input)?;
                return Ok(json!({"revision":existing.revision,"recorded":true}));
            }
        }
        let delivery = job
            .deliveries
            .last()
            .ok_or("Esta atribuição ainda não tem entrega estruturada.")?;
        if delivery.revision != input.delivery_revision
            || delivery.reviews.len() as u64 != input.expected_review_revision
        {
            return Err(
                "Entrega ou conferência desatualizada. Consulte get_deliveries antes de revisar."
                    .into(),
            );
        }
        let mut seen = HashSet::new();
        for check in &input.checks {
            let evidence = delivery
                .declaration
                .evidence
                .iter()
                .find(|e| e.id == check.evidence_id)
                .ok_or("Conferência aponta para evidência inexistente.")?;
            if !seen.insert(&check.evidence_id) {
                return Err("Conferência duplicada.".into());
            }
            text_limit(&check.source, 300, check.state == CheckState::Verified)?;
            text_limit(&check.reference, 1000, check.state == CheckState::Verified)?;
            text_limit(&check.version, 200, check.state == CheckState::Verified)?;
            text_limit(&check.note, 2000, true)?;
            if check.state == CheckState::Verified
                && !evidence.version.is_empty()
                && check.version != evidence.version
            {
                return Err("A versão conferida diverge da declarada. Registre divergência ou solicite uma nova entrega.".into());
            }
        }
        if seen.len() != delivery.declaration.evidence.len() {
            return Err(
                "Informe o estado de cada evidência, inclusive indisponível ou não conferida."
                    .into(),
            );
        }
        let review = DeliveryReview {
            revision: input.expected_review_revision + 1,
            reviewed_by: caller.into(),
            reviewed_at: unix_timestamp()?,
            assessment: input.clone(),
        };
        size_limit(&review, 48 * 1024)?;
        let mut candidate = p.clone();
        assignment_mut(
            &mut candidate,
            room_id,
            &input.terminal_id,
            &input.assignment_id,
        )?
        .deliveries
        .last_mut()
        .unwrap()
        .reviews
        .push(review.clone());
        self.save(&candidate)?;
        *p = candidate;
        Ok(json!({"revision":review.revision,"recorded":true,"taskUnchanged":true}))
    }

    pub(crate) fn get_deliveries(
        &self,
        room_id: &str,
        caller: &str,
        query: DeliveryQuery,
    ) -> Result<Value, String> {
        let p = guard(&self.persisted)?;
        let room = find_workspace(&p, room_id)?;
        member(room, caller)?;
        let code = query.task_code.map(office::task_code).transpose()?;
        let limit = query.limit.unwrap_or(10);
        if !(1..=20).contains(&limit) {
            return Err("limit deve estar entre 1 e 20.".into());
        }
        if query.assignment_id.is_some() {
            let owner = query
                .terminal_id
                .as_deref()
                .ok_or("Informe terminalId junto de assignmentId.")?;
            let job = assignment(room, owner, query.assignment_id.as_deref().unwrap())?;
            if code
                .as_ref()
                .is_some_and(|c| job.task_code.as_ref() != Some(c))
            {
                return Err("Atribuição não pertence à tarefa informada.".into());
            }
            let delivery = if let Some(revision) = query.revision {
                Some(
                    job.deliveries
                        .iter()
                        .find(|d| d.revision == revision)
                        .ok_or("Revisão inexistente.")?,
                )
            } else {
                job.deliveries.last()
            };
            // Return one immutable delivery version and one review at a time.
            // Previous assessments remain persisted and are summarized below.
            let selected_review = match (delivery, query.review_revision) {
                (Some(d), Some(r)) => Some(
                    d.reviews
                        .iter()
                        .find(|review| review.revision == r)
                        .ok_or("Conferência inexistente.")?,
                ),
                (Some(d), None) => d.reviews.last(),
                (None, Some(_)) => return Err("Não há entrega para conferir.".into()),
                (None, None) => None,
            };
            let detail = delivery.map(|d| {
                let mut v = json!(d);
                v["reviews"] = json!(selected_review.into_iter().collect::<Vec<_>>());
                v["reviewCount"] = json!(d.reviews.len());
                v
            });
            let result = json!({"terminalId":owner,"assignment":summary(job),"contexts":job.work_contexts,
                "delivery":detail,"versions":job.deliveries.iter().skip(query.offset).take(limit).map(|d|json!({"revision":d.revision,"submittedAt":d.submitted_at,"reviewCount":d.reviews.len()})).collect::<Vec<_>>(),
                "versionCount":job.deliveries.len(),"nextVersionOffset":(query.offset.saturating_add(limit) < job.deliveries.len()).then_some(query.offset.saturating_add(limit)),
                "notice":"Git é uma observação no momento do registro, não uma prova de autoria. Evidências externas são declaradas; conferências são registradas pelo gerente. Ausência de entrega não comprova ausência de trabalho."});
            size_limit(&result, 240 * 1024)?;
            return Ok(result);
        }
        if query.revision.is_some() || query.review_revision.is_some() {
            return Err("Informe uma atribuição para consultar sua revisão.".into());
        }
        let jobs: Vec<_> = room
            .panels
            .iter()
            .filter(|p| query.terminal_id.as_ref().is_none_or(|id| id == &p.id))
            .flat_map(|p| {
                p.member
                    .iter()
                    .flat_map(move |m| m.jobs.iter().map(move |j| (p, j)))
            })
            .filter(|(_, j)| {
                code.as_ref()
                    .is_none_or(|c| j.task_code.as_ref() == Some(c))
            })
            .collect();
        let items: Vec<_> = jobs
            .iter()
            .skip(query.offset)
            .take(limit)
            .map(|(p, j)| json!({"terminalId":p.id,"name":p.title,"assignment":summary(j)}))
            .collect();
        let next = query.offset.saturating_add(items.len());
        Ok(
            json!({"items":items,"total":jobs.len(),"nextOffset":(next < jobs.len()).then_some(next)}),
        )
    }
}

pub fn tools() -> Vec<Value> {
    let string = |max| json!({"type":"string","maxLength":max});
    let required_string = |max| json!({"type":"string","minLength":1,"maxLength":max});
    let object = |properties: Value, required: Vec<&str>| json!({"type":"object","properties":properties,"required":required,"additionalProperties":false});
    let evidence = object(
        json!({
            "id":required_string(80),"kind":{"type":"string","enum":["code","gitlab","merge_request","test","review","deployment","investigation"]},
            "reference":required_string(1000),"version":string(200),"details":required_string(2000)
        }),
        vec!["id", "kind", "reference", "version", "details"],
    );
    let check = object(
        json!({
            "evidenceId":required_string(80),"state":{"type":"string","enum":["verified","unavailable","divergent","not_checked"]},
            "source":string(300),"reference":string(1000),"version":string(200),"note":required_string(2000)
        }),
        vec![
            "evidenceId",
            "state",
            "source",
            "reference",
            "version",
            "note",
        ],
    );
    let definition = |name, description, schema, read| json!({"name":name,"description":description,"inputSchema":schema,"annotations":{"readOnlyHint":read,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false}});
    vec![
        definition("register_work_context", "Registre o checkout absoluto da SUA atribuição antes de trabalhar. Captura Git local somente leitura (SHA, branch, dirty e impressões de diffs). A captura ocorre no registro, não retroativamente no início da atribuição. Explique em note se já havia trabalho ou se o checkout é compartilhado. Não prova autoria, testes ou envio remoto. Até quatro checkouts; requestId permite repetição segura. Retorna id do contexto.", object(json!({"assignmentId":required_string(120),"requestId":required_string(120),"checkoutPath":required_string(2000),"note":required_string(1500)}),vec!["assignmentId","requestId","checkoutPath","note"]),false),
        definition("submit_delivery", "Registre a entrega da SUA atribuição antes de finalizar. Versões anteriores são preservadas. Consulte get_deliveries para expectedRevision (0 na primeira). contextIds são IDs de register_work_context; coleta Git final automaticamente. evidence contém referências DECLARADAS: MR, teste, código, revisão, deploy ou investigação; version identifica o SHA/versão testada (vazio se desconhecido), details explica cenário, comando, ambiente, esperado, observado e limitações. Investigações sem código são válidas. Ausência de checkout inicial não prova ausência de alteração. Não conclui a tarefa nem aprova a entrega.", object(json!({"assignmentId":required_string(120),"requestId":required_string(120),"expectedRevision":{"type":"integer","minimum":0},"kind":{"type":"string","enum":["code_change","investigation","validation","documentation"]},"summary":required_string(4000),"limitations":string(2000),"contextIds":{"type":"array","maxItems":4,"uniqueItems":true,"items":required_string(120)},"evidence":{"type":"array","maxItems":12,"items":evidence}}),vec!["assignmentId","requestId","expectedRevision","kind","summary","limitations","contextIds","evidence"]),false),
        definition("get_deliveries", "Consulta entregas de qualquer pessoa da sala. Por taskCode retorna atribuições e estado de entrega/conferência (paginado). Com terminalId e assignmentId retorna contexto Git, entrega e revisão do gerente; revision seleciona uma versão anterior e reviewRevision uma conferência anterior. Sem entrega significa não registrada, nunca ausência de trabalho. Git local não confirma autoria ou GitLab; conferências externas são registradas pelo gerente com fonte, referência e versão.", object(json!({"taskCode":required_string(80),"terminalId":required_string(120),"assignmentId":required_string(120),"revision":{"type":"integer","minimum":1},"reviewRevision":{"type":"integer","minimum":1},"offset":{"type":"integer","minimum":0},"limit":{"type":"integer","minimum":1,"maximum":20}}),vec![]),true),
        definition("review_delivery", "Somente gerente treinado: registra a conferência da versão atual da entrega de outra pessoa após consultar referências com ferramentas disponíveis. Não verifica GitLab automaticamente. Declare um estado por evidência: verified exige fonte, referência e versão correspondente; indisponibilidade de acesso é unavailable, não ausência de alteração. Confere apenas deliveryRevision; expectedReviewRevision protege concorrência. Accepted é aceite do gerente, não prova automática de todas as evidências. Preserva histórico e não altera tarefa, Jira ou fila. Consulte entregas antes de atualizar resumo/status; não infira autoria de pasta compartilhada.",object(json!({"terminalId":required_string(120),"assignmentId":required_string(120),"deliveryRevision":{"type":"integer","minimum":1},"expectedReviewRevision":{"type":"integer","minimum":0},"requestId":required_string(120),"decision":{"type":"string","enum":["accepted","changes_requested","inconclusive"]},"summary":required_string(4000),"checks":{"type":"array","maxItems":12,"items":check}}),vec!["terminalId","assignmentId","deliveryRevision","expectedReviewRevision","requestId","decision","summary","checks"]),false),
    ]
}

pub const GUIDANCE: &str = r#"## Entregas verificáveis

- Antes de alterar código, use register_work_context com checkout absoluto; informe
  alterações preexistentes, uso compartilhado ou registro tardio em note. Git observa
  o checkout no registro/entrega; não prova autoria nem reconstrói o início do trabalho.
- Antes de finalizar, use submit_delivery: tipo, resumo, limitações, contextIds e
  evidence. expectedRevision começa em 0. code_change exige checkout registrado;
  investigação sem alteração é válida. Evidência: referência, versão (vazia se
  desconhecida) e, em testes, cenário/comando, ambiente, esperado e observado.
- Gerente: leia get_deliveries e confira fontes antes de atualizar resumo/Jira.
  review_delivery registra um check por evidência: verified, unavailable, divergent
  ou not_checked. verified exige fonte, referência e versão correspondentes;
  relato sozinho não basta. GitLab inacessível é unavailable. A tool não verifica fontes.
- Nova entrega exige nova conferência. Execução encerrada, teste, aceite, publicação
  e deploy são distintos. Entrega/revisão não alteram Jira, status ou fila;
  release_task encerra a participação individual, mesmo com step compartilhado aberto.
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    struct Fixture {
        state: AppState,
        path: PathBuf,
        room: String,
        manager: String,
        worker: String,
        repo: PathBuf,
    }
    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!("kanasha-delivery-{}", new_id()));
            fs::create_dir_all(&root).unwrap();
            let path = root.join("state.json");
            let state = AppState::load(path.clone()).unwrap();
            let snap = state
                .create_area(CreateAreaInput {
                    workspace_kind: crate::models::WorkspaceKind::Team,
                    name: "Entregas".into(),
                    root_path: root.to_string_lossy().into(),
                })
                .unwrap();
            let room = snap.areas[0].workspaces[0].id.clone();
            let manager = snap.areas[0].workspaces[0].panels[0].id.clone();
            let snap = state
                .hire_agent(HireAgentInput {
                    workspace_id: room.clone(),
                    name: "Davi".into(),
                    specialty: "DEV".into(),
                    instructions: "Implementar".into(),
                    execution: None,
                })
                .unwrap();
            let worker = snap.areas[0].workspaces[0].panels[1].id.clone();
            {
                let mut p = guard(&state.persisted).unwrap();
                for panel in &mut find_workspace_mut(&mut p, &room).unwrap().panels {
                    panel.member.as_mut().unwrap().training = TrainingState::Ready;
                }
            }
            state
                .enqueue_assignment(
                    AssignAgentInput {
                        terminal_id: worker.clone(),
                        request_id: "work".into(),
                        message: "Investigar".into(),
                        task_code: Some("B2C-4423".into()),
                        step_ids: vec![],
                        fresh_context: None,
                        training: false,
                    },
                    Some((&room, &manager)),
                )
                .unwrap();
            {
                let mut p = guard(&state.persisted).unwrap();
                assignment_mut(&mut p, &room, &worker, "work")
                    .unwrap()
                    .status = AssignmentStatus::Done;
                state.save(&p).unwrap();
            }
            let repo = root.join("checkout");
            fs::create_dir(&repo).unwrap();
            let f = Self {
                state,
                path,
                room,
                manager,
                worker,
                repo,
            };
            f.git(&["init", "-b", "main"]);
            fs::write(f.repo.join("code.txt"), "original\n").unwrap();
            f.git(&["add", "code.txt"]);
            f.git(&["commit", "-m", "B2C-4233: alteração preexistente"]);
            f
        }
        fn git(&self, args: &[&str]) -> String {
            let out = Command::new("git")
                .arg("-C")
                .arg(&self.repo)
                .args([
                    "-c",
                    "user.name=Fixture",
                    "-c",
                    "user.email=fixture@example.invalid",
                ])
                .args(args)
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
            String::from_utf8(out.stdout).unwrap().trim().into()
        }
        fn context(&self) -> RegisterInput {
            RegisterInput {
                assignment_id: "work".into(),
                request_id: "context-1".into(),
                checkout_path: self.repo.to_string_lossy().into(),
                note: "Compartilhado; registro feito após parte da investigação.".into(),
            }
        }
        fn submission(&self) -> SubmitInput {
            SubmitInput {
                assignment_id: "work".into(),
                request_id: "delivery-1".into(),
                expected_revision: 0,
                kind: DeliveryKind::Investigation,
                summary: "MR encontrada não comprova autoria ou correção do timeout.".into(),
                limitations: "Acesso ao GitLab expirou.".into(),
                context_ids: vec![],
                evidence: vec![Evidence {
                    id: "mr".into(),
                    kind: EvidenceKind::MergeRequest,
                    reference: "https://example.invalid/project/merge_requests/96".into(),
                    version: "old-sha".into(),
                    details: "Referência relatada; autoria ainda não confirmada.".into(),
                }],
            }
        }
        fn query(&self) -> DeliveryQuery {
            DeliveryQuery {
                task_code: Some("B2C-4423".into()),
                terminal_id: Some(self.worker.clone()),
                assignment_id: Some("work".into()),
                revision: None,
                review_revision: None,
                offset: 0,
                limit: None,
            }
        }
        fn review(&self) -> ReviewInput {
            ReviewInput {
                terminal_id: self.worker.clone(),
                assignment_id: "work".into(),
                delivery_revision: 1,
                expected_review_revision: 0,
                request_id: "review-1".into(),
                decision: Decision::Inconclusive,
                summary: "Não foi possível conferir a MR.".into(),
                checks: vec![EvidenceCheck {
                    evidence_id: "mr".into(),
                    state: CheckState::Unavailable,
                    source: "GitLab MCP".into(),
                    reference: "https://example.invalid/project/merge_requests/96".into(),
                    version: "".into(),
                    note: "Autenticação expirada; não é ausência de mudança.".into(),
                }],
            }
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(self.path.parent().unwrap());
        }
    }

    #[test]
    fn checkout_baseline_and_final_capture_preserve_preexisting_changes_and_receipts() {
        let f = Fixture::new();
        let old_sha = f.git(&["rev-parse", "HEAD"]);
        fs::write(f.repo.join("code.txt"), "alteração já existente\n").unwrap();
        fs::write(f.repo.join("untracked.txt"), "preexistente").unwrap();
        let index_before = fs::read(f.repo.join(".git/index")).unwrap();
        let context = f
            .state
            .register_work_context(&f.room, &f.worker, f.context())
            .unwrap();
        assert_eq!(context["baseline"]["head"], old_sha);
        assert!(context["baseline"]["status"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s.as_str().unwrap().contains("code.txt")));
        assert_eq!(fs::read(f.repo.join(".git/index")).unwrap(), index_before);
        fs::write(f.repo.join("code.txt"), "alteração na entrega\n").unwrap();
        f.git(&["add", "code.txt"]);
        f.git(&["commit", "-m", "B2C-4423: observação posterior"]);
        let sha = f.git(&["rev-parse", "HEAD"]);
        assert_eq!(
            f.state
                .register_work_context(&f.room, &f.worker, f.context())
                .unwrap(),
            context
        );
        let mut input = f.submission();
        input.kind = DeliveryKind::CodeChange;
        input.context_ids = vec![context["id"].as_str().unwrap().into()];
        f.state
            .submit_delivery(&f.room, &f.worker, input.clone())
            .unwrap();
        fs::write(f.repo.join("after.txt"), "mudança posterior").unwrap();
        f.state.submit_delivery(&f.room, &f.worker, input).unwrap();
        let record = f
            .state
            .get_deliveries(&f.room, &f.manager, f.query())
            .unwrap();
        assert_eq!(record["versions"].as_array().unwrap().len(), 1);
        let observation = &record["delivery"]["repositories"][0];
        assert_eq!(observation["finalSnapshot"]["head"], sha);
        assert_eq!(observation["comparison"]["commits"], json!([sha]));
        assert_eq!(
            observation["comparison"]["committedFiles"],
            json!(["code.txt"])
        );
        assert!(observation["finalSnapshot"]["status"]
            .as_array()
            .unwrap()
            .iter()
            .all(|s| !s.as_str().unwrap().contains("after.txt")));
        assert_eq!(record["delivery"]["submittedBy"], f.worker);
        assert_eq!(record["delivery"]["reviews"], json!([]));
    }

    #[test]
    fn reviews_are_scoped_versioned_and_do_not_change_task_execution_or_queue() {
        let f = Fixture::new();
        let before = f.state.snapshot().unwrap();
        f.state
            .submit_delivery(&f.room, &f.worker, f.submission())
            .unwrap();
        assert!(f
            .state
            .review_delivery(&f.room, &f.worker, f.review())
            .is_err());
        assert!(f
            .state
            .review_delivery("other-room", &f.manager, f.review())
            .is_err());
        f.state
            .review_delivery(&f.room, &f.manager, f.review())
            .unwrap();
        f.state
            .review_delivery(&f.room, &f.manager, f.review())
            .unwrap();
        let record = f
            .state
            .get_deliveries(&f.room, &f.manager, f.query())
            .unwrap();
        assert_eq!(record["delivery"]["reviews"][0]["reviewedBy"], f.manager);
        assert_eq!(
            record["delivery"]["reviews"][0]["assessment"]["checks"][0]["state"],
            "unavailable"
        );
        let mut next = f.submission();
        next.expected_revision = 1;
        next.request_id = "delivery-2".into();
        next.summary = "Entrega corrigida".into();
        f.state
            .submit_delivery(&f.room, &f.worker, next.clone())
            .unwrap();
        assert!(f
            .state
            .submit_delivery(
                &f.room,
                &f.worker,
                SubmitInput {
                    request_id: "stale".into(),
                    ..next
                }
            )
            .is_err());
        let latest = f
            .state
            .get_deliveries(&f.room, &f.manager, f.query())
            .unwrap();
        assert!(latest["assignment"]["reviewDecision"].is_null());
        let mut review = f.review();
        review.request_id = "stale-review".into();
        assert!(f
            .state
            .review_delivery(&f.room, &f.manager, review)
            .is_err());
        let mut query = f.query();
        query.revision = Some(1);
        query.review_revision = Some(1);
        assert_eq!(
            f.state.get_deliveries(&f.room, &f.manager, query).unwrap()["delivery"]["reviews"][0]
                ["assessment"]["decision"],
            "inconclusive"
        );
        let restored = AppState::load(f.path.clone()).unwrap();
        let after = restored.snapshot().unwrap();
        assert_eq!(
            before.areas[0].workspaces[0].tasks,
            after.areas[0].workspaces[0].tasks
        );
        let old = before.areas[0].workspaces[0]
            .panel(&f.worker)
            .unwrap()
            .member
            .as_ref()
            .unwrap();
        let new = after.areas[0].workspaces[0]
            .panel(&f.worker)
            .unwrap()
            .member
            .as_ref()
            .unwrap();
        assert_eq!(old.task_queue, new.task_queue);
        assert_eq!(old.jobs[0].status, new.jobs[0].status);
        assert_eq!(new.jobs[0].deliveries.len(), 2);
        assert_eq!(
            restored
                .get_deliveries(&f.room, &f.manager, f.query())
                .unwrap(),
            latest
        );
    }

    #[test]
    fn foreign_assignments_spoofed_identity_stale_inputs_and_unsupported_verification_are_rejected()
    {
        let f = Fixture::new();
        assert!(f
            .state
            .submit_delivery(&f.room, &f.manager, f.submission())
            .is_err());
        assert!(f
            .state
            .register_work_context(&f.room, &f.manager, f.context())
            .is_err());
        assert!(f
            .state
            .get_deliveries(&f.room, "outsider", f.query())
            .is_err());
        let mut spoof = json!(f.submission());
        spoof["terminalId"] = json!(f.manager);
        assert!(serde_json::from_value::<SubmitInput>(spoof).is_err());
        f.state
            .submit_delivery(&f.room, &f.worker, f.submission())
            .unwrap();
        let mut changed = f.submission();
        changed.summary = "outro conteúdo".into();
        assert!(f
            .state
            .submit_delivery(&f.room, &f.worker, changed)
            .is_err());
        let mut review = f.review();
        review.checks[0].state = CheckState::Verified;
        assert!(f
            .state
            .review_delivery(&f.room, &f.manager, review.clone())
            .is_err());
        review.checks[0].version = "different-sha".into();
        assert!(f
            .state
            .review_delivery(&f.room, &f.manager, review.clone())
            .is_err());
        review.checks[0].version = "old-sha".into();
        review.decision = Decision::Accepted;
        f.state
            .review_delivery(&f.room, &f.manager, review.clone())
            .unwrap();
        let mut stale = review.clone();
        stale.request_id = "new-review".into();
        assert!(f.state.review_delivery(&f.room, &f.manager, stale).is_err());
        review.summary = "alteração com o mesmo recibo".into();
        assert!(f
            .state
            .review_delivery(&f.room, &f.manager, review)
            .is_err());
    }

    #[test]
    fn investigation_without_checkout_and_legacy_history_remain_queryable() {
        let f = Fixture::new();
        let legacy = f
            .state
            .get_deliveries(&f.room, &f.manager, f.query())
            .unwrap();
        assert!(legacy["delivery"].is_null());
        let mut code = f.submission();
        code.kind = DeliveryKind::CodeChange;
        assert!(f.state.submit_delivery(&f.room, &f.worker, code).is_err());
        f.state
            .submit_delivery(&f.room, &f.worker, f.submission())
            .unwrap();
        let mut query = f.query();
        query.terminal_id = None;
        query.assignment_id = None;
        query.limit = Some(1);
        let list = f.state.get_deliveries(&f.room, &f.manager, query).unwrap();
        assert_eq!(list["total"], 1);
        assert_eq!(list["items"][0]["assignment"]["deliveryRevision"], 1);
        let mut query = f.query();
        query.task_code = Some("B2C-OTHER".into());
        assert!(f.state.get_deliveries(&f.room, &f.manager, query).is_err());
        let mut serialized = json!(assignment(
            find_workspace(&guard(&f.state.persisted).unwrap(), &f.room).unwrap(),
            &f.worker,
            "work"
        )
        .unwrap());
        serialized.as_object_mut().unwrap().remove("deliveries");
        serialized.as_object_mut().unwrap().remove("workContexts");
        let legacy: Assignment = serde_json::from_value(serialized).unwrap();
        assert!(legacy.deliveries.is_empty());
        assert!(legacy.work_contexts.is_empty());
    }

    #[test]
    fn failed_save_preserves_previous_delivery_and_review() {
        let f = Fixture::new();
        f.state
            .submit_delivery(&f.room, &f.worker, f.submission())
            .unwrap();
        let before = f.state.snapshot().unwrap();
        let backup = f.path.with_extension("backup");
        fs::rename(&f.path, &backup).unwrap();
        fs::create_dir(&f.path).unwrap();
        assert!(f
            .state
            .review_delivery(&f.room, &f.manager, f.review())
            .is_err());
        let mut next = f.submission();
        next.expected_revision = 1;
        next.request_id = "next".into();
        assert!(f.state.submit_delivery(&f.room, &f.worker, next).is_err());
        assert!(f
            .state
            .register_work_context(&f.room, &f.worker, f.context())
            .is_err());
        assert_eq!(f.state.snapshot().unwrap().areas, before.areas);
        fs::remove_dir(&f.path).unwrap();
        fs::rename(backup, &f.path).unwrap();
    }

    #[test]
    fn git_failure_and_divergent_history_are_explicit_and_do_not_claim_no_changes() {
        let f = Fixture::new();
        let first = delivery_git::capture(f.repo.to_str().unwrap());
        let missing = delivery_git::capture(f.repo.join("missing").to_str().unwrap());
        assert!(missing.head.is_none());
        assert!(!missing.limitations.is_empty());
        f.git(&["checkout", "--orphan", "unrelated"]);
        fs::write(f.repo.join("code.txt"), "outra história").unwrap();
        f.git(&["add", "code.txt"]);
        f.git(&["commit", "-m", "Outra origem"]);
        let after = delivery_git::capture(f.repo.to_str().unwrap());
        let comparison = delivery_git::compare(&first, &after);
        assert!(comparison.commits.is_empty());
        assert!(comparison
            .limitations
            .iter()
            .any(|s| s.contains("ancestral")));
        let mut context = f.context();
        context.checkout_path = f.repo.join("missing").to_string_lossy().into();
        assert!(f
            .state
            .register_work_context(&f.room, &f.worker, context)
            .unwrap()["baseline"]["head"]
            .is_null());
    }

    #[test]
    fn multiple_worktrees_have_distinct_checkouts_and_each_own_baseline() {
        let f = Fixture::new();
        let context = f
            .state
            .register_work_context(&f.room, &f.worker, f.context())
            .unwrap();
        let worktree = f.repo.parent().unwrap().join("checkout with spaces");
        f.git(&[
            "worktree",
            "add",
            "-b",
            "isolated",
            worktree.to_str().unwrap(),
        ]);
        let mut registration = f.context();
        registration.request_id = "second-checkout".into();
        registration.checkout_path = worktree.to_string_lossy().into();
        let second = f
            .state
            .register_work_context(&f.room, &f.worker, registration)
            .unwrap();
        assert_ne!(
            context["baseline"]["repositoryRoot"],
            second["baseline"]["repositoryRoot"]
        );
        assert_eq!(
            context["baseline"]["gitCommonDir"],
            second["baseline"]["gitCommonDir"]
        );
        assert_eq!(second["baseline"]["branch"], "isolated");
        fs::write(worktree.join("code.txt"), "alteração só no worktree").unwrap();
        let mut input = f.submission();
        input.context_ids = vec![
            context["id"].as_str().unwrap().into(),
            second["id"].as_str().unwrap().into(),
        ];
        f.state.submit_delivery(&f.room, &f.worker, input).unwrap();
        let record = f
            .state
            .get_deliveries(&f.room, &f.manager, f.query())
            .unwrap();
        let repos = record["delivery"]["repositories"].as_array().unwrap();
        assert_eq!(repos.len(), 2);
        assert!(repos[0]["finalSnapshot"]["status"]
            .as_array()
            .unwrap()
            .is_empty());
        assert!(!repos[1]["finalSnapshot"]["status"]
            .as_array()
            .unwrap()
            .is_empty());
        let expired =
            delivery_git::capture_until(f.repo.to_str().unwrap(), std::time::Instant::now());
        assert!(expired.repository_root.is_none());
        assert!(!expired.limitations.is_empty());
    }

    #[test]
    fn history_query_selects_specific_review_and_new_checks_need_all_evidence() {
        let f = Fixture::new();
        f.state
            .submit_delivery(&f.room, &f.worker, f.submission())
            .unwrap();
        let mut input = f.review();
        input.checks.clear();
        assert!(f.state.review_delivery(&f.room, &f.manager, input).is_err());
        f.state
            .review_delivery(&f.room, &f.manager, f.review())
            .unwrap();
        let mut second = f.review();
        second.request_id = "review-2".into();
        second.expected_review_revision = 1;
        second.checks[0].state = CheckState::Divergent;
        f.state
            .review_delivery(&f.room, &f.manager, second)
            .unwrap();
        let mut query = f.query();
        query.review_revision = Some(1);
        assert_eq!(
            f.state.get_deliveries(&f.room, &f.manager, query).unwrap()["delivery"]["reviews"][0]
                ["assessment"]["checks"][0]["state"],
            "unavailable"
        );
        assert_eq!(
            f.state
                .get_deliveries(&f.room, &f.manager, f.query())
                .unwrap()["delivery"]["reviews"][0]["revision"],
            2
        );
    }
}
