mod models;
mod terminal;

use crate::{
    models::{
        new_id, AppSnapshot, Area, ConfigureProfileInput, CreateAreaInput, CreateTerminalInput,
        CreateWorkspaceInput, PersistedProfile, PersistedState, ProfileView, ReorderDirection,
        ResizeTerminalInput, SetSplitRatioInput, SetTerminalLabelInput, SplitDirection,
        TerminalExit, TerminalOutput, TerminalPanel, Workspace,
    },
    terminal::{start_pty, PtySession},
};
use std::{
    collections::HashMap,
    env, fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use tauri::{AppHandle, Emitter, Manager, State};

const SHELL: &str = "shell";
const CODEX: &str = "codex";
const CLAUDE: &str = "claude";
const GEMINI: &str = "gemini";

#[derive(Clone)]
struct RuntimeProfile {
    executable: String,
    arguments: Vec<String>,
}

#[derive(Clone)]
struct CommandSpec {
    executable: String,
    arguments: Vec<String>,
}

pub struct AppState {
    state_path: PathBuf,
    persisted: Mutex<PersistedState>,
    sessions: Arc<Mutex<HashMap<String, PtySession>>>,
    runtime_profiles: Mutex<HashMap<String, RuntimeProfile>>,
}

impl AppState {
    fn load(state_path: PathBuf) -> Result<Self, String> {
        let mut persisted = match fs::read_to_string(&state_path) {
            Ok(contents) => serde_json::from_str(&contents).map_err(|error| {
                format!("A configuração local está inválida e foi preservada: {error}")
            })?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => PersistedState {
                version: 2,
                ..Default::default()
            },
            Err(error) => {
                return Err(format!(
                    "Não foi possível ler a configuração local: {error}"
                ))
            }
        };
        persisted.version = 2;
        Ok(Self {
            state_path,
            persisted: Mutex::new(persisted),
            sessions: Arc::new(Mutex::new(HashMap::new())),
            runtime_profiles: Mutex::new(HashMap::new()),
        })
    }

    fn snapshot(&self) -> Result<AppSnapshot, String> {
        let persisted = guard(&self.persisted)?;
        let runtime_profiles = guard(&self.runtime_profiles)?;
        let profiles = profile_views(&persisted, &runtime_profiles);
        let active_terminal_ids = guard(&self.sessions)?.keys().cloned().collect();
        Ok(AppSnapshot {
            areas: persisted.areas.clone(),
            profiles,
            active_terminal_ids,
        })
    }

    fn create_area(&self, input: CreateAreaInput) -> Result<AppSnapshot, String> {
        let mut persisted = guard(&self.persisted)?;
        let name = name(&input.name, "área")?;
        let root_path = directory(&input.root_path)?;
        persisted.areas.push(Area {
            id: new_id(),
            name,
            root_path,
            workspaces: vec![Workspace {
                id: new_id(),
                name: "Workspace 1".to_string(),
                panels: Vec::new(),
                layout: None,
            }],
        });
        self.save(&persisted)?;
        drop(persisted);
        self.snapshot()
    }

    fn rename_area(&self, area_id: &str, new_name: &str) -> Result<AppSnapshot, String> {
        let mut persisted = guard(&self.persisted)?;
        find_area_mut(&mut persisted, area_id)?.name = name(new_name, "área")?;
        self.save(&persisted)?;
        drop(persisted);
        self.snapshot()
    }

    fn change_area_root(&self, area_id: &str, path: &str) -> Result<AppSnapshot, String> {
        let mut persisted = guard(&self.persisted)?;
        find_area_mut(&mut persisted, area_id)?.root_path = directory(path)?;
        self.save(&persisted)?;
        drop(persisted);
        self.snapshot()
    }

    fn move_area(&self, area_id: &str, direction: ReorderDirection) -> Result<AppSnapshot, String> {
        let mut persisted = guard(&self.persisted)?;
        move_matching(&mut persisted.areas, |area| area.id == area_id, direction)?;
        self.save(&persisted)?;
        drop(persisted);
        self.snapshot()
    }

    fn remove_area(&self, area_id: &str) -> Result<AppSnapshot, String> {
        let ids = {
            let persisted = guard(&self.persisted)?;
            find_area(&persisted, area_id)?
                .workspaces
                .iter()
                .flat_map(Workspace::panel_ids)
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        };
        self.require_inactive(&ids)?;
        let mut persisted = guard(&self.persisted)?;
        persisted.areas.retain(|area| area.id != area_id);
        self.save(&persisted)?;
        drop(persisted);
        self.snapshot()
    }

    fn create_workspace(&self, input: CreateWorkspaceInput) -> Result<AppSnapshot, String> {
        let mut persisted = guard(&self.persisted)?;
        find_area_mut(&mut persisted, &input.area_id)?
            .workspaces
            .push(Workspace {
                id: new_id(),
                name: name(&input.name, "workspace")?,
                panels: Vec::new(),
                layout: None,
            });
        self.save(&persisted)?;
        drop(persisted);
        self.snapshot()
    }

    fn rename_workspace(&self, workspace_id: &str, new_name: &str) -> Result<AppSnapshot, String> {
        let mut persisted = guard(&self.persisted)?;
        find_workspace_mut(&mut persisted, workspace_id)?.name = name(new_name, "workspace")?;
        self.save(&persisted)?;
        drop(persisted);
        self.snapshot()
    }

    fn move_workspace(
        &self,
        area_id: &str,
        workspace_id: &str,
        direction: ReorderDirection,
    ) -> Result<AppSnapshot, String> {
        let mut persisted = guard(&self.persisted)?;
        move_matching(
            &mut find_area_mut(&mut persisted, area_id)?.workspaces,
            |workspace| workspace.id == workspace_id,
            direction,
        )?;
        self.save(&persisted)?;
        drop(persisted);
        self.snapshot()
    }

    fn duplicate_workspace(&self, workspace_id: &str) -> Result<AppSnapshot, String> {
        let mut persisted = guard(&self.persisted)?;
        for area in &mut persisted.areas {
            if let Some(index) = area
                .workspaces
                .iter()
                .position(|workspace| workspace.id == workspace_id)
            {
                let duplicate = area.workspaces[index].clone_for_duplicate();
                area.workspaces.insert(index + 1, duplicate);
                self.save(&persisted)?;
                drop(persisted);
                return self.snapshot();
            }
        }
        Err("Workspace não encontrado.".to_string())
    }

    fn remove_workspace(&self, workspace_id: &str) -> Result<AppSnapshot, String> {
        let panel_ids = {
            let persisted = guard(&self.persisted)?;
            find_workspace(&persisted, workspace_id)?
                .panel_ids()
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        };
        self.require_inactive(&panel_ids)?;
        let mut persisted = guard(&self.persisted)?;
        for area in &mut persisted.areas {
            let before = area.workspaces.len();
            area.workspaces
                .retain(|workspace| workspace.id != workspace_id);
            if before != area.workspaces.len() {
                self.save(&persisted)?;
                drop(persisted);
                return self.snapshot();
            }
        }
        Err("Workspace não encontrado.".to_string())
    }

    fn create_terminal(&self, input: CreateTerminalInput) -> Result<AppSnapshot, String> {
        let default_title = self.profile_name(&input.profile_id)?;
        let panel = TerminalPanel {
            id: new_id(),
            title: match input.title.as_deref() {
                Some(value) if !value.trim().is_empty() => name(value, "terminal")?,
                _ => default_title,
            },
            label: None,
            profile_id: input.profile_id,
        };
        let mut persisted = guard(&self.persisted)?;
        find_workspace_mut(&mut persisted, &input.workspace_id)?.add_panel(
            panel,
            input.target_panel_id.as_deref(),
            input.direction.unwrap_or(SplitDirection::Vertical),
        )?;
        self.save(&persisted)?;
        drop(persisted);
        self.snapshot()
    }

    fn remove_terminal(&self, terminal_id: &str) -> Result<AppSnapshot, String> {
        self.require_inactive(&[terminal_id.to_string()])?;
        let mut persisted = guard(&self.persisted)?;
        for area in &mut persisted.areas {
            for workspace in &mut area.workspaces {
                if workspace.remove_panel(terminal_id) {
                    self.save(&persisted)?;
                    drop(persisted);
                    return self.snapshot();
                }
            }
        }
        Err("Terminal não encontrado.".to_string())
    }

    fn rename_terminal(&self, terminal_id: &str, new_name: &str) -> Result<AppSnapshot, String> {
        let mut persisted = guard(&self.persisted)?;
        let panel = find_terminal_mut(&mut persisted, terminal_id)?;
        panel.title = name(new_name, "terminal")?;
        self.save(&persisted)?;
        drop(persisted);
        self.snapshot()
    }

    fn set_terminal_label(&self, input: SetTerminalLabelInput) -> Result<AppSnapshot, String> {
        let mut persisted = guard(&self.persisted)?;
        let panel = find_terminal_mut(&mut persisted, &input.terminal_id)?;
        panel.label = terminal_label(input.label)?;
        self.save(&persisted)?;
        drop(persisted);
        self.snapshot()
    }

    fn set_split_ratio(&self, input: SetSplitRatioInput) -> Result<AppSnapshot, String> {
        let ratio = split_ratio(input.ratio)?;
        let mut persisted = guard(&self.persisted)?;
        let layout = find_workspace_mut(&mut persisted, &input.workspace_id)?
            .layout
            .as_mut()
            .ok_or("O workspace não contém splits para ajustar.")?;
        if !layout.set_split_ratio(&input.split_id, ratio) {
            return Err("O divisor escolhido não pertence a este workspace.".to_string());
        }
        self.save(&persisted)?;
        drop(persisted);
        self.snapshot()
    }

    fn start_terminal(&self, app: AppHandle, terminal_id: &str) -> Result<AppSnapshot, String> {
        if guard(&self.sessions)?.contains_key(terminal_id) {
            return self.snapshot();
        }
        let (root, profile_id) = {
            let persisted = guard(&self.persisted)?;
            let (area, workspace) = find_terminal_workspace(&persisted, terminal_id)?;
            let panel = workspace
                .panel(terminal_id)
                .ok_or("Terminal não encontrado.")?;
            (PathBuf::from(&area.root_path), panel.profile_id.clone())
        };
        let spec = self.command_spec(&profile_id)?;
        let sessions = Arc::clone(&self.sessions);
        let output_id = terminal_id.to_string();
        let exit_id = terminal_id.to_string();
        let output_app = app.clone();
        let exit_app = app.clone();
        let session = start_pty(
            &spec.executable,
            &spec.arguments,
            &root,
            move |data| {
                let _ = output_app.emit(
                    "terminal-output",
                    TerminalOutput {
                        terminal_id: output_id.clone(),
                        data,
                    },
                );
            },
            move || {
                if let Ok(mut active) = sessions.lock() {
                    active.remove(&exit_id);
                }
                let _ = exit_app.emit(
                    "terminal-exit",
                    TerminalExit {
                        terminal_id: exit_id,
                    },
                );
            },
        )?;
        guard(&self.sessions)?.insert(terminal_id.to_string(), session);
        self.snapshot()
    }

    fn write_terminal(&self, terminal_id: &str, input: &str) -> Result<(), String> {
        if input.len() > 1_048_576 {
            return Err("A entrada do terminal excede 1 MiB.".to_string());
        }
        guard(&self.sessions)?
            .get_mut(terminal_id)
            .ok_or("O terminal não está ativo.")?
            .write(input)
    }

    fn resize_terminal(&self, input: ResizeTerminalInput) -> Result<(), String> {
        if !(2..=1000).contains(&input.columns) || !(2..=1000).contains(&input.rows) {
            return Err("O tamanho solicitado para o terminal é inválido.".to_string());
        }
        guard(&self.sessions)?
            .get(&input.terminal_id)
            .ok_or("O terminal não está ativo.")?
            .resize(input.columns, input.rows)
    }

    fn stop_terminal(&self, terminal_id: &str) -> Result<AppSnapshot, String> {
        let mut session = guard(&self.sessions)?
            .remove(terminal_id)
            .ok_or("O terminal não está ativo.")?;
        session.stop()?;
        self.snapshot()
    }

    fn create_custom_profile(&self, profile_name: &str) -> Result<AppSnapshot, String> {
        let mut persisted = guard(&self.persisted)?;
        persisted.custom_profiles.push(PersistedProfile {
            id: new_id(),
            name: name(profile_name, "perfil")?,
        });
        self.save(&persisted)?;
        drop(persisted);
        self.snapshot()
    }

    fn configure_custom_profile(
        &self,
        input: ConfigureProfileInput,
    ) -> Result<AppSnapshot, String> {
        let executable = executable(&input.executable)?;
        if input.arguments.len() > 32
            || input
                .arguments
                .iter()
                .any(|argument| argument.contains('\0') || argument.chars().count() > 512)
        {
            return Err("Os argumentos do perfil são inválidos.".to_string());
        }
        if !guard(&self.persisted)?
            .custom_profiles
            .iter()
            .any(|profile| profile.id == input.profile_id)
        {
            return Err("Perfil personalizado não encontrado.".to_string());
        }
        guard(&self.runtime_profiles)?.insert(
            input.profile_id,
            RuntimeProfile {
                executable,
                arguments: input.arguments,
            },
        );
        self.snapshot()
    }

    fn remove_custom_profile(&self, profile_id: &str) -> Result<AppSnapshot, String> {
        let mut persisted = guard(&self.persisted)?;
        if persisted
            .areas
            .iter()
            .flat_map(|area| &area.workspaces)
            .flat_map(|workspace| &workspace.panels)
            .any(|panel| panel.profile_id == profile_id)
        {
            return Err("Remova os painéis que usam este perfil antes de excluí-lo.".to_string());
        }
        let previous_len = persisted.custom_profiles.len();
        persisted
            .custom_profiles
            .retain(|profile| profile.id != profile_id);
        if previous_len == persisted.custom_profiles.len() {
            return Err("Perfil personalizado não encontrado.".to_string());
        }
        self.save(&persisted)?;
        drop(persisted);
        guard(&self.runtime_profiles)?.remove(profile_id);
        self.snapshot()
    }

    fn profile_name(&self, profile_id: &str) -> Result<String, String> {
        if let Some((_, label)) = builtin(profile_id) {
            return Ok(label.to_string());
        }
        guard(&self.persisted)?
            .custom_profiles
            .iter()
            .find(|profile| profile.id == profile_id)
            .map(|profile| profile.name.clone())
            .ok_or_else(|| "Perfil não encontrado.".to_string())
    }

    fn command_spec(&self, profile_id: &str) -> Result<CommandSpec, String> {
        if profile_id == SHELL {
            return Ok(CommandSpec {
                executable: env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string()),
                arguments: Vec::new(),
            });
        }
        if let Some((program, _)) = builtin(profile_id) {
            if !available(program) {
                return Err("A CLI deste perfil não está disponível no PATH local.".to_string());
            }
            return Ok(CommandSpec {
                executable: program.to_string(),
                arguments: Vec::new(),
            });
        }
        let configured = guard(&self.runtime_profiles)?
            .get(profile_id)
            .cloned()
            .ok_or("Configure o comando deste perfil para esta sessão.")?;
        if !available(&configured.executable) {
            return Err("O executável configurado não está disponível no PATH local.".to_string());
        }
        Ok(CommandSpec {
            executable: configured.executable,
            arguments: configured.arguments,
        })
    }

    fn require_inactive(&self, ids: &[String]) -> Result<(), String> {
        let active = guard(&self.sessions)?;
        if ids.iter().any(|id| active.contains_key(id)) {
            return Err("Há terminais ativos. Encerre-os após confirmar a ação.".to_string());
        }
        Ok(())
    }

    fn save(&self, persisted: &PersistedState) -> Result<(), String> {
        let directory = self
            .state_path
            .parent()
            .ok_or("O local da configuração é inválido.")?;
        fs::create_dir_all(directory)
            .map_err(|error| format!("Não foi possível criar a configuração local: {error}"))?;
        let temporary = self.state_path.with_extension("tmp");
        fs::write(
            &temporary,
            serde_json::to_vec_pretty(persisted)
                .map_err(|error| format!("Não foi possível serializar a configuração: {error}"))?,
        )
        .map_err(|error| format!("Não foi possível gravar a configuração: {error}"))?;
        protect(&temporary)?;
        fs::rename(&temporary, &self.state_path)
            .map_err(|error| format!("Não foi possível atualizar a configuração: {error}"))
    }
}

