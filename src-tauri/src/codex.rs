//! One local Codex server per employee. Authentication stays with the installed CLI.
//! A private Unix socket is shared by the real terminal UI and this structured client.
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    io,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{mpsc, Mutex},
    thread,
    time::{Duration, Instant},
};

type Reply = mpsc::Sender<Result<Value, String>>;
enum Operation {
    Request(String, Value, Reply),
    Response(Value),
    Stop,
}

pub struct CodexSession {
    pub model: String,
    pub reasoning_effort: String,
    pub thread_id: String,
    pub endpoint: String,
    pub connection_id: String,
    sender: mpsc::Sender<Operation>,
    child: Mutex<Child>,
    directory: PathBuf,
}

impl CodexSession {
    #[cfg(test)]
    pub(crate) fn test_session(thread_id: &str) -> Self {
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            while let Ok(operation) = receiver.recv() {
                match operation {
                    Operation::Request(_, _, reply) => {
                        let _ = reply.send(Ok(json!({})));
                    }
                    Operation::Stop => break,
                    Operation::Response(_) => {}
                }
            }
        });
        let directory =
            std::env::temp_dir().join(format!("kt-session-test-{}", crate::models::new_id()));
        Self {
            model: "test".into(),
            reasoning_effort: "low".into(),
            thread_id: thread_id.into(),
            endpoint: String::new(),
            connection_id: crate::models::new_id(),
            sender,
            child: Mutex::new(Command::new("sleep").arg("60").spawn().unwrap()),
            directory,
        }
    }

    #[cfg(unix)]
    fn connect(
        root: &Path,
        arguments: &[String],
        environment: &[(String, String)],
        on_event: impl Fn(Value) + Send + 'static,
    ) -> Result<Self, String> {
        use std::os::unix::{fs::PermissionsExt, net::UnixStream};
        // Keep the socket below Unix's path-length limit; mode 0700 also protects the log.
        let directory = PathBuf::from("/tmp").join(format!("kt-{}", crate::models::new_id()));
        std::fs::create_dir(&directory).map_err(|e| e.to_string())?;
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700))
            .map_err(|e| e.to_string())?;
        let socket = directory.join("codex.sock");
        let endpoint = format!("unix://{}", socket.display());
        let log = std::fs::File::create(directory.join("codex.log")).map_err(|e| e.to_string())?;
        let mut command = Command::new("codex");
        command
            .args(arguments)
            .args([
                "-c",
                "agents.enabled=false",
                // Remote resume rejects permission flags on the TUI. Set the
                // server defaults too, so its clients inherit YOLO permissions.
                "-c",
                "approval_policy=\"never\"",
                "-c",
                "sandbox_mode=\"danger-full-access\"",
                "app-server",
                "--listen",
                &endpoint,
            ])
            .envs(environment.iter().cloned())
            .current_dir(root)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(log);
        let child = match command.spawn() {
            Ok(child) => child,
            Err(e) => {
                let _ = std::fs::remove_dir_all(&directory);
                return Err(format!("Não foi possível iniciar o Codex: {e}"));
            }
        };
        let (sender, receiver) = mpsc::channel();
        let session = Self {
            model: String::new(),
            reasoning_effort: String::new(),
            thread_id: String::new(),
            endpoint,
            connection_id: directory.to_string_lossy().into_owned(),
            sender,
            child: Mutex::new(child),
            directory,
        };
        let deadline = Instant::now() + Duration::from_secs(15);
        while !socket.exists() {
            if session
                .child
                .lock()
                .map_err(|e| e.to_string())?
                .try_wait()
                .map_err(|e| e.to_string())?
                .is_some()
            {
                return Err(
                    "O Codex encerrou ao iniciar. Verifique a instalação e a configuração do CLI."
                        .into(),
                );
            }
            if Instant::now() > deadline {
                return Err("O Codex não disponibilizou sua conexão local a tempo.".into());
            }
            thread::sleep(Duration::from_millis(50));
        }
        let stream = UnixStream::connect(&socket).map_err(|e| e.to_string())?;
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .map_err(|e| e.to_string())?;
        stream
            .set_write_timeout(Some(Duration::from_secs(10)))
            .map_err(|e| e.to_string())?;
        let (mut websocket, _) = tungstenite::client("ws://localhost/", stream)
            .map_err(|e| format!("Conexão com o Codex incompatível: {e}"))?;
        websocket
            .get_mut()
            .set_read_timeout(Some(Duration::from_millis(50)))
            .map_err(|e| e.to_string())?;
        // Events run separately: callbacks may themselves send requests or update the application.
        let (events, event_receiver) = mpsc::channel();
        thread::spawn(move || {
            for event in event_receiver {
                on_event(event);
            }
        });
        let connection_id = session.connection_id.clone();
        thread::spawn(move || {
            let mut pending: HashMap<u64, Reply> = HashMap::new();
            let mut sequence = 0_u64;
            'connection: loop {
                for operation in receiver.try_iter() {
                    let message = match operation {
                        Operation::Stop => break 'connection,
                        Operation::Response(value) => value,
                        Operation::Request(method, params, reply) => {
                            sequence += 1;
                            pending.insert(sequence, reply);
                            json!({ "id": sequence, "method": method, "params": params })
                        }
                    };
                    if websocket
                        .send(tungstenite::Message::Text(message.to_string().into()))
                        .is_err()
                    {
                        break 'connection;
                    }
                }
                match websocket.read() {
                    Ok(tungstenite::Message::Text(text)) => {
                        if let Ok(mut value) = serde_json::from_str::<Value>(&text) {
                            if value.get("method").is_some() {
                                value["_connection"] = json!(connection_id);
                                let _ = events.send(value);
                            } else if let Some(reply) =
                                value["id"].as_u64().and_then(|id| pending.remove(&id))
                            {
                                let result = if let Some(error) = value.get("error") {
                                    Err(error["message"]
                                        .as_str()
                                        .unwrap_or("Erro no Codex")
                                        .to_owned())
                                } else {
                                    Ok(value["result"].clone())
                                };
                                let _ = reply.send(result);
                            }
                        }
                    }
                    Ok(tungstenite::Message::Close(_)) => break,
                    Ok(_) => {}
                    Err(tungstenite::Error::Io(e))
                        if matches!(
                            e.kind(),
                            io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                        ) => {}
                    Err(_) => break,
                }
            }
            for (_, reply) in pending {
                let _ = reply.send(Err("A conexão com o Codex foi encerrada.".into()));
            }
            let _ =
                events.send(json!({"method":"kanasha/disconnected", "_connection":connection_id}));
        });
        session.request(
            "initialize",
            json!({"clientInfo":{"name":"kanasha_terminal","version":env!("CARGO_PKG_VERSION")}}),
        )?;
        session
            .sender
            .send(Operation::Response(json!({"method":"initialized"})))
            .map_err(|e| e.to_string())?;
        Ok(session)
    }

    /// Query the same catalog used by execution, without creating a thread or turn.
    pub fn list_models(root: &Path) -> Result<Vec<Value>, String> {
        let session = Self::connect(root, &[], &[], |_| {})?;
        session.models()
    }

    fn models(&self) -> Result<Vec<Value>, String> {
        read_model_catalog(|params| self.request("model/list", params))
    }

    #[cfg(unix)]
    pub fn start(
        root: &Path,
        arguments: &[String],
        environment: &[(String, String)],
        existing_thread: Option<&str>,
        instructions: &str,
        execution: Option<&crate::execution::AgentExecution>,
        on_event: impl Fn(Value) + Send + 'static,
    ) -> Result<Self, String> {
        let mut session = Self::connect(root, arguments, environment, on_event)?;
        let mut params = json!({"cwd":root,"developerInstructions":instructions,"approvalPolicy":"never","sandbox":"danger-full-access"});
        if let Some(execution) = execution {
            execution.verify_catalog(&session.models()?)?;
            execution.apply_thread(&mut params);
        }
        let method = if let Some(thread_id) = existing_thread {
            params["threadId"] = json!(thread_id);
            params["excludeTurns"] = json!(true);
            "thread/resume"
        } else {
            "thread/start"
        };
        let response = session.request(method, params)?;
        if let Some(execution) = execution {
            execution.verify_response(&response)?;
        }
        session.model = response["model"].as_str().unwrap_or_default().into();
        session.reasoning_effort = response["reasoningEffort"]
            .as_str()
            .unwrap_or_default()
            .into();
        session.thread_id = response["thread"]["id"]
            .as_str()
            .ok_or("O Codex não retornou o identificador da sessão.")?
            .to_owned();
        if existing_thread.is_none() {
            // The remote TUI resumes by thread id. Persist a new thread before
            // the TUI connects, without starting an inference turn.
            session.request(
                "thread/inject_items",
                json!({
                    "threadId": session.thread_id,
                    "items": [{"type":"message","role":"developer","content":[{
                        "type":"input_text","text":"Sessão KanashaTerminal inicializada."
                    }]}]
                }),
            )?;
        }
        Ok(session)
    }

    pub fn request(&self, method: &str, params: Value) -> Result<Value, String> {
        let (reply, response) = mpsc::channel();
        self.sender
            .send(Operation::Request(method.to_owned(), params, reply))
            .map_err(|_| "O Codex não está conectado.".to_string())?;
        response
            .recv_timeout(Duration::from_secs(30))
            .map_err(|_| {
                "O Codex não confirmou a operação. Consulte o estado antes de repetir.".to_string()
            })?
    }

    pub fn respond(&self, id: Value, result: Value) -> Result<(), String> {
        self.sender
            .send(Operation::Response(json!({"id":id,"result":result})))
            .map_err(|e| e.to_string())
    }

    pub fn respond_error(&self, id: Value, message: &str) -> Result<(), String> {
        self.sender
            .send(Operation::Response(
                json!({"id":id,"error":{"code":-32601,"message":message}}),
            ))
            .map_err(|e| e.to_string())
    }
}

