//! Read-only observations of a registered checkout, never a claim of authorship.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    io::Read,
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

const OUTPUT_LIMIT: usize = 8192;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GitSnapshot {
    pub observed_at: u64,
    pub checkout_path: String,
    pub repository_root: Option<String>,
    pub git_common_dir: Option<String>,
    pub branch: Option<String>,
    pub head: Option<String>,
    pub status: Vec<String>,
    pub tracked_diff_sha256: Option<String>,
    pub staged_diff_sha256: Option<String>,
    pub limitations: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GitComparison {
    pub commits: Vec<String>,
    pub committed_files: Vec<String>,
    pub limitations: Vec<String>,
}

struct Output {
    bytes: Vec<u8>,
    hash: String,
    truncated: bool,
    success: bool,
}

fn git(path: &str, args: &[&str], deadline: Instant) -> Result<Output, String> {
    if Instant::now() >= deadline {
        return Err("Prazo total de coleta Git excedido.".into());
    }
    let mut child = Command::new("git")
        .args([
            "--no-pager",
            "-c",
            "core.fsmonitor=false",
            "-c",
            "core.untrackedCache=false",
            "-C",
            path,
        ])
        .args(args)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .env_remove("GIT_COMMON_DIR")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "Git indisponível para este checkout.".to_string())?;
    let mut stdout = child.stdout.take().unwrap();
    let reader = thread::spawn(move || {
        let mut bytes = Vec::new();
        let mut hasher = Sha256::new();
        let mut size = 0usize;
        let mut buf = [0u8; 8192];
        loop {
            let n = stdout
                .read(&mut buf)
                .map_err(|_| "Falha ao ler a observação Git.".to_string())?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
            size = size.saturating_add(n);
            let keep = n.min(OUTPUT_LIMIT.saturating_sub(bytes.len()));
            bytes.extend_from_slice(&buf[..keep]);
        }
        Ok::<_, String>((
            bytes,
            format!("{:x}", hasher.finalize()),
            size > OUTPUT_LIMIT,
        ))
    });
    let start = Instant::now();
    let success = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status.success(),
            Ok(None) if start.elapsed() < Duration::from_secs(3) && Instant::now() < deadline => {
                thread::sleep(Duration::from_millis(10))
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                // Do not wait indefinitely for a descendant that inherited stdout.
                return Err("Coleta Git excedeu o prazo ou foi interrompida.".into());
            }
        }
    };
    while !reader.is_finished() {
        if Instant::now() >= deadline {
            return Err("Prazo de leitura da observação Git excedido.".into());
        }
        thread::sleep(Duration::from_millis(10));
    }
    let (bytes, hash, truncated) = reader
        .join()
        .map_err(|_| "Falha na coleta Git.".to_string())??;
    Ok(Output {
        bytes,
        hash,
        truncated,
        success,
    })
}

fn value(path: &str, args: &[&str], deadline: Instant) -> Result<String, String> {
    let out = git(path, args, deadline)?;
    if !out.success || out.truncated {
        return Err("Referência Git indisponível ou extensa demais.".into());
    }
    String::from_utf8(out.bytes)
        .map(|v| v.trim_end_matches('\n').to_owned())
        .map_err(|_| "Referência Git não é UTF-8.".into())
}

fn records(
    path: &str,
    args: &[&str],
    limitations: &mut Vec<String>,
    deadline: Instant,
) -> Vec<String> {
    match git(path, args, deadline) {
        Ok(out) if out.success => {
            if out.truncated {
                limitations.push("Lista Git parcial: limite de tamanho atingido.".into());
            }
            if std::str::from_utf8(&out.bytes).is_err() {
                limitations.push("Lista Git contém nomes que não são UTF-8.".into());
            }
            // Drop a partial final record when the output was bounded.
            let mut bytes = out.bytes.as_slice();
            if out.truncated {
                bytes = &bytes[..bytes
                    .iter()
                    .rposition(|b| *b == 0)
                    .map(|i| i + 1)
                    .unwrap_or(0)];
            }
            bytes
                .split(|b| *b == 0)
                .filter(|s| !s.is_empty())
                .map(|s| String::from_utf8_lossy(s).into_owned())
                .collect()
        }
        _ => {
            limitations.push("Não foi possível coletar a lista Git.".into());
            Vec::new()
        }
    }
}

pub fn capture(path: &str) -> GitSnapshot {
    capture_until(path, Instant::now() + Duration::from_secs(12))
}

