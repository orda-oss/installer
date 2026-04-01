use tokio::sync::mpsc;

use super::StepOutcome;
use crate::{
    message::Message,
    model::{HostInfo, InstallContext, Step},
    system::{command_exists, command_output},
};

pub async fn run(ctx: &InstallContext, tx: &mpsc::Sender<Message>) -> Result<StepOutcome, String> {
    // Gather host info (works on any OS -- needed for the welcome screen)
    let os = std::env::consts::OS.to_string();
    let arch = match std::env::consts::ARCH {
        "x86_64" => "amd64".to_string(),
        "aarch64" | "arm64" => "arm64".to_string(),
        other => other.to_string(),
    };
    let hostname = command_output("hostname", &["-f"])
        .or_else(|| command_output("hostname", &[]))
        .unwrap_or_else(|| "unknown".to_string());
    let docker = command_exists("docker");
    let (public_ip, connectivity) = fetch_public_ip().await;

    let host = HostInfo {
        os,
        arch: arch.clone(),
        hostname,
        public_ip,
        docker,
        connectivity,
    };

    // Sudo check (only on Linux, skip in dry-run)
    let use_sudo = if ctx.dry_run || std::env::consts::OS != "linux" {
        false
    } else {
        let is_root = command_output("id", &["-u"])
            .map(|s| s.trim() == "0")
            .unwrap_or(false);

        if is_root {
            false
        } else if command_exists("sudo") {
            let _ = tx
                .send(Message::StepLog(
                    Step::Preflight,
                    "Caching sudo credentials...".to_string(),
                ))
                .await;

            let status = tokio::task::spawn_blocking(|| {
                std::process::Command::new("sudo").arg("true").status()
            })
            .await
            .map_err(|e| format!("sudo check failed: {e}"))?
            .map_err(|e| format!("sudo check failed: {e}"))?;

            if !status.success() {
                return Err("sudo authentication failed".to_string());
            }
            true
        } else {
            return Err("Not running as root and sudo is not available".to_string());
        }
    };

    let _ = tx.send(Message::HostDetected(host, use_sudo)).await;
    Ok(StepOutcome::Done)
}

async fn fetch_public_ip() -> (String, bool) {
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
    {
        Ok(c) => c,
        Err(_) => return ("--".to_string(), false),
    };

    match client.get("https://checkip.amazonaws.com").send().await {
        Ok(resp) if resp.status().is_success() => {
            let ip = resp.text().await.unwrap_or_default().trim().to_string();
            if ip.is_empty() {
                ("--".to_string(), true)
            } else {
                (ip, true)
            }
        }
        Ok(_) => ("--".to_string(), true),
        Err(_) => ("--".to_string(), false),
    }
}
