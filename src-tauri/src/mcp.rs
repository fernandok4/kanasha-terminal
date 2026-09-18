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

const MAX_IPC_MESSAGE: u64 = 16 * 1024;

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct IpcRequest {
    token: String,
    workspace_id: String,
    terminal_id: String,
    patch: Option<TaskPatch>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct IpcResponse {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    context: Option<TaskContext>,
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
                    Ok(stream) => handle_ipc(stream, &state, &app),
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
        if request.token != state.mcp_token {
            return Err("A solicitação MCP não foi autorizada.".into());
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
            error: None,
        })
    })()
    .unwrap_or_else(|error| IpcResponse {
        ok: false,
        context: None,
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
                "instructions": "Leia task_state ao iniciar. Envie agentStatus=working ao começar; atualize agentCurrent só ao mudar de etapa; use waiting, blocked, done ou idle nas transições. Envie apenas campos alterados."
            }),
        )),
        "ping" => Some(rpc_result(id, json!({}))),
        "tools/list" => Some(rpc_result(id, tools_list())),
        "tools/call" => Some(rpc_result(id, call_tool(request.get("params")))),
        _ => Some(rpc_error(id, -32601, "Método MCP não encontrado.")),
    }
}

fn tools_list() -> Value {
    json!({
        "tools": [{
            "name": "task_state",
            "description": "Lê o contexto compartilhado e o estado deste agente; atualiza somente os campos enviados.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "status": {
                        "type": "string",
                        "enum": ["not_started", "in_progress", "waiting", "blocked", "done"]
                    },
                    "summary": { "type": "string", "maxLength": 800 },
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
        }]
    })
}

fn call_tool(params: Option<&Value>) -> Value {
    let Some(params) = params else {
        return tool_error("Os parâmetros da ferramenta não foram informados.");
    };
    if params.get("name").and_then(Value::as_str) != Some("task_state") {
        return tool_error("Ferramenta MCP desconhecida.");
    }
    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let patch = match serde_json::from_value::<TaskPatch>(arguments) {
        Ok(patch) => patch,
        Err(error) => return tool_error(&format!("Parâmetros inválidos: {error}")),
    };
    let changed = !patch.is_empty();
    match call_app(if changed { Some(patch) } else { None }) {
        Ok(context) => {
            let text = if changed {
                "ok".to_string()
            } else {
                serde_json::to_string(&context).unwrap_or_else(|_| "{}".into())
            };
            json!({ "content": [{ "type": "text", "text": text }], "isError": false })
        }
        Err(error) => tool_error(&error),
    }
}

#[cfg(unix)]
fn call_app(patch: Option<TaskPatch>) -> Result<TaskContext, String> {
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
    };
    let mut stream = UnixStream::connect(socket)
        .map_err(|error| format!("O KanashaTerminal não está acessível: {error}"))?;
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
        response
            .context
            .ok_or_else(|| "O KanashaTerminal não retornou o contexto da tarefa.".to_string())
    } else {
        Err(response
            .error
            .unwrap_or_else(|| "O KanashaTerminal recusou a atualização.".into()))
    }
}

#[cfg(not(unix))]
fn call_app(_patch: Option<TaskPatch>) -> Result<TaskContext, String> {
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
    fn exposes_one_small_task_tool() {
        let tools = tools_list();
        let list = tools["tools"].as_array().expect("tools array");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0]["name"], "task_state");
        assert_eq!(
            list[0]["inputSchema"]["properties"]["status"]["enum"]
                .as_array()
                .expect("statuses")
                .len(),
            5
        );
        assert_eq!(
            list[0]["inputSchema"]["properties"]["agentStatus"]["enum"]
                .as_array()
                .expect("agent statuses")
                .len(),
            5
        );
        assert_eq!(
            list[0]["inputSchema"]["properties"]["agentCurrent"]["maxLength"],
            160
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
}
