use crate::{
    models::{TaskContext, TaskPatch},
    AppState,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    env,
    io::{self, BufRead, BufReader, Read, Write},
    thread,
};
use tauri::{AppHandle, Emitter};

// Memory responses include both the proposed and approved soul, plus the initial profile.
const MAX_IPC_MESSAGE: u64 = 256 * 1024;

const SUMMARY_GUIDANCE: &str = "Markdown claro, com títulos e listas: Visão geral (objetivo/motivo); Tarefas envolvidas (subtítulo por ID: problema, impacto, esperado); O que foi implementado (mudanças por tarefa); Testes e resultados (cenário, ambiente, resultado, versão, evidência); Pendências e próximos passos (motivo e ação); Links de acompanhamento (Jira, Raposa/MRs, GMUDs reais e estado conhecido). Use [nome](URL), explique siglas, diferencie solicitado/entregue/validado/publicado. Preserve fatos confirmados; marque lacunas e informação antiga não revalidada. Link não informado não significa não criado. Não copie conversas/steps nem invente resultados. Até 8000 caracteres; use só o necessário.";

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct IpcRequest {
    token: String,
    workspace_id: String,
    terminal_id: String,
    patch: Option<TaskPatch>,
    tool: Option<String>,
    arguments: Option<Value>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct IpcResponse {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    context: Option<TaskContext>,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[cfg(unix)]
pub fn start_listener(state: AppState, app: AppHandle) -> Result<(), String> {
    use std::{
        fs,
        os::unix::{fs::PermissionsExt, net::UnixListener},
    };

    if let Some(directory) = state.mcp_socket_path.parent() {
        fs::create_dir_all(directory)
            .map_err(|error| format!("Não foi possível preparar o MCP local: {error}"))?;
    }
    if state.mcp_socket_path.exists() {
        fs::remove_file(&state.mcp_socket_path)
            .map_err(|error| format!("Não foi possível renovar o MCP local: {error}"))?;
    }
    let listener = UnixListener::bind(&state.mcp_socket_path)
        .map_err(|error| format!("Não foi possível iniciar o MCP local: {error}"))?;
    fs::set_permissions(&state.mcp_socket_path, fs::Permissions::from_mode(0o600))
        .map_err(|error| format!("Não foi possível proteger o MCP local: {error}"))?;

    thread::Builder::new()
        .name("kanasha-mcp-ipc".into())
        .spawn(move || {
            for connection in listener.incoming() {
                match connection {
                    Ok(stream) => {
                        let state = state.clone();
                        let app = app.clone();
                        thread::spawn(move || handle_ipc(stream, &state, &app));
                    }
                    Err(error) => eprintln!("Kanasha MCP IPC: {error}"),
                }
            }
        })
        .map_err(|error| format!("Não foi possível executar o MCP local: {error}"))?;
    Ok(())
}

#[cfg(not(unix))]
pub fn start_listener(_state: AppState, _app: AppHandle) -> Result<(), String> {
    Ok(())
}

#[cfg(unix)]
fn handle_ipc(mut stream: std::os::unix::net::UnixStream, state: &AppState, app: &AppHandle) {
    let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(45)));
    let _ = stream.set_write_timeout(Some(std::time::Duration::from_secs(45)));
    let response = (|| -> Result<IpcResponse, String> {
        let mut line = String::new();
        BufReader::new(&mut stream)
            .take(MAX_IPC_MESSAGE + 1)
            .read_line(&mut line)
            .map_err(|error| format!("Não foi possível ler a solicitação MCP: {error}"))?;
        if line.len() as u64 > MAX_IPC_MESSAGE {
            return Err("A solicitação MCP excede o limite permitido.".into());
        }
        let request: IpcRequest = serde_json::from_str(&line)
            .map_err(|error| format!("A solicitação MCP é inválida: {error}"))?;
        if !state.valid_mcp_token(&request.terminal_id, &request.token)? {
            return Err("A solicitação MCP não foi autorizada.".into());
        }
        if let Some(tool) = request.tool.as_deref() {
            let arguments = request.arguments.unwrap_or_else(|| json!({}));
            let data = match tool {
                "get_tasks" if arguments.as_object().is_some_and(|o| o.is_empty()) => {
                    state.issue_tasks_context(&request.workspace_id, &request.terminal_id)?
                }
                "get_task" | "get_task_steps" => {
                    #[derive(Deserialize)]
                    #[serde(rename_all = "camelCase", deny_unknown_fields)]
                    struct Input { task_code: String }
                    let input: Input = serde_json::from_value(arguments).map_err(|e| e.to_string())?;
                    state.issue_task_context(&request.workspace_id, &request.terminal_id, &input.task_code, tool == "get_task_steps")?
                }
                "set_task" => {
                    let (code, patch) = scoped_task_patch(arguments)?;
                    let context = state.task_context_for(&request.workspace_id, &request.terminal_id, Some(&code), Some(patch))?;
                    let _ = app.emit("task-state-changed", &request.workspace_id);
                    let ids: Vec<String> = {
                        let p = crate::guard(&state.persisted)?;
                        crate::find_workspace(&p, &request.workspace_id)?.panels.iter().filter(|panel| panel.member.is_some()).map(|p| p.id.clone()).collect()
                    };
                    for id in ids { state.wake_member(app, &id); }
                    json!({"ok":true,"taskCode":code,"revision":context.task.revision})
                }
                "get_team" if arguments.as_object().is_some_and(|o| o.is_empty()) => {
                    state.team_context(&request.workspace_id, &request.terminal_id, None)?
                }
                "get_agent" => {
                    #[derive(Deserialize)]
                    #[serde(rename_all = "camelCase", deny_unknown_fields)]
                    struct Target {
                        terminal_id: String,
                    }
                    let target: Target =
                        serde_json::from_value(arguments).map_err(|e| e.to_string())?;
                    state.team_context(
                        &request.workspace_id,
                        &request.terminal_id,
                        Some(&target.terminal_id),
                    )?
                }
                "set_assignment" => {
                    let input = serde_json::from_value(arguments).map_err(|e| e.to_string())?;
                    let id = state.assign_agent(
                        app,
                        input,
                        Some((&request.workspace_id, &request.terminal_id)),
                    )?;
                    json!({"id":id,"accepted":true})
                }
                "reset_context" => {
                    let input = serde_json::from_value(arguments).map_err(|e| e.to_string())?;
                    state.reset_agent_context(app, input, Some((&request.workspace_id, &request.terminal_id)))?;
                    json!({"accepted":true,"preservesMemory":true})
                }
                "release_task" => {
                    let input: crate::office::ReleaseTaskInput = serde_json::from_value(arguments).map_err(|e| e.to_string())?;
                    let id = input.terminal_id.clone();
                    state.release_task(&request.workspace_id, &request.terminal_id, input)?;
                    let _ = app.emit("task-state-changed", &request.workspace_id);
                    state.wake_member(app, &id);
                    json!({"accepted":true,"sharedTaskUnchanged":true})
                }
                "register_work_context" | "submit_delivery" | "get_deliveries" | "review_delivery" => {
                    let data = match tool {
                        "register_work_context" => state.register_work_context(&request.workspace_id, &request.terminal_id, serde_json::from_value(arguments).map_err(|e| e.to_string())?)?,
                        "submit_delivery" => state.submit_delivery(&request.workspace_id, &request.terminal_id, serde_json::from_value(arguments).map_err(|e| e.to_string())?)?,
                        "review_delivery" => state.review_delivery(&request.workspace_id, &request.terminal_id, serde_json::from_value(arguments).map_err(|e| e.to_string())?)?,
                        _ => state.get_deliveries(&request.workspace_id, &request.terminal_id, serde_json::from_value(arguments).map_err(|e| e.to_string())?)?,
                    };
                    if tool != "get_deliveries" { let _ = app.emit("task-state-changed", &request.workspace_id); }
                    data
                }
                "get_memory" | "get_soul" => {
                    #[derive(Deserialize)]
                    #[serde(rename_all = "camelCase", deny_unknown_fields)]
                    struct Input {
                        terminal_id: Option<String>,
                    }
                    let input: Input =
                        serde_json::from_value(arguments).map_err(|e| e.to_string())?;
                    let context = state.memory_context(
                        &request.workspace_id,
                        &request.terminal_id,
                        input.terminal_id.as_deref(),
                    )?;
                    if tool == "get_soul" {
                        json!({"soul":context["soul"],"name":context["name"],"specialty":context["specialty"],"training":context["training"]})
                    } else { context }
                }
                "set_professional_profile" => {
                    let input = serde_json::from_value(arguments).map_err(|e| e.to_string())?;
                    let profile = state.set_professional_profile(
                        &request.workspace_id,
                        &request.terminal_id,
                        input,
                    )?;
                    let _ = app.emit("task-state-changed", &request.terminal_id);
                    json!({"ok":true,"name":profile.name,"generatedAt":profile.generated_at})
                }
                "set_memory" => {
                    #[derive(Deserialize)]
                    #[serde(rename_all = "camelCase", deny_unknown_fields)]
                    struct Input {
                        content: String,
                        expected_revision: u64,
                    }
                    let input: Input =
                        serde_json::from_value(arguments).map_err(|e| e.to_string())?;
                    let memory = state.update_memory(
                        &request.terminal_id,
                        input.content,
                        input.expected_revision,
                        Some((&request.workspace_id, &request.terminal_id)),
                    )?;
                    let _ = app.emit("task-state-changed", &request.terminal_id);
                    json!({"ok":true,"revision":memory.revision})
                }
                "set_soul" => {
                    #[derive(Deserialize)]
                    #[serde(rename_all = "camelCase", deny_unknown_fields)]
                    struct Input { content: String, expected_revision: u64 }
                    let input: Input = serde_json::from_value(arguments).map_err(|e| e.to_string())?;
                    let soul = state.update_soul(&request.terminal_id, input.content, input.expected_revision,
                        Some((&request.workspace_id, &request.terminal_id)))?;
                    let _ = app.emit("task-state-changed", &request.terminal_id);
                    json!({"soul":soul})
                }
                "set_training_complete" => {
                    #[derive(Deserialize)]
                    #[serde(rename_all = "camelCase", deny_unknown_fields)]
                    struct Input {
                        terminal_id: String,
                        expected_revision: u64,
                        expected_soul_revision: u64,
                    }
                    let input: Input =
                        serde_json::from_value(arguments).map_err(|e| e.to_string())?;
                    state.approve_training(
                        &input.terminal_id,
                        input.expected_revision,
                        input.expected_soul_revision,
                        Some((&request.workspace_id, &request.terminal_id)),
                    )?;
                    let _ = app.emit("task-state-changed", &request.terminal_id);
                    json!({"ok":true})
                }
                _ => return Err("Ferramenta ou parâmetros inválidos.".into()),
            };
            return Ok(IpcResponse {
                ok: true,
                context: None,
                data: Some(data),
                error: None,
            });
        }
        let changed = request
            .patch
            .as_ref()
            .is_some_and(|patch| !patch.is_empty());
        let context =
            state.task_context(&request.workspace_id, &request.terminal_id, request.patch)?;
        if changed {
            let _ = app.emit("task-state-changed", &request.workspace_id);
        }
        Ok(IpcResponse {
            ok: true,
            context: Some(context),
            data: None,
            error: None,
        })
    })()
    .unwrap_or_else(|error| IpcResponse {
        ok: false,
        context: None,
        data: None,
        error: Some(error),
    });

    if let Ok(mut encoded) = serde_json::to_vec(&response) {
        encoded.push(b'\n');
        let _ = stream.write_all(&encoded);
        let _ = stream.flush();
    }
}

