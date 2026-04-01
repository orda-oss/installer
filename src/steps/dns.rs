use std::sync::atomic::AtomicBool;

use tokio::sync::mpsc;

use super::{StepOutcome, is_cancelled};
use crate::{
    message::Message,
    model::{InstallContext, Step},
};

const DNS_TIMEOUT_SECS: u64 = 180;
const DNS_POLL_INTERVAL_SECS: u64 = 5;

pub async fn run(
    ctx: &InstallContext,
    tx: &mpsc::Sender<Message>,
    cancelled: &AtomicBool,
) -> Result<StepOutcome, String> {
    let domain = match &ctx.domain {
        Some(d) => d.clone(),
        None => return Err("No domain available for network check".to_string()),
    };

    if ctx.dry_run {
        let _ = tx
            .send(Message::StepLog(
                Step::Network,
                "Waiting for network...".to_string(),
            ))
            .await;
        for i in 1..=3 {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            let _ = tx
                .send(Message::StepLog(
                    Step::Network,
                    format!("  Network check... ({i}s)"),
                ))
                .await;
        }

        return Ok(StepOutcome::Done);
    }

    let _ = tx
        .send(Message::StepLog(
            Step::Network,
            "Waiting for network...".to_string(),
        ))
        .await;

    let max_attempts = DNS_TIMEOUT_SECS / DNS_POLL_INTERVAL_SECS;
    let mut elapsed = 0u64;

    for _ in 0..max_attempts {
        is_cancelled(cancelled)?;
        if check_dns(&domain).await {
            let _ = tx
                .send(Message::StepLog(Step::Network, "Network ready".to_string()))
                .await;

            return Ok(StepOutcome::Done);
        }

        elapsed += DNS_POLL_INTERVAL_SECS;
        let _ = tx
            .send(Message::StepLog(
                Step::Network,
                format!("  Waiting for network... ({elapsed}s/{DNS_TIMEOUT_SECS}s)"),
            ))
            .await;
        tokio::time::sleep(std::time::Duration::from_secs(DNS_POLL_INTERVAL_SECS)).await;
    }

    Err(format!(
        "Network setup timed out after {DNS_TIMEOUT_SECS}s. Re-run the installer to retry."
    ))
}

async fn check_dns(domain: &str) -> bool {
    let output = tokio::process::Command::new("dig")
        .args(["+short", domain, "@1.1.1.1"])
        .output()
        .await;

    if let Ok(out) = output
        && out.status.success()
    {
        let result = String::from_utf8_lossy(&out.stdout);
        let trimmed = result.trim();
        if !trimmed.is_empty() && !trimmed.starts_with(";;") {
            return true;
        }
    }

    let output = tokio::process::Command::new("nslookup")
        .args([domain, "1.1.1.1"])
        .output()
        .await;

    if let Ok(out) = output {
        let result = String::from_utf8_lossy(&out.stdout);
        let lines: Vec<&str> = result.lines().collect();
        return lines
            .iter()
            .skip(2)
            .any(|line| line.starts_with("Address:"));
    }

    false
}