fn guard<T>(mutex: &Mutex<T>) -> Result<std::sync::MutexGuard<'_, T>, String> {
    mutex
        .lock()
        .map_err(|_| "O estado local está indisponível.".to_string())
}

fn name(value: &str, subject: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() || value.contains('\0') || value.chars().count() > 120 {
        return Err(format!("Informe um nome válido para {subject}."));
    }
    Ok(value.to_string())
}

fn terminal_label(value: Option<String>) -> Result<Option<String>, String> {
    let Some(value) = value else {
        return Ok(None);
    };
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    if value.contains('\0') || value.contains(['\r', '\n']) || value.chars().count() > 32 {
        return Err(
            "Informe uma etiqueta de terminal com até 32 caracteres em uma linha.".to_string(),
        );
    }
    Ok(Some(value.to_string()))
}

fn split_ratio(value: f64) -> Result<f64, String> {
    if !value.is_finite() || !(0.15..=0.85).contains(&value) {
        return Err("A proporção do divisor deve ficar entre 15% e 85%.".to_string());
    }
    Ok(value)
}

fn executable(value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty()
        || value.contains('\0')
        || value.chars().any(char::is_whitespace)
        || value.chars().count() > 4096
    {
        return Err(
            "Informe um executável local válido, sem espaços ou caracteres nulos.".to_string(),
        );
    }
    Ok(value.to_string())
}