pub fn run_stdio() {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut output = stdout.lock();
    for line in stdin.lock().lines() {
        let response = match line {
            Ok(line) if line.trim().is_empty() => continue,
            Ok(line) => match serde_json::from_str::<Value>(&line) {
                Ok(request) => handle_rpc(request),
                Err(error) => Some(rpc_error(Value::Null, -32700, &error.to_string())),
            },
            Err(error) => {
                eprintln!("Kanasha MCP: {error}");
                break;
            }
        };
        if let Some(response) = response {
            if serde_json::to_writer(&mut output, &response).is_err()
                || output.write_all(b"\n").is_err()
                || output.flush().is_err()
            {
                break;
            }
        }
    }
}

fn handle_rpc(request: Value) -> Option<Value> {
    let method = request.get("method")?.as_str()?;
    let id = request.get("id").cloned();
    if id.is_none() {
        return None;
    }
    let id = id.unwrap_or(Value::Null);
    match method {
        "initialize" => Some(rpc_result(
            id,
            json!({
                "protocolVersion": "2025-06-18",
                "capabilities": { "tools": { "listChanged": false } },
                "serverInfo": { "name": "kanasha-terminal", "version": env!("CARGO_PKG_VERSION") },
                "instructions": format!("Leia get_task ao iniciar e get_task_steps quando precisar das etapas. {SUMMARY_GUIDANCE} Use set_task para atualizar somente campos alterados; mantenha summary atualizado nas mudanças de escopo, implementação, validação e publicação. Envie agentStatus=working ao começar; atualize agentCurrent ao mudar de etapa; use waiting, blocked, done ou idle nas transições. Registre etapas em steps e atualize seus estados com stepStatus.")
            }),
        )),
        "ping" => Some(rpc_result(id, json!({}))),
        "tools/list" => Some(rpc_result(id, tools_list())),
        "tools/call" => Some(rpc_result(id, call_tool(request.get("params")))),
        _ => Some(rpc_error(id, -32601, "Método MCP não encontrado.")),
    }
}

