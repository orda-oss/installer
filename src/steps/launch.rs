use std::sync::atomic::AtomicBool;

use tokio::sync::mpsc;

use super::{StepOutcome, is_cancelled};
use crate::{
    cleanup::{Artifact, CleanupRegistry},
    message::Message,
    model::{InstallContext, Step},
    system::run_cmd,
};

const HEALTH_TIMEOUT_SECS: u64 = 90;
const HEALTH_POLL_INTERVAL_SECS: u64 = 5;

pub async fn run(
    ctx: &InstallContext,
    tx: &mpsc::Sender<Message>,
    cleanup: &CleanupRegistry,
    cancelled: &AtomicBool,
) -> Result<StepOutcome, String> {
    if ctx.dry_run {
        let _ = tx
            .send(Message::StepLog(
                Step::Launch,
                "  [dry-run] would run: docker compose pull".to_string(),
            ))
            .await;
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        let _ = tx
            .send(Message::StepLog(
                Step::Launch,
                "  [dry-run] would run: docker compose up -d".to_string(),
            ))
            .await;
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        let _ = tx
            .send(Message::StepLog(
                Step::Launch,
                "Health check passed".to_string(),
            ))
            .await;

        return Ok(StepOutcome::Done);
    }

    let compose_file = ctx.lokal_dir.join("docker-compose.yml");
    let cf = compose_file.to_string_lossy().to_string();

    // Pull images
    let _ = tx
        .send(Message::StepLog(
            Step::Launch,
            "Pulling images...".to_string(),
        ))
        .await;

    let out = compose_cmd(tx, &cf, &["pull", "--quiet"]).await?;
    if !out.success {
        return Err("Failed to pull Docker images".to_string());
    }

    // Start services
    let _ = tx
        .send(Message::StepLog(
            Step::Launch,
            "Starting services...".to_string(),
        ))
        .await;

    let out = compose_cmd(tx, &cf, &["up", "-d"]).await?;
    if !out.success {
        return Err("Failed to start services".to_string());
    }

    cleanup.record(Artifact::DockerComposeUp(ctx.lokal_dir.clone()));

    // Health check
    let _ = tx
        .send(Message::StepLog(
            Step::Launch,
            "Waiting for server to become healthy...".to_string(),
        ))
        .await;

    let domain = ctx.domain.as_deref().ok_or("No domain for health check")?;
    let health_url = format!("https://{domain}/health");
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .danger_accept_invalid_certs(true)
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {e}"))?;

    let mut elapsed = 0u64;
    while elapsed < HEALTH_TIMEOUT_SECS {
        is_cancelled(cancelled)?;
        if !is_container_running(&cf).await {
            let logs = get_container_logs(&cf).await;
            let _ = tx
                .send(Message::StepLog(
                    Step::Launch,
                    "Container stopped unexpectedly:".to_string(),
                ))
                .await;
            for line in logs.lines().take(30) {
                let _ = tx
                    .send(Message::StepLog(Step::Launch, format!("  {line}")))
                    .await;
            }
            return Err("Server failed to start. Check the logs above.".to_string());
        }

        if let Ok(resp) = client
            .get(&health_url)
            .bearer_auth(&ctx.health_token)
            .send()
            .await
            && resp.status().is_success()
        {
            let _ = tx
                .send(Message::StepLog(
                    Step::Launch,
                    "Health check passed".to_string(),
                ))
                .await;

            return Ok(StepOutcome::Done);
        }

        elapsed += HEALTH_POLL_INTERVAL_SECS;
        let _ = tx
            .send(Message::StepLog(
                Step::Launch,
                format!("  Waiting... ({elapsed}s/{HEALTH_TIMEOUT_SECS}s)"),
            ))
            .await;
        tokio::time::sleep(std::time::Duration::from_secs(HEALTH_POLL_INTERVAL_SECS)).await;
    }

    let logs = get_container_logs(&cf).await;
    let _ = tx
        .send(Message::StepLog(Step::Launch, "Server logs:".to_string()))
        .await;
    for line in logs.lines().take(30) {
        let _ = tx
            .send(Message::StepLog(Step::Launch, format!("  {line}")))
            .await;
    }

    Err(format!(
        "Server did not become healthy within {HEALTH_TIMEOUT_SECS}s"
    ))
}

async fn compose_cmd(
    tx: &mpsc::Sender<Message>,
    compose_file: &str,
    sub_args: &[&str],
) -> Result<crate::system::CommandOutput, String> {
    let mut args = vec!["compose", "-f", compose_file];
    args.extend_from_slice(sub_args);
    run_cmd(Step::Launch, tx, false, "docker", &args).await
}

async fn is_container_running(compose_file: &str) -> bool {
    let output = tokio::process::Command::new("docker")
        .args(["compose", "-f", compose_file, "ps", "--status", "running"])
        .output()
        .await;

    if let Ok(out) = output {
        return String::from_utf8_lossy(&out.stdout).contains("alacahoyuk");
    }
    false
}

async fn get_container_logs(compose_file: &str) -> String {
    let output = tokio::process::Command::new("docker")
        .args([
            "compose",
            "-f",
            compose_file,
            "logs",
            "alacahoyuk",
            "--tail",
            "30",
        ])
        .output()
        .await;

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let stderr = String::from_utf8_lossy(&out.stderr);
            format!("{stdout}{stderr}")
        }
        Err(e) => format!("Failed to get logs: {e}"),
    }
}