pub fn capture_until(path: &str, deadline: Instant) -> GitSnapshot {
    let mut snapshot = GitSnapshot {
        observed_at: crate::unix_timestamp().unwrap_or(0),
        checkout_path: path.into(),
        repository_root: None,
        git_common_dir: None,
        branch: None,
        head: None,
        status: Vec::new(),
        tracked_diff_sha256: None,
        staged_diff_sha256: None,
        limitations: Vec::new(),
    };
    if !Path::new(path).is_absolute() {
        snapshot
            .limitations
            .push("Informe um checkout absoluto.".into());
        return snapshot;
    }
    let Ok(root) = value(path, &["rev-parse", "--show-toplevel"], deadline) else {
        snapshot.limitations.push("Checkout Git indisponível. Ausência de observação não significa ausência de alterações.".into());
        return snapshot;
    };
    snapshot.repository_root = Some(root);
    snapshot.git_common_dir = value(
        path,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
        deadline,
    )
    .ok();
    snapshot.branch = value(
        path,
        &["symbolic-ref", "--quiet", "--short", "HEAD"],
        deadline,
    )
    .ok();
    snapshot.head = value(path, &["rev-parse", "--verify", "HEAD"], deadline).ok();
    if snapshot.head.is_none() {
        snapshot
            .limitations
            .push("SHA indisponível; repositório pode ainda não ter commits.".into());
    }
    snapshot.status = records(
        path,
        &[
            "status",
            "--porcelain=v1",
            "--untracked-files=all",
            "--no-renames",
            "-z",
        ],
        &mut snapshot.limitations,
        deadline,
    );
    for staged in [false, true] {
        let mut args = vec![
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--binary",
            "--no-renames",
            "--ignore-submodules=all",
        ];
        if staged {
            args.push("--cached");
        }
        match git(path, &args, deadline) {
            Ok(out) if out.success => {
                if staged {
                    snapshot.staged_diff_sha256 = Some(out.hash);
                } else {
                    snapshot.tracked_diff_sha256 = Some(out.hash);
                }
            }
            _ => snapshot
                .limitations
                .push("Não foi possível calcular a impressão do diff rastreado.".into()),
        }
    }
    snapshot.limitations.push("Coleta sequencial do checkout; alterações concorrentes podem ocorrer. Não comprova autoria. Impressões de diff não cobrem conteúdo de arquivos não rastreados nem alterações internas de submódulos.".into());
    if value(path, &["rev-parse", "--verify", "HEAD"], deadline).ok() != snapshot.head {
        snapshot.limitations.push(
            "HEAD mudou durante a coleta; esta observação não representa uma versão estável."
                .into(),
        );
    }
    snapshot
}

#[cfg(test)]
pub fn compare(before: &GitSnapshot, after: &GitSnapshot) -> GitComparison {
    compare_until(before, after, Instant::now() + Duration::from_secs(12))
}

pub fn compare_until(
    before: &GitSnapshot,
    after: &GitSnapshot,
    deadline: Instant,
) -> GitComparison {
    let mut result = GitComparison {
        commits: Vec::new(),
        committed_files: Vec::new(),
        limitations: Vec::new(),
    };
    let (Some(initial), Some(final_sha)) = (&before.head, &after.head) else {
        result
            .limitations
            .push("Comparação indisponível sem SHA inicial e final.".into());
        return result;
    };
    if before.repository_root != after.repository_root
        || before.git_common_dir != after.git_common_dir
    {
        result
            .limitations
            .push("A identidade do checkout mudou entre as observações.".into());
        return result;
    }
    match git(
        &after.checkout_path,
        &["merge-base", "--is-ancestor", initial, final_sha],
        deadline,
    ) {
        Ok(out) if out.success => {}
        _ => {
            result.limitations.push("O SHA inicial não é ancestral confirmado do final. Não atribua estes commits à pessoa.".into());
            return result;
        }
    }
    let range = format!("{initial}..{final_sha}");
    match value(
        &after.checkout_path,
        &["rev-list", "--max-count=101", &range],
        deadline,
    ) {
        Ok(commits) => {
            result.commits = commits.lines().take(100).map(str::to_owned).collect();
            if commits.lines().count() > 100 {
                result
                    .limitations
                    .push("Lista de commits limitada aos primeiros 100.".into());
            }
        }
        Err(e) => result.limitations.push(e),
    }
    result.committed_files = records(
        &after.checkout_path,
        &[
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--name-only",
            "--no-renames",
            "-z",
            initial,
            final_sha,
            "--",
        ],
        &mut result.limitations,
        deadline,
    );
    result.limitations.push("Commits observados entre duas coletas; autoria e envio remoto ainda precisam de conferência.".into());
    result
}