fn directory(value: &str) -> Result<String, String> {
    let path = fs::canonicalize(value.trim())
        .map_err(|_| "A pasta selecionada não existe ou não está acessível.".to_string())?;
    if !path.is_dir() {
        return Err("A seleção precisa ser uma pasta local.".to_string());
    }
    path.into_os_string()
        .into_string()
        .map_err(|_| "O caminho da pasta não é suportado pela interface.".to_string())
}

fn find_area<'a>(state: &'a PersistedState, id: &str) -> Result<&'a Area, String> {
    state
        .areas
        .iter()
        .find(|area| area.id == id)
        .ok_or_else(|| "Área de trabalho não encontrada.".to_string())
}

fn find_area_mut<'a>(state: &'a mut PersistedState, id: &str) -> Result<&'a mut Area, String> {
    state
        .areas
        .iter_mut()
        .find(|area| area.id == id)
        .ok_or_else(|| "Área de trabalho não encontrada.".to_string())
}

fn find_workspace<'a>(state: &'a PersistedState, id: &str) -> Result<&'a Workspace, String> {
    state
        .areas
        .iter()
        .flat_map(|area| &area.workspaces)
        .find(|workspace| workspace.id == id)
        .ok_or_else(|| "Workspace não encontrado.".to_string())
}