fn tools_list() -> Value {
    let mut list = json!({
        "tools": [{
            "name": "get_task",
            "description": "Consulta o estado completo da tarefa deste Workspace: status, resumo descritivo, andamento, próximo passo, bloqueio e revisão. Não inclui etapas nem altera dados.",
            "inputSchema": { "type": "object", "properties": {"taskCode":{"type":"string","minLength":1,"maxLength":80,"description":"Código Jira; sem ele consulta a visão compartilhada da sala."}}, "additionalProperties": false },
            "annotations": {
                "readOnlyHint": true,
                "destructiveHint": false,
                "idempotentHint": true,
                "openWorldHint": false
            }
        }, {
            "name": "get_task_steps",
            "description": "Consulta a lista ordenada de etapas da tarefa, com identificador, título e status de cada etapa. Não altera dados.",
            "inputSchema": { "type": "object", "properties": {"taskCode":{"type":"string","minLength":1,"maxLength":80,"description":"Código Jira; sem ele consulta a visão compartilhada da sala."}}, "additionalProperties": false },
            "annotations": {
                "readOnlyHint": true,
                "destructiveHint": false,
                "idempotentHint": true,
                "openWorldHint": false
            }
        }, {
            "name": "set_task",
            "description": "Atualiza somente os campos enviados da tarefa ou deste agente. Requer ao menos um campo.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "taskCode": {"type":"string","minLength":1,"maxLength":80,"description":"Código Jira para registrar/atualizar a tarefa e seus próprios steps. Sem código atualiza a visão compartilhada da sala."},
                    "status": {
                        "type": "string",
                        "enum": ["not_started", "in_progress", "waiting", "blocked", "done"]
                    },
                    "summary": { "type": "string", "maxLength": 8000, "description": SUMMARY_GUIDANCE },
                    "steps": {
                        "type": "array",
                        "maxItems": 20,
                        "items": {
                            "type": "object",
                            "properties": {
                                "id": { "type": "string", "minLength": 1, "maxLength": 40, "pattern": "^[A-Za-z0-9_-]+$" },
                                "title": { "type": "string", "minLength": 1, "maxLength": 160 },
                                "status": {
                                    "type": "string",
                                    "enum": ["not_started", "in_progress", "waiting", "blocked", "done"]
                                }
                            },
                            "required": ["id", "title", "status"],
                            "additionalProperties": false
                        }
                    },
                    "stepStatus": {
                        "type": "object",
                        "properties": {
                            "id": { "type": "string", "minLength": 1, "maxLength": 40, "pattern": "^[A-Za-z0-9_-]+$" },
                            "status": {
                                "type": "string",
                                "enum": ["not_started", "in_progress", "waiting", "blocked", "done"]
                            }
                        },
                        "required": ["id", "status"],
                        "additionalProperties": false
                    },
                    "current": { "type": "string", "maxLength": 200 },
                    "next": { "type": "string", "maxLength": 200 },
                    "blocker": { "type": "string", "maxLength": 200 },
                    "agentStatus": {
                        "type": "string",
                        "enum": ["idle", "working", "waiting", "blocked", "done"]
                    },
                    "agentCurrent": { "type": "string", "maxLength": 160 }
                },
                "additionalProperties": false
            },
            "annotations": {
                "destructiveHint": false,
                "idempotentHint": false,
                "openWorldHint": false
            }
        }, {
            "name":"get_tasks", "description":"Lista tarefas Jira desta sala, status, quantidade de steps e vínculos persistidos com funcionários e atribuições. Use get_task/get_task_steps com taskCode para detalhes.",
            "inputSchema":{"type":"object","properties":{},"additionalProperties":false},
            "annotations":{"readOnlyHint":true,"destructiveHint":false,"openWorldHint":false}
        }, {
            "name":"get_team", "description":"Consulta as pessoas contratadas nesta sala, especialidades, disponibilidade e atividade atual.",
            "inputSchema":{"type":"object","properties":{},"additionalProperties":false},
            "annotations":{"readOnlyHint":true,"destructiveHint":false,"openWorldHint":false}
        }, {
            "name":"get_agent", "description":"Consulta orientações, execução atual, fila em ordem de execução e última atribuição/resultado de uma pessoa desta sala. O histórico completo fica na tela Equipe.",
            "inputSchema":{"type":"object","properties":{"terminalId":{"type":"string"}},"required":["terminalId"],"additionalProperties":false},
            "annotations":{"readOnlyHint":true,"destructiveHint":false,"openWorldHint":false}
        }, {
            "name":"set_assignment", "description":"Gerente: enfileira trabalho para membro contratado, inclusive ocupado (até 10 pendentes, uma execução por vez). Steps da tarefa atual precedem as seguintes até conclusão/liberação por release_task. Mesma taskCode preserva conversa; troca de código inicia outra. Sem código, freshContext=true pede nova conversa. Resultado automático, sem polling. requestId único por pedido; mesmo ID/conteúdo não duplica. Mensagem curta em Markdown: objetivo, escopo, referências/versão e aceite. Não contrata pessoas.",
            "inputSchema":{"type":"object","properties":{"terminalId":{"type":"string"},"requestId":{"type":"string","minLength":1,"maxLength":120},"message":{"type":"string","minLength":1,"maxLength":8000},"stepIds":{"type":"array","maxItems":20,"uniqueItems":true,"items":{"type":"string","minLength":1,"maxLength":40},"description":"IDs reais retornados por get_task_steps da taskCode; vinculam atribuição, funcionário e steps sem depender dos títulos."},"taskCode":{"type":"string","minLength":1,"maxLength":80,"description":"Código Jira fornecido pelo usuário, igual em todos os steps da tarefa. A troca de código abre conversa nova do funcionário; mesmo código mantém o contexto."},"freshContext":{"type":"boolean","description":"Troca explícita para demanda independente sem código; padrão false. Para funcionário com taskCode, a fronteira é o código: mudança de step da mesma tarefa nunca limpa."},"training":{"type":"boolean","description":"true para treinar um membro antes de liberá-lo para trabalho"}},"required":["terminalId","requestId","message"],"additionalProperties":false},
            "annotations":{"readOnlyHint":false,"destructiveHint":false,"idempotentHint":true,"openWorldHint":true}
        }, {
            "name":"release_task", "description":"Gerente treinado: encerra participação individual após conferir entrega; libera próxima tarefa sem alterar steps compartilhados/status global. assignmentId deve ser a última atribuição concluída nessa tarefa. Exige ausência de trabalho próprio pendente, turno, pergunta ou resultado incerto; não retoma pausados. Correções futuras entram no fim da fila sem interromper trabalho atual. Preserva vínculos, alma, memória, arquivos e histórico.",
            "inputSchema":{"type":"object","properties":{"terminalId":{"type":"string"},"taskCode":{"type":"string","minLength":1,"maxLength":80},"assignmentId":{"type":"string","minLength":1,"maxLength":120}},"required":["terminalId","taskCode","assignmentId"],"additionalProperties":false},
            "annotations":{"readOnlyHint":false,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false}
        }, {
            "name":"reset_context", "description":"Gerente treinado: nova conversa de outro membro ocioso, sem pergunta/execução incerta; não retoma pausados. Preserva alma, memória, arquivos, histórico e fila. Troca de taskCode já faz isso; não use entre steps. Após resposta incerta, consulte get_agent e repita o mesmo requestId.",
            "inputSchema":{"type":"object","properties":{"terminalId":{"type":"string"},"requestId":{"type":"string","minLength":1,"maxLength":120}},"required":["terminalId","requestId"],"additionalProperties":false},
            "annotations":{"readOnlyHint":false,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false}
        }, {
            "name":"get_memory", "description":"Consulta memória, alma (proposta e versão aprovada) e estado de treinamento. O gerente pode informar terminalId de um membro da mesma sala para revisar o aprendizado.",
            "inputSchema":{"type":"object","properties":{"terminalId":{"type":"string"}},"additionalProperties":false},
            "annotations":{"readOnlyHint":true,"destructiveHint":false,"openWorldHint":false}
        }, {
            "name":"set_professional_profile", "description":"Cria seu perfil uma única vez. Consulte get_memory/get_team; preserve o nome atual, inclusive automático. Descreva abordagem da função em Markdown curto, sem inventar experiência. Não renomeia nem aprova treinamento. Mesmo conteúdo pode ser repetido.",
            "inputSchema":{"type":"object","properties":{"name":{"type":"string","minLength":1,"maxLength":120,"description":"Nome atual retornado por get_memory. Deve ser preservado; somente o usuário pode alterá-lo pela interface."},"personality":{"type":"string","minLength":1,"maxLength":800},"content":{"type":"string","minLength":1,"maxLength":6000,"description":"Markdown: missão, competências, forma de trabalhar, comunicação, qualidade, limites e desenvolvimento no treinamento."}},"required":["name","personality","content"],"additionalProperties":false},
            "annotations":{"readOnlyHint":false,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false}
        }, {
            "name":"get_soul", "description":"Consulta a alma: proposta em treinamento e identidade permanente aprovada. O gerente pode consultar um membro da mesma sala. Não confunda com memória ou perfil inicial.",
            "inputSchema":{"type":"object","properties":{"terminalId":{"type":"string"}},"additionalProperties":false},
            "annotations":{"readOnlyHint":true,"destructiveHint":false,"openWorldHint":false}
        }, {
            "name":"set_soul", "description":"Propõe sua alma só em treinamento inicial/complementar explícito. Markdown curto: Missão, Atuação e critérios, Limites, Origem e dúvidas. Prefira 2000 caracteres; registre apenas princípios ensinados. Use soul.revision de get_soul/get_memory. Não aprova, renomeia nem substitui a alma aprovada; demandas normais não a alteram.",
            "inputSchema":{"type":"object","properties":{"content":{"type":"string","minLength":1,"maxLength":12000},"expectedRevision":{"type":"integer","minimum":0}},"required":["content","expectedRevision"],"additionalProperties":false},
            "annotations":{"readOnlyHint":false,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false}
        }, {
            "name":"set_memory", "description":"Substitui sua memória inteira usando memory.revision de get_memory; preserve lembranças válidas. Markdown: lista datada (AAAA-MM-DD), aprendizado, origem, resultado/pendência e link real para detalhes. Prefira índice até 4000 caracteres; leia referências sob demanda. Salve só aprendizado útil, sem copiar alma/AGENTS ou registrar segredos. Exige perfil; não aprova treinamento nem escreve por outra pessoa.",
            "inputSchema":{"type":"object","properties":{"content":{"type":"string","minLength":1,"maxLength":16000},"expectedRevision":{"type":"integer","minimum":0}},"required":["content","expectedRevision"],"additionalProperties":false},
            "annotations":{"readOnlyHint":false,"destructiveHint":false,"openWorldHint":false}
        }, {
            "name":"set_training_complete", "description":"Gerente treinado: aprova alma e treinamento de um membro após revisar as versões atuais da alma e memória e o término da conversa. Salvar conteúdo não aprova treinamento. Somente o usuário pode aprovar o gerente pela interface.",
            "inputSchema":{"type":"object","properties":{"terminalId":{"type":"string"},"expectedRevision":{"type":"integer","minimum":0},"expectedSoulRevision":{"type":"integer","minimum":0}},"required":["terminalId","expectedRevision","expectedSoulRevision"],"additionalProperties":false},
            "annotations":{"readOnlyHint":false,"destructiveHint":false,"openWorldHint":false}
        }]
    });
    list["tools"]
        .as_array_mut()
        .unwrap()
        .extend(crate::deliveries::tools());
    list
}

