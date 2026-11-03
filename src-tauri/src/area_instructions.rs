//! Area-owned instructions: preserve user text and load explicitly into managed sessions.
use crate::*;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::io::{Read, Write};

const DEFAULT: &str = include_str!("../../.kanasha/AGENTS.md");
const REFERENCE: &str = r#"## Integração com KanashaTerminal

<!-- kanasha-terminal:instructions -->
Leia [.kanasha/AGENTS.md](.kanasha/AGENTS.md) se ainda não recebido pela aplicação.
Use as tools disponíveis; sem integração, informe uma vez e continue o trabalho
independente dela. Este arquivo não configura a conexão MCP.
<!-- /kanasha-terminal:instructions -->
"#;
const LIMIT: usize = 64 * 1024;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Document {
    pub path: String,
    pub content: String,
    pub revision: String,
}

fn file_path(root: &Path) -> PathBuf {
    root.join(".kanasha/AGENTS.md")
}

pub fn revision(content: &str) -> String {
    format!("{:x}", Sha256::digest(content.as_bytes()))
}

fn validate(content: &str) -> Result<(), String> {
    if content.trim().is_empty() || content.len() > LIMIT {
        return Err("As instruções da área devem conter texto e ter no máximo 64 KiB.".into());
    }
    Ok(())
}

pub fn read(root: &Path) -> Result<Document, String> {
    let path = file_path(root);
    let result = (|| -> Result<String, String> {
        let mut content = String::new();
        let file = fs::File::open(&path).map_err(|e| e.to_string())?;
        if !file.metadata().map_err(|e| e.to_string())?.is_file() {
            return Err("O caminho deve ser um arquivo.".into());
        }
        file.take((LIMIT + 1) as u64)
            .read_to_string(&mut content)
            .map_err(|e| e.to_string())?;
        validate(&content)?;
        Ok(content)
    })();
    let content =
        result.map_err(|e| format!("Não foi possível carregar {}: {e}", path.display()))?;
    Ok(Document {
        path: path.to_string_lossy().into_owned(),
        revision: revision(&content),
        content,
    })
}

fn require_regular_path(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(meta) if !meta.file_type().is_file() => Err(format!(
            "{} deve ser um arquivo regular para edição; o caminho foi preservado.",
            path.display()
        )),
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!(
            "Não foi possível verificar {}: {e}",
            path.display()
        )),
    }
}

pub fn prepare(root: &Path) -> Result<Document, String> {
    let directory = root.join(".kanasha");
    fs::create_dir_all(&directory)
        .map_err(|e| format!("Não foi possível preparar {}: {e}", directory.display()))?;
    let path = file_path(root);
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
    {
        Ok(mut file) => file
            .write_all(DEFAULT.as_bytes())
            .map_err(|e| format!("Não foi possível criar {}: {e}", path.display()))?,
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(format!("Não foi possível criar {}: {e}", path.display())),
    }
    // Validate before modifying the root instructions. Never replace a customized template.
    let document = read(root)?;
    let root_agents = root.join("AGENTS.md");
    let existing = match fs::read_to_string(&root_agents) {
        Ok(content) => content,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => {
            return Err(format!(
                "Não foi possível ler {}: {e}",
                root_agents.display()
            ))
        }
    };
    if !existing.contains(".kanasha/AGENTS.md") {
        require_regular_path(&root_agents)?;
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&root_agents)
            .map_err(|e| {
                format!(
                    "Não foi possível referenciar as instruções em {}: {e}",
                    root_agents.display()
                )
            })?;
        let separator = if existing.is_empty() { "" } else { "\n\n" };
        file.write_all(format!("{separator}{REFERENCE}").as_bytes())
            .map_err(|e| format!("Não foi possível atualizar {}: {e}", root_agents.display()))?;
    }
    Ok(document)
}