fn find_workspace_mut<'a>(
    state: &'a mut PersistedState,
    id: &str,
) -> Result<&'a mut Workspace, String> {
    for area in &mut state.areas {
        if let Some(index) = area
            .workspaces
            .iter()
            .position(|workspace| workspace.id == id)
        {
            return Ok(&mut area.workspaces[index]);
        }
    }
    Err("Workspace não encontrado.".to_string())
}

fn find_terminal_workspace<'a>(
    state: &'a PersistedState,
    terminal_id: &str,
) -> Result<(&'a Area, &'a Workspace), String> {
    for area in &state.areas {
        if let Some(workspace) = area
            .workspaces
            .iter()
            .find(|workspace| workspace.panel(terminal_id).is_some())
        {
            return Ok((area, workspace));
        }
    }
    Err("Terminal não encontrado.".to_string())
}

fn find_terminal_mut<'a>(
    state: &'a mut PersistedState,
    terminal_id: &str,
) -> Result<&'a mut TerminalPanel, String> {
    for area in &mut state.areas {
        for workspace in &mut area.workspaces {
            if let Some(panel) = workspace.panel_mut(terminal_id) {
                return Ok(panel);
            }
        }
    }
    Err("Terminal não encontrado.".to_string())
}

fn move_matching<T, F>(
    items: &mut [T],
    matches: F,
    direction: ReorderDirection,
) -> Result<(), String>
where
    F: Fn(&T) -> bool,
{
    let position = items
        .iter()
        .position(matches)
        .ok_or_else(|| "Item não encontrado.".to_string())?;
    match direction {
        ReorderDirection::Earlier if position > 0 => items.swap(position, position - 1),
        ReorderDirection::Later if position + 1 < items.len() => items.swap(position, position + 1),
        _ => {}
    }
    Ok(())
}