fn call_tool(params: Option<&Value>) -> Value {
    let Some(params) = params else {
        return tool_error("Os parâmetros da ferramenta não foram informados.");
    };
    let name = params.get("name").and_then(Value::as_str);
    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    if matches!(
        name,
        Some(
            "get_team"
                | "get_agent"
                | "get_tasks"
                | "set_assignment"
                | "reset_context"
                | "release_task"
                | "register_work_context"
                | "submit_delivery"
                | "get_deliveries"
                | "review_delivery"
                | "get_memory"
                | "set_memory"
                | "get_soul"
                | "set_soul"
                | "set_professional_profile"
                | "set_training_complete"
        )
    ) {
        return match call_ipc(None, name.map(str::to_owned), Some(arguments)) {
            Ok(response) => tool_result(response.data.unwrap_or(Value::Null)),
            Err(error) => tool_error(&error),
        };
    }
    if matches!(name, Some("get_task" | "get_task_steps")) {
        if !arguments
            .as_object()
            .is_some_and(|object| object.is_empty())
        {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase", deny_unknown_fields)]
            struct Query {
                task_code: String,
            }
            match serde_json::from_value::<Query>(arguments.clone()) {
                Ok(query) if !query.task_code.trim().is_empty() => (),
                _ => return tool_error("Esta consulta não aceita parâmetros."),
            }
            return match call_ipc(None, name.map(str::to_owned), Some(arguments)) {
                Ok(response) => tool_result(response.data.unwrap_or(Value::Null)),
                Err(error) => tool_error(&error),
            };
        }
        return match call_app(None) {
            Ok(context) if name == Some("get_task") => tool_result(get_task_payload(context)),
            Ok(context) => tool_result(get_task_steps_payload(context)),
            Err(error) => tool_error(&error),
        };
    }
    if name != Some("set_task") {
        return tool_error("Ferramenta MCP desconhecida.");
    }
    if arguments.get("taskCode").is_some() {
        if let Err(error) = scoped_task_patch(arguments.clone()) {
            return tool_error(&error);
        }
        return match call_ipc(None, Some("set_task".into()), Some(arguments)) {
            Ok(response) => tool_result(response.data.unwrap_or(Value::Null)),
            Err(error) => tool_error(&error),
        };
    }
    let patch = match serde_json::from_value::<TaskPatch>(arguments) {
        Ok(patch) => patch,
        Err(error) => return tool_error(&format!("Parâmetros inválidos: {error}")),
    };
    if patch.is_empty() {
        return tool_error("Informe ao menos um campo para atualizar a tarefa.");
    }
    match call_app(Some(patch)) {
        Ok(_) => json!({ "content": [{ "type": "text", "text": "ok" }], "isError": false }),
        Err(error) => tool_error(&error),
    }
}

