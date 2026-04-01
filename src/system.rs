use std::{path::Path, process::Stdio};

use sha2::{Digest, Sha256};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::Command,
    sync::mpsc,
};

use crate::{message::Message, model::Step};

pub struct CommandOutput {
    pub success: bool,
}

pub async fn run_cmd(
    step: Step,
    tx: &mpsc::Sender<Message>,
    dry_run: bool,
    program: &str,
    args: &[&str],
) -> Result<CommandOutput, String> {
    let cmd_str = format!("{} {}", program, args.join(" "));

    if dry_run {
        let _ = tx
            .send(Message::StepLog(
                step,
                format!("  [dry-run] would run: {cmd_str}"),
            ))
            .await;
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        return Ok(CommandOutput { success: true });
    }

    let _ = tx
        .send(Message::StepLog(step, format!("  $ {cmd_str}")))
        .await;

    let mut child = Command::new(program)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to run {program}: {e}"))?;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    // Read stdout and stderr concurrently to avoid deadlock
    let tx_out = tx.clone();
    let stdout_handle = tokio::spawn(async move {
        if let Some(stdout) = stdout {
            let mut reader = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                let _ = tx_out
                    .send(Message::StepLog(step, format!("  {line}")))
                    .await;
            }
        }
    });

    let tx_err = tx.clone();
    let stderr_handle = tokio::spawn(async move {
        if let Some(stderr) = stderr {
            let mut reader = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                let _ = tx_err
                    .send(Message::StepLog(step, format!("  {line}")))
                    .await;
            }
        }
    });

    let _ = stdout_handle.await;
    let _ = stderr_handle.await;

    let status = child
        .wait()
        .await
        .map_err(|e| format!("Failed to wait for {program}: {e}"))?;

    Ok(CommandOutput {
        success: status.success(),
    })
}

pub async fn run_sudo(
    step: Step,
    tx: &mpsc::Sender<Message>,
    dry_run: bool,
    use_sudo: bool,
    program: &str,
    args: &[&str],
) -> Result<CommandOutput, String> {
    if use_sudo {
        let mut sudo_args = vec![program];
        sudo_args.extend_from_slice(args);
        run_cmd(step, tx, dry_run, "sudo", &sudo_args).await
    } else {
        run_cmd(step, tx, dry_run, program, args).await
    }
}

pub async fn write_file(
    path: &Path,
    content: &str,
    dry_run: bool,
    use_sudo: bool,
) -> Result<(), String> {
    if dry_run {
        return Ok(());
    }

    let path_str = path.to_string_lossy();

    if use_sudo {
        let mut child = Command::new("sudo")
            .args(["tee", &path_str])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .spawn()
            .map_err(|e| format!("Failed to write {path_str}: {e}"))?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(content.as_bytes())
                .await
                .map_err(|e| format!("Failed to write {path_str}: {e}"))?;
        }

        let status = child
            .wait()
            .await
            .map_err(|e| format!("Failed to write {path_str}: {e}"))?;

        if !status.success() {
            return Err(format!("Failed to write {path_str}"));
        }
    } else {
        tokio::fs::write(path, content)
            .await
            .map_err(|e| format!("Failed to write {path_str}: {e}"))?;
    }
    Ok(())
}

pub fn command_exists(name: &str) -> bool {
    std::process::Command::new("which")
        .arg(name)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub fn command_output(program: &str, args: &[&str]) -> Option<String> {
    std::process::Command::new(program)
        .args(args)
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
            } else {
                None
            }
        })
}

pub fn derive_health_token(license_key: &str) -> String {
    hex_encode(Sha256::digest(license_key.as_bytes()))
}

pub fn validate_license_key(key: &str) -> bool {
    key.len() == 64 && key.chars().all(|c| c.is_ascii_hexdigit())
}

pub fn extract_env_val(content: &str, key: &str) -> Option<String> {
    let prefix = format!("{key}=");
    content
        .lines()
        .find_map(|line| line.strip_prefix(&prefix).map(|v| v.trim().to_string()))
}

fn hex_encode(bytes: impl AsRef<[u8]>) -> String {
    use std::fmt::Write;
    let bytes = bytes.as_ref();
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(s, "{b:02x}");
    }
    s
}