fn builtin(id: &str) -> Option<(&'static str, &'static str)> {
    match id {
        SHELL => Some((SHELL, "Shell")),
        CODEX => Some((CODEX, "Codex")),
        CLAUDE => Some((CLAUDE, "Claude")),
        GEMINI => Some((GEMINI, "Gemini")),
        _ => None,
    }
}

fn profile_views(
    persisted: &PersistedState,
    runtime_profiles: &HashMap<String, RuntimeProfile>,
) -> Vec<ProfileView> {
    let mut views = vec![
        ProfileView {
            id: SHELL.into(),
            name: "Shell".into(),
            built_in: true,
            available: true,
            configured: true,
        },
        ProfileView {
            id: CODEX.into(),
            name: "Codex".into(),
            built_in: true,
            available: available(CODEX),
            configured: true,
        },
        ProfileView {
            id: CLAUDE.into(),
            name: "Claude".into(),
            built_in: true,
            available: available(CLAUDE),
            configured: true,
        },
        ProfileView {
            id: GEMINI.into(),
            name: "Gemini".into(),
            built_in: true,
            available: available(GEMINI),
            configured: true,
        },
    ];
    views.extend(persisted.custom_profiles.iter().map(|profile| {
        let runtime = runtime_profiles.get(&profile.id);
        ProfileView {
            id: profile.id.clone(),
            name: profile.name.clone(),
            built_in: false,
            available: runtime.is_some_and(|item| available(&item.executable)),
            configured: runtime.is_some(),
        }
    }));
    views
}