fn scoped_task_patch(mut arguments: Value) -> Result<(String, TaskPatch), String> {
    let object = arguments
        .as_object_mut()
        .ok_or("Informe um objeto de parâmetros.")?;
    let code = object
        .remove("taskCode")
        .and_then(|v| v.as_str().map(str::to_owned))
        .ok_or("Informe taskCode.")?;
    let code = crate::office::task_code(code)?;
    let patch: TaskPatch = serde_json::from_value(arguments).map_err(|e| e.to_string())?;
    if patch.is_empty() {
        return Err("Informe ao menos um campo para atualizar a tarefa.".into());
    }
    Ok((code, patch))
}

fn get_task_payload(context: TaskContext) -> Value {
    json!({
        "status": context.task.status,
        "summary": context.task.summary,
        "current": context.task.current,
        "next": context.task.next,
        "blocker": context.task.blocker,
        "updatedAt": context.task.updated_at,
        "revision": context.task.revision
    })
}

fn get_task_steps_payload(context: TaskContext) -> Value {
    json!({
        "steps": context.task.steps,
        "updatedAt": context.task.updated_at,
        "revision": context.task.revision
    })
}

fn tool_result(payload: Value) -> Value {
    json!({
        "content": [{ "type": "text", "text": payload.to_string() }],
        "isError": false
    })
}