fn save(root: &Path, content: &str, expected_revision: &str) -> Result<Document, String> {
    validate(content)?;
    let path = file_path(root);
    require_regular_path(&path)?;
    if read(root)?.revision != expected_revision {
        return Err("As instruções foram alteradas fora deste editor. Copie suas alterações e reabra o editor antes de salvar.".into());
    }
    let temporary = path.with_file_name(format!(".AGENTS-{}.tmp", new_id()));
    let result = (|| -> Result<(), String> {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|e| e.to_string())?;
        file.set_permissions(
            fs::metadata(&path)
                .map_err(|e| e.to_string())?
                .permissions(),
        )
        .map_err(|e| e.to_string())?;
        file.write_all(content.as_bytes())
            .map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        fs::rename(&temporary, &path).map_err(|e| e.to_string())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result.map_err(|e| format!("Não foi possível salvar {}: {e}", path.display()))?;
    read(root)
}

impl AppState {
    fn area_root(&self, area_id: &str) -> Result<PathBuf, String> {
        let persisted = guard(&self.persisted)?;
        let area = persisted
            .areas
            .iter()
            .find(|a| a.id == area_id)
            .ok_or("Área não encontrada.")?;
        Ok(PathBuf::from(&area.root_path))
    }
}

#[tauri::command]
pub fn prepare_area(state: tauri::State<'_, AppState>, area_id: String) -> Result<(), String> {
    let _gate = guard(&state.office_gate)?;
    prepare(&state.area_root(&area_id)?)?;
    Ok(())
}

#[tauri::command]
pub fn get_area_instructions(
    state: tauri::State<'_, AppState>,
    area_id: String,
) -> Result<Document, String> {
    let _gate = guard(&state.office_gate)?;
    read(&state.area_root(&area_id)?)
}

#[tauri::command]
pub fn set_area_instructions(
    state: tauri::State<'_, AppState>,
    app: AppHandle,
    area_id: String,
    content: String,
    expected_revision: String,
) -> Result<Document, String> {
    let document = {
        let _gate = guard(&state.office_gate)?;
        save(&state.area_root(&area_id)?, &content, &expected_revision)?
    };
    let _ = app.emit("task-state-changed", ());
    Ok(document)
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Folder(PathBuf);
    impl Folder {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!("kt-instructions-{}", new_id()));
            fs::create_dir_all(&root).unwrap();
            Self(root)
        }
    }
    impl Drop for Folder {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn prepares_once_preserving_both_user_files() {
        let folder = Folder::new();
        fs::write(
            folder.0.join("AGENTS.md"),
            "# Regras do projeto\nNão publicar.\n",
        )
        .unwrap();
        let first = prepare(&folder.0).unwrap();
        assert_eq!(first.content, DEFAULT);
        let root = fs::read_to_string(folder.0.join("AGENTS.md")).unwrap();
        assert!(root.starts_with("# Regras do projeto\nNão publicar.\n"));
        fs::write(file_path(&folder.0), "# Empresa\nOrientação personalizada.").unwrap();
        let next = prepare(&folder.0).unwrap();
        assert!(next.content.contains("personalizada"));
        assert_ne!(first.revision, next.revision);
        assert_eq!(
            fs::read_to_string(folder.0.join("AGENTS.md")).unwrap(),
            root
        );
    }

    #[test]
    fn custom_reference_is_not_duplicated() {
        let folder = Folder::new();
        let custom = "# Projeto\nConsulte [.kanasha/AGENTS.md](.kanasha/AGENTS.md).\n";
        fs::write(folder.0.join("AGENTS.md"), custom).unwrap();
        prepare(&folder.0).unwrap();
        assert_eq!(
            fs::read_to_string(folder.0.join("AGENTS.md")).unwrap(),
            custom
        );
    }

    #[test]
    fn save_detects_external_edits_and_rejects_invalid_content_without_overwrite() {
        let folder = Folder::new();
        let doc = prepare(&folder.0).unwrap();
        let saved = save(&folder.0, "# Novas instruções", &doc.revision).unwrap();
        assert_ne!(saved.revision, doc.revision);
        assert!(save(&folder.0, "# Edição antiga", &doc.revision).is_err());
        assert!(save(&folder.0, " ", &saved.revision).is_err());
        assert!(save(&folder.0, &"x".repeat(LIMIT + 1), &saved.revision).is_err());
        assert_eq!(read(&folder.0).unwrap().content, saved.content);
        fs::write(file_path(&folder.0), [0xff]).unwrap();
        assert!(prepare(&folder.0).is_err());
        assert_eq!(fs::read(file_path(&folder.0)).unwrap(), [0xff]);
    }
}