// Keep pagination identical for the picker and execution validation.
fn read_model_catalog(
    mut request: impl FnMut(Value) -> Result<Value, String>,
) -> Result<Vec<Value>, String> {
    let mut models = Vec::new();
    let mut cursor = Value::Null;
    let mut visited = std::collections::HashSet::new();
    for _ in 0..100 {
        let response = request(json!({"cursor":cursor,"includeHidden":false}))?;
        let page = response["data"]
            .as_array()
            .ok_or("Catálogo de modelos inválido.")?;
        for model in page {
            if model["model"].as_str().is_none()
                || model["supportedReasoningEfforts"].as_array().is_none()
            {
                return Err("Catálogo de modelos inválido.".into());
            }
            if model["hidden"].as_bool() != Some(true)
                && !models
                    .iter()
                    .any(|existing: &Value| existing["model"] == model["model"])
            {
                models.push(model.clone());
            }
        }
        let next = response["nextCursor"].clone();
        if next.is_null() {
            return Ok(models);
        }
        let token = next
            .as_str()
            .filter(|token| !token.is_empty())
            .ok_or("Cursor do catálogo inválido.")?;
        if !visited.insert(token.to_owned()) {
            return Err("O catálogo de modelos não avançou.".into());
        }
        cursor = next;
    }
    Err("O catálogo excedeu o limite de páginas.".into())
}