fn call_app(patch: Option<TaskPatch>) -> Result<TaskContext, String> {
    call_ipc(patch, None, None)?
        .context
        .ok_or_else(|| "O KanashaTerminal não retornou o contexto da tarefa.".into())
}

#[cfg(unix)]
fn call_ipc(
    patch: Option<TaskPatch>,
    tool: Option<String>,
    arguments: Option<Value>,
) -> Result<IpcResponse, String> {
    use std::os::unix::net::UnixStream;

    let socket = env::var("KANASHA_MCP_SOCKET").map_err(|_| {
        "Este MCP precisa ser iniciado por um terminal do KanashaTerminal.".to_string()
    })?;
    let request = IpcRequest {
        token: env::var("KANASHA_MCP_TOKEN").map_err(|_| {
            "A credencial local do KanashaTerminal não está disponível.".to_string()
        })?,
        workspace_id: env::var("KANASHA_WORKSPACE_ID")
            .map_err(|_| "O Workspace atual não foi identificado.".to_string())?,
        terminal_id: env::var("KANASHA_TERMINAL_ID")
            .map_err(|_| "O terminal atual não foi identificado.".to_string())?,
        patch,
        tool,
        arguments,
    };
    let mut stream = UnixStream::connect(socket)
        .map_err(|error| format!("O KanashaTerminal não está acessível: {error}"))?;
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(45)))
        .map_err(|e| e.to_string())?;
    stream
        .set_write_timeout(Some(std::time::Duration::from_secs(45)))
        .map_err(|e| e.to_string())?;
    serde_json::to_writer(&mut stream, &request)
        .map_err(|error| format!("Não foi possível enviar a atualização: {error}"))?;
    stream
        .write_all(b"\n")
        .and_then(|_| stream.flush())
        .map_err(|error| format!("Não foi possível enviar a atualização: {error}"))?;
    let mut response = String::new();
    BufReader::new(stream)
        .take(MAX_IPC_MESSAGE + 1)
        .read_line(&mut response)
        .map_err(|error| format!("Não foi possível receber a atualização: {error}"))?;
    if response.len() as u64 > MAX_IPC_MESSAGE {
        return Err("A resposta do KanashaTerminal excede o limite permitido.".into());
    }
    let response: IpcResponse = serde_json::from_str(&response)
        .map_err(|error| format!("A resposta do KanashaTerminal é inválida: {error}"))?;
    if response.ok {
        Ok(response)
    } else {
        Err(response
            .error
            .unwrap_or_else(|| "O KanashaTerminal recusou a atualização.".into()))
    }
}