fn available(program: &str) -> bool {
    let path = Path::new(program);
    if path.components().count() > 1 {
        return path.is_file();
    }
    env::var_os("PATH")
        .is_some_and(|paths| env::split_paths(&paths).any(|folder| folder.join(program).is_file()))
}

#[cfg(unix)]
fn protect(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .map_err(|error| format!("Não foi possível proteger a configuração: {error}"))
}

#[cfg(not(unix))]
fn protect(_path: &Path) -> Result<(), String> {
    Ok(())
}

#[tauri::command]
fn get_snapshot(state: State<'_, AppState>) -> Result<AppSnapshot, String> {
    state.snapshot()
}
#[tauri::command]
fn create_area(state: State<'_, AppState>, input: CreateAreaInput) -> Result<AppSnapshot, String> {
    state.create_area(input)
}
#[tauri::command]
fn rename_area(
    state: State<'_, AppState>,
    area_id: String,
    name: String,
) -> Result<AppSnapshot, String> {
    state.rename_area(&area_id, &name)
}
#[tauri::command]
fn change_area_root(
    state: State<'_, AppState>,
    area_id: String,
    root_path: String,
) -> Result<AppSnapshot, String> {
    state.change_area_root(&area_id, &root_path)
}
#[tauri::command]
fn move_area(
    state: State<'_, AppState>,
    area_id: String,
    direction: ReorderDirection,
) -> Result<AppSnapshot, String> {
    state.move_area(&area_id, direction)
}
#[tauri::command]
fn remove_area(state: State<'_, AppState>, area_id: String) -> Result<AppSnapshot, String> {
    state.remove_area(&area_id)
}
#[tauri::command]
fn create_workspace(
    state: State<'_, AppState>,
    input: CreateWorkspaceInput,
) -> Result<AppSnapshot, String> {
    state.create_workspace(input)
}
#[tauri::command]
fn rename_workspace(
    state: State<'_, AppState>,
    workspace_id: String,
    name: String,
) -> Result<AppSnapshot, String> {
    state.rename_workspace(&workspace_id, &name)
}
#[tauri::command]
fn move_workspace(
    state: State<'_, AppState>,
    area_id: String,
    workspace_id: String,
    direction: ReorderDirection,
) -> Result<AppSnapshot, String> {
    state.move_workspace(&area_id, &workspace_id, direction)
}
#[tauri::command]
fn duplicate_workspace(
    state: State<'_, AppState>,
    workspace_id: String,
) -> Result<AppSnapshot, String> {
    state.duplicate_workspace(&workspace_id)
}
#[tauri::command]
fn remove_workspace(
    state: State<'_, AppState>,
    workspace_id: String,
) -> Result<AppSnapshot, String> {
    state.remove_workspace(&workspace_id)
}
#[tauri::command]
fn create_terminal(
    state: State<'_, AppState>,
    input: CreateTerminalInput,
) -> Result<AppSnapshot, String> {
    state.create_terminal(input)
}
#[tauri::command]
fn remove_terminal(state: State<'_, AppState>, terminal_id: String) -> Result<AppSnapshot, String> {
    state.remove_terminal(&terminal_id)
}
#[tauri::command]
fn rename_terminal(
    state: State<'_, AppState>,
    terminal_id: String,
    name: String,
) -> Result<AppSnapshot, String> {
    state.rename_terminal(&terminal_id, &name)
}
#[tauri::command]
fn set_terminal_label(
    state: State<'_, AppState>,
    input: SetTerminalLabelInput,
) -> Result<AppSnapshot, String> {
    state.set_terminal_label(input)
}
#[tauri::command]
fn set_split_ratio(
    state: State<'_, AppState>,
    input: SetSplitRatioInput,
) -> Result<AppSnapshot, String> {
    state.set_split_ratio(input)
}
#[tauri::command]
fn start_terminal(
    app: AppHandle,
    state: State<'_, AppState>,
    terminal_id: String,
) -> Result<AppSnapshot, String> {
    state.start_terminal(app, &terminal_id)
}
#[tauri::command]
fn write_terminal(
    state: State<'_, AppState>,
    terminal_id: String,
    input: String,
) -> Result<(), String> {
    state.write_terminal(&terminal_id, &input)
}
#[tauri::command]
fn resize_terminal(state: State<'_, AppState>, input: ResizeTerminalInput) -> Result<(), String> {
    state.resize_terminal(input)
}
#[tauri::command]
fn stop_terminal(state: State<'_, AppState>, terminal_id: String) -> Result<AppSnapshot, String> {
    state.stop_terminal(&terminal_id)
}
#[tauri::command]
fn create_custom_profile(state: State<'_, AppState>, name: String) -> Result<AppSnapshot, String> {
    state.create_custom_profile(&name)
}
#[tauri::command]
fn configure_custom_profile(
    state: State<'_, AppState>,
    input: ConfigureProfileInput,
) -> Result<AppSnapshot, String> {
    state.configure_custom_profile(input)
}
#[tauri::command]
fn remove_custom_profile(
    state: State<'_, AppState>,
    profile_id: String,
) -> Result<AppSnapshot, String> {
    state.remove_custom_profile(&profile_id)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[cfg(target_os = "macos")]
    let _ = fix_path_env::fix();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let directory = app
                .path()
                .app_config_dir()
                .map_err(|error| error.to_string())?;
            app.manage(AppState::load(directory.join("workspace.json"))?);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_snapshot,
            create_area,
            rename_area,
            change_area_root,
            move_area,
            remove_area,
            create_workspace,
            rename_workspace,
            move_workspace,
            duplicate_workspace,
            remove_workspace,
            create_terminal,
            remove_terminal,
            rename_terminal,
            set_terminal_label,
            set_split_ratio,
            start_terminal,
            write_terminal,
            resize_terminal,
            stop_terminal,
            create_custom_profile,
            configure_custom_profile,
            remove_custom_profile
        ])
        .run(tauri::generate_context!())
        .expect("Erro ao executar o KanashaTerminal");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state_path() -> PathBuf {
        std::env::temp_dir()
            .join(format!("kanasha-terminal-test-{}", new_id()))
            .join("workspace.json")
    }

    #[test]
    fn custom_profile_command_is_not_persisted() {
        let path = state_path();
        let state = AppState::load(path.clone()).expect("state");
        let snapshot = state.create_custom_profile("Privado").expect("profile");
        let profile_id = snapshot
            .profiles
            .iter()
            .find(|profile| profile.name == "Privado")
            .expect("profile id")
            .id
            .clone();
        state
            .configure_custom_profile(ConfigureProfileInput {
                profile_id,
                executable: "secret-command-never-persisted".into(),
                arguments: vec!["--secret-never-persisted".into()],
            })
            .expect("runtime configuration");
        let saved = fs::read_to_string(&path).expect("saved state");
        assert!(!saved.contains("secret-command-never-persisted"));
        assert!(!saved.contains("secret-never-persisted"));
        let _ = fs::remove_dir_all(path.parent().expect("parent"));
    }

    #[test]
    fn areas_and_workspaces_are_restored_but_processes_are_not() {
        let path = state_path();
        let root = std::env::temp_dir().to_string_lossy().into_owned();
        let state = AppState::load(path.clone()).expect("state");
        let created = state
            .create_area(CreateAreaInput {
                name: "Área persistida".into(),
                root_path: root,
            })
            .expect("area");
        let area_id = created.areas[0].id.clone();
        let workspace_id = created.areas[0].workspaces[0].id.clone();
        state
            .create_terminal(CreateTerminalInput {
                workspace_id,
                profile_id: SHELL.into(),
                title: None,
                target_panel_id: None,
                direction: None,
            })
            .expect("panel");
        drop(state);

        let restored = AppState::load(path.clone())
            .expect("restored state")
            .snapshot()
            .expect("snapshot");
        assert_eq!(restored.areas[0].id, area_id);
        assert_eq!(restored.areas[0].workspaces[0].panels.len(), 1);
        assert!(restored.active_terminal_ids.is_empty());
        let _ = fs::remove_dir_all(path.parent().expect("parent"));
    }

    #[test]
    fn panel_label_and_split_ratio_are_saved_as_safe_layout_metadata() {
        let path = state_path();
        let root = std::env::temp_dir().to_string_lossy().into_owned();
        let state = AppState::load(path.clone()).expect("state");
        let created = state
            .create_area(CreateAreaInput {
                name: "Área de teste".into(),
                root_path: root,
            })
            .expect("area");
        let workspace_id = created.areas[0].workspaces[0].id.clone();
        let first = state
            .create_terminal(CreateTerminalInput {
                workspace_id: workspace_id.clone(),
                profile_id: SHELL.into(),
                title: None,
                target_panel_id: None,
                direction: None,
            })
            .expect("first terminal");
        let first_id = first.areas[0].workspaces[0].panels[0].id.clone();
        let second = state
            .create_terminal(CreateTerminalInput {
                workspace_id: workspace_id.clone(),
                profile_id: SHELL.into(),
                title: None,
                target_panel_id: Some(first_id),
                direction: Some(SplitDirection::Vertical),
            })
            .expect("second terminal");
        let workspace = &second.areas[0].workspaces[0];
        let second_id = workspace.panels[1].id.clone();
        let split_id = match workspace.layout.as_ref().expect("split") {
            models::LayoutNode::Split { id, .. } => id.clone(),
            models::LayoutNode::Panel { .. } => panic!("expected split"),
        };

        state
            .set_split_ratio(SetSplitRatioInput {
                workspace_id,
                split_id,
                ratio: 0.7,
            })
            .expect("valid ratio");
        state
            .rename_terminal(&second_id, "Agente de revisão")
            .expect("rename terminal");
        let snapshot = state
            .set_terminal_label(SetTerminalLabelInput {
                terminal_id: second_id,
                label: Some("revisão".into()),
            })
            .expect("label terminal");
        let panel = &snapshot.areas[0].workspaces[0].panels[1];
        assert_eq!(panel.title, "Agente de revisão");
        assert_eq!(panel.label.as_deref(), Some("revisão"));
        assert!(state
            .set_split_ratio(SetSplitRatioInput {
                workspace_id: snapshot.areas[0].workspaces[0].id.clone(),
                split_id: "qualquer".into(),
                ratio: 0.9,
            })
            .is_err());

        let saved = fs::read_to_string(&path).expect("saved state");
        assert!(saved.contains("\"label\": \"revisão\""));
        assert!(saved.contains("\"ratio\": 0.7"));
        let _ = fs::remove_dir_all(path.parent().expect("parent"));
    }
}