impl Drop for CodexSession {
    fn drop(&mut self) {
        let _ = self.sender.send(Operation::Stop);
        if let Ok(mut child) = self.child.lock() {
            let _ = child.kill();
            let _ = child.wait();
        }
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    #[test]
    fn model_catalog_reads_every_page_and_deduplicates_without_exposing_hidden_models() {
        let model = |id: &str| json!({"model":id,"supportedReasoningEfforts":[{"reasoningEffort":"ultra"}]});
        let mut count = 0;
        let result = read_model_catalog(|params| {
            assert_eq!(params["includeHidden"], false);
            count += 1;
            if count == 1 {
                assert!(params["cursor"].is_null());
                Ok(json!({"data":[model("gpt-6.1-sol")],"nextCursor":"page2"}))
            } else {
                assert_eq!(params["cursor"], "page2");
                Ok(json!({"data":[model("gpt-6.1-sol"),model("gpt-5.6-terra"),{"model":"hidden","hidden":true,"supportedReasoningEfforts":[]}],"nextCursor":null}))
            }
        }).unwrap();
        assert_eq!(count, 2);
        assert_eq!(result.len(), 2);
        assert_eq!(result[1]["model"], "gpt-5.6-terra");
    }

    #[test]
    fn model_catalog_rejects_cycles_malformed_pages_and_partial_failure() {
        let mut count = 0;
        assert!(read_model_catalog(|_| {
            count += 1;
            Ok(json!({"data":[],"nextCursor":if count % 2 == 0 { "b" } else { "a" }}))
        })
        .is_err());
        assert_eq!(count, 3);
        assert!(read_model_catalog(|_| Ok(json!({"data":{}}))).is_err());
        assert!(read_model_catalog(|_| Err("Catálogo indisponível".into())).is_err());
    }

    #[test]
    #[ignore = "requires an installed authenticated Codex; catalog only, no inference"]
    fn installed_codex_lists_models_without_creating_a_thread() {
        let root = std::env::current_dir().unwrap();
        let session = CodexSession::connect(&root, &[], &[], |_| {}).unwrap();
        let models = session.models().unwrap();
        assert!(!models.is_empty());
        assert!(session.thread_id.is_empty());
        for model in &models {
            println!("{}", model["model"].as_str().unwrap());
        }
        let loaded = session.request("thread/loaded/list", json!({})).unwrap();
        assert_eq!(loaded["data"].as_array().unwrap().len(), 0);
    }

    #[test]
    #[ignore = "uses the installed Codex and ChatGPT login; runs one short inference"]
    fn installed_codex_applies_per_person_models_and_resumes_same_thread() {
        use crate::execution::AgentExecution;
        let (events, receiver) = mpsc::channel();
        let junior = AgentExecution::preset("junior").unwrap();
        let session = CodexSession::start(
            Path::new("/tmp"),
            &junior.cli_arguments(),
            &[],
            None,
            "Integration check. Answer only MODEL_PROFILE_OK. Do not call tools.",
            Some(&junior),
            move |e| {
                let _ = events.send(e);
            },
        )
        .unwrap();
        assert_eq!(
            session
                .request("account/read", json!({"refreshToken":false}))
                .unwrap()["account"]["type"],
            "chatgpt"
        );
        session
            .request(
                "turn/start",
                junior.turn_params(
                    &session.thread_id,
                    "Respond MODEL_PROFILE_OK without using tools.",
                ),
            )
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(120);
        let mut answer = String::new();
        loop {
            let event = receiver
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .expect("Codex inference timeout");
            if event["method"] == "item/completed"
                && event["params"]["item"]["type"] == "agentMessage"
            {
                answer.push_str(event["params"]["item"]["text"].as_str().unwrap_or_default());
            }
            if event["method"] == "turn/completed" {
                assert_eq!(event["params"]["turn"]["status"], "completed", "{event}");
                break;
            }
        }
        assert!(answer.contains("MODEL_PROFILE_OK"), "{answer}");
        let thread_id = session.thread_id.clone();
        drop(session);
        for level in ["mid", "senior", "specialist", "manager"] {
            let config = AgentExecution::preset(level).unwrap();
            let session = CodexSession::start(
                Path::new("/tmp"),
                &config.cli_arguments(),
                &[],
                Some(&thread_id),
                "Integration check. Do not run tools.",
                Some(&config),
                |_| {},
            )
            .unwrap();
            assert_eq!(session.thread_id, thread_id);
            assert_eq!(session.model, config.model);
            assert_eq!(session.reasoning_effort, config.reasoning_effort);
            let history = session
                .request(
                    "thread/read",
                    json!({"threadId":thread_id,"includeTurns":true}),
                )
                .unwrap();
            assert!(history["thread"]["turns"]
                .to_string()
                .contains("MODEL_PROFILE_OK"));
            eprintln!(
                "Codex confirmed: {} / {}, same thread and history",
                session.model, session.reasoning_effort
            );
        }
    }
    #[test]
    #[ignore = "requires the installed Codex CLI; creates a local session without an inference"]
    fn installed_codex_accepts_private_socket_and_returns_session_id() {
        let session = CodexSession::start(
            Path::new("/tmp"),
            &[],
            &[],
            None,
            "Kanasha integration probe. Do not run work.",
            None,
            |_| {},
        )
        .unwrap();
        assert!(!session.thread_id.is_empty());
        let account = session
            .request("account/read", json!({"refreshToken":false}))
            .unwrap();
        assert_eq!(
            account["account"]["type"], "chatgpt",
            "This integration must work with ChatGPT login"
        );
        // The bootstrap item makes the new thread resumable before its first turn.
    }

    #[test]
    #[ignore = "requires the installed Codex CLI and a PTY; tests the manager terminal connection"]
    fn installed_codex_remote_ui_attaches_to_manager_session() {
        let manager = crate::execution::AgentExecution::manager();
        let session = CodexSession::start(
            Path::new("/tmp"),
            &manager.cli_arguments(),
            &[],
            None,
            "Kanasha manager terminal probe. Do not run work.",
            Some(&manager),
            |_| {},
        )
        .unwrap();
        let config = session.request("config/read", json!({})).unwrap();
        assert_eq!(config["config"]["approval_policy"], "never");
        assert_eq!(config["config"]["sandbox_mode"], "danger-full-access");
        let mut arguments = manager.cli_arguments();
        arguments.extend([
            "--remote".into(),
            session.endpoint.clone(),
            "resume".into(),
            session.thread_id.clone(),
        ]);
        let output = std::sync::Arc::new(Mutex::new(String::new()));
        let output_for_reader = output.clone();
        let (exited, receiver) = mpsc::channel();
        let mut terminal = crate::terminal::start_pty(
            "codex",
            &arguments,
            &[],
            Path::new("/tmp"),
            move |chunk| output_for_reader.lock().unwrap().push_str(&chunk),
            move || {
                let _ = exited.send(());
            },
        )
        .unwrap();
        let result = receiver.recv_timeout(Duration::from_secs(3));
        if result.is_ok() {
            panic!("manager terminal exited early: {}", output.lock().unwrap());
        }
        terminal.stop().unwrap();
    }

    #[test]
    #[ignore = "uses installed Codex login; one isolated inference with sleep, no workspace files"]
    fn installed_codex_turn_survives_remote_display_switch() {
        let config = crate::execution::AgentExecution::preset("junior").unwrap();
        let (events, receiver) = mpsc::channel();
        let session = CodexSession::start(
            Path::new("/tmp"), &config.cli_arguments(), &[], None,
            "Isolated terminal lifecycle test. Only run the sleep command requested by the user. Do not read or write files, load skills, or call other tools.",
            Some(&config), move |event| { let _ = events.send(event); },
        ).unwrap();
        let mut arguments = config.cli_arguments();
        arguments.extend([
            "--remote".into(),
            session.endpoint.clone(),
            "resume".into(),
            session.thread_id.clone(),
        ]);
        let mut terminal =
            crate::terminal::start_pty("codex", &arguments, &[], Path::new("/tmp"), |_| {}, || {})
                .unwrap();
        let started = session.request("turn/start", config.turn_params(&session.thread_id,
            "Execute the shell command /bin/sleep 5, then respond exactly KANASHA_BACKGROUND_OK. No other commands or tools.")).unwrap();
        let turn = started["turn"]["id"].as_str().unwrap();
        let deadline = Instant::now() + Duration::from_secs(90);
        loop {
            let event = receiver
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .unwrap();
            assert_ne!(
                event["method"], "turn/completed",
                "Turn completed before the sleep command: {event}"
            );
            if event["method"] == "item/started"
                && event["params"]["item"]["type"] == "commandExecution"
            {
                break;
            }
        }
        // This is exactly what navigating away does to the display client.
        // The independent structured connection and app server remain alive.
        terminal.stop().unwrap();
        loop {
            let event = receiver
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .unwrap();
            if event["method"] == "turn/completed" {
                assert_eq!(event["params"]["turn"]["id"], turn);
                assert_eq!(event["params"]["turn"]["status"], "completed", "{event}");
                break;
            }
        }
        let history = session
            .request(
                "thread/read",
                json!({"threadId":session.thread_id,"includeTurns":true}),
            )
            .unwrap();
        assert!(history["thread"]["turns"]
            .to_string()
            .contains("KANASHA_BACKGROUND_OK"));
        let mut resumed_display =
            crate::terminal::start_pty("codex", &arguments, &[], Path::new("/tmp"), |_| {}, || {})
                .unwrap();
        assert_eq!(
            session
                .request(
                    "thread/read",
                    json!({"threadId":session.thread_id,"includeTurns":true})
                )
                .unwrap()["thread"]["turns"],
            history["thread"]["turns"]
        );
        resumed_display.stop().unwrap();
    }
}