#[cfg(not(unix))]
fn call_ipc(
    _patch: Option<TaskPatch>,
    _tool: Option<String>,
    _arguments: Option<Value>,
) -> Result<IpcResponse, String> {
    Err("O MCP local ainda não está disponível neste sistema operacional.".into())
}

fn tool_error(message: &str) -> Value {
    json!({
        "content": [{ "type": "text", "text": message }],
        "isError": true
    })
}

fn rpc_result(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn rpc_error(id: Value, code: i64, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scoped_tasks_and_assignments_expose_real_step_relationships() {
        let list = tools_list();
        let tools = list["tools"].as_array().unwrap();
        for name in ["get_task", "get_task_steps", "set_task"] {
            let tool = tools.iter().find(|tool| tool["name"] == name).unwrap();
            assert_eq!(
                tool["inputSchema"]["properties"]["taskCode"]["maxLength"],
                80
            );
        }
        let assignment = tools
            .iter()
            .find(|tool| tool["name"] == "set_assignment")
            .unwrap();
        assert_eq!(
            assignment["inputSchema"]["properties"]["stepIds"]["uniqueItems"],
            true
        );
        assert_eq!(
            assignment["inputSchema"]["properties"]["stepIds"]["maxItems"],
            20
        );
        let (code, patch) = scoped_task_patch(
            json!({"taskCode":"b2c-123", "stepStatus":{"id":"plano","status":"done"}}),
        )
        .unwrap();
        assert_eq!(code, "B2C-123");
        assert_eq!(patch.step_status.unwrap().id, "plano");
        assert!(scoped_task_patch(json!({"taskCode":"B2C-123"})).is_err());
        assert!(scoped_task_patch(json!({"taskCode":"B2C-123","unexpected":true})).is_err());
    }

    #[test]
    fn exposes_separate_read_tools_and_the_existing_update_tool() {
        let tools = tools_list();
        let list = tools["tools"].as_array().expect("tools array");
        assert_eq!(list.len(), 19);
        assert_eq!(list[0]["name"], "get_task");
        assert_eq!(list[1]["name"], "get_task_steps");
        assert_eq!(list[2]["name"], "set_task");
        for tool in &list[..2] {
            assert_eq!(tool["annotations"]["readOnlyHint"], true);
            assert_eq!(tool["inputSchema"]["additionalProperties"], false);
        }
        assert_eq!(
            list[2]["inputSchema"]["properties"]["status"]["enum"]
                .as_array()
                .expect("statuses")
                .len(),
            5
        );
        assert_eq!(
            list[2]["inputSchema"]["properties"]["agentStatus"]["enum"]
                .as_array()
                .expect("agent statuses")
                .len(),
            5
        );
        assert_eq!(
            list[2]["inputSchema"]["properties"]["agentCurrent"]["maxLength"],
            160
        );
        assert_eq!(
            list[2]["inputSchema"]["properties"]["summary"]["maxLength"],
            8000
        );
        assert_eq!(
            list[2]["inputSchema"]["properties"]["steps"]["maxItems"],
            20
        );
        assert_eq!(
            list[2]["inputSchema"]["properties"]["stepStatus"]["required"],
            json!(["id", "status"])
        );
    }

    #[test]
    fn task_reads_keep_the_description_and_steps_separate() {
        let context = TaskContext {
            task: crate::models::TaskState {
                status: crate::models::TaskStatus::InProgress,
                summary:
                    "# Objetivo\nInvestigar a sincronização.\n\n## Escopo\nConferir persistência."
                        .into(),
                steps: vec![crate::models::TaskStep {
                    id: "conferir".into(),
                    title: "Conferir dados".into(),
                    status: crate::models::TaskStatus::Done,
                }],
                current: "Validando o dado".into(),
                next: "Concluir".into(),
                blocker: String::new(),
                updated_at: 123,
                revision: 2,
            },
            agent: crate::models::AgentState::default(),
        };
        let task = get_task_payload(context.clone());
        assert_eq!(task["status"], "in_progress");
        assert_eq!(task["summary"], context.task.summary);
        assert_eq!(task["revision"], 2);
        assert!(task.get("steps").is_none());
        assert!(task.get("agent").is_none());

        let steps = get_task_steps_payload(context);
        assert_eq!(steps["steps"][0]["title"], "Conferir dados");
        assert_eq!(steps["steps"][0]["status"], "done");
        assert_eq!(steps["revision"], 2);
        assert!(steps.get("summary").is_none());
    }

    #[test]
    fn read_tools_reject_updates_and_set_requires_a_change() {
        let read = call_tool(Some(&json!({
            "name": "get_task",
            "arguments": { "status": "done" }
        })));
        assert_eq!(read["isError"], true);
        assert_eq!(
            read["content"][0]["text"],
            "Esta consulta não aceita parâmetros."
        );

        let empty_set = call_tool(Some(&json!({
            "name": "set_task",
            "arguments": {}
        })));
        assert_eq!(empty_set["isError"], true);
        assert_eq!(
            empty_set["content"][0]["text"],
            "Informe ao menos um campo para atualizar a tarefa."
        );
    }

    #[test]
    fn initializes_with_tools_only() {
        let response = handle_rpc(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": { "protocolVersion": "2025-06-18" }
        }))
        .expect("response");
        assert_eq!(response["result"]["protocolVersion"], "2025-06-18");
        assert_eq!(
            response["result"]["capabilities"]["tools"]["listChanged"],
            false
        );
    }

    #[test]
    fn soul_tools_use_their_own_revision_and_training_reviews_both_documents() {
        let tools = tools_list();
        let list = tools["tools"].as_array().unwrap();
        let get = list.iter().find(|t| t["name"] == "get_soul").unwrap();
        assert_eq!(get["annotations"]["readOnlyHint"], true);
        let set = list.iter().find(|t| t["name"] == "set_soul").unwrap();
        assert_eq!(
            set["inputSchema"]["required"],
            json!(["content", "expectedRevision"])
        );
        assert_eq!(
            set["inputSchema"]["properties"]["content"]["maxLength"],
            12000
        );
        assert_eq!(set["inputSchema"]["additionalProperties"], false);
        let approve = list
            .iter()
            .find(|t| t["name"] == "set_training_complete")
            .unwrap();
        assert_eq!(
            approve["inputSchema"]["required"],
            json!(["terminalId", "expectedRevision", "expectedSoulRevision"])
        );
        assert!(!list.iter().any(|t| t["name"] == "approve_soul"));
    }

    #[test]
    fn full_memory_context_with_two_soul_versions_fits_the_ipc_limit() {
        let response = json!({"ok":true,"data":{
            "memory":{"content":"🧪".repeat(16000),"revision":1},
            "soul":{"content":"🧪".repeat(12000),"revision":2,
                "approved":{"content":"🧪".repeat(12000),"revision":1,"approvedAt":1,"approvedBy":"user"}},
            "professionalProfile":{"content":"🧪".repeat(6000),"personality":"🧪".repeat(800),"name":"🧪".repeat(120)},
            "name":"🧪".repeat(120),"specialty":"🧪".repeat(120),"company":"🧪".repeat(120),"workspace":"🧪".repeat(120)
        }});
        assert!(serde_json::to_vec(&response).unwrap().len() as u64 + 1 <= MAX_IPC_MESSAGE);
    }

    #[test]
    fn maximum_step_list_fits_the_local_ipc_limit() {
        let steps: Vec<Value> = (0..20)
            .map(|index| {
                json!({
                    "id": format!("step_{index}"),
                    "title": "🧪".repeat(160),
                    "status": "in_progress"
                })
            })
            .collect();
        let request = json!({
            "token": "x".repeat(36),
            "workspaceId": "x".repeat(36),
            "terminalId": "x".repeat(36),
            "patch": {
                "summary": "🧪".repeat(8000),
                "steps": steps,
                "current": "🧪".repeat(200),
                "next": "🧪".repeat(200),
                "blocker": "🧪".repeat(200),
                "agentCurrent": "🧪".repeat(160)
            }
        });
        let encoded = serde_json::to_vec(&request).expect("encoded request");
        assert!(encoded.len() as u64 + 1 <= MAX_IPC_MESSAGE);
        let response = json!({
            "ok": true,
            "context": {
                "task": {
                    "status": "in_progress",
                    "summary": "🧪".repeat(8000),
                    "steps": request["patch"]["steps"],
                    "current": "🧪".repeat(200),
                    "next": "🧪".repeat(200),
                    "blocker": "🧪".repeat(200)
                },
                "agent": { "current": "🧪".repeat(160) }
            }
        });
        let encoded = serde_json::to_vec(&response).expect("encoded response");
        assert!(encoded.len() as u64 + 1 <= MAX_IPC_MESSAGE);
    }
}
