use tokio::sync::mpsc;

use super::StepOutcome;
use crate::{
    message::Message,
    model::{InstallContext, Step},
};

pub async fn run(ctx: &InstallContext, tx: &mpsc::Sender<Message>) -> Result<StepOutcome, String> {
    let ssh_port = detect_ssh_port(ctx).await;
    let _ = tx.send(Message::SshPortDetected(ssh_port)).await;

    if is_already_configured(ctx).await {
        let _ = tx
            .send(Message::StepLog(
                Step::Security,
                "Security already configured (ufw active with Lokal rules)".to_string(),
            ))
            .await;
        return Ok(StepOutcome::Done);
    }

    Ok(StepOutcome::WaitingForInput)
}

async fn detect_ssh_port(ctx: &InstallContext) -> u16 {
    if ctx.dry_run {
        return 22;
    }

    if let Ok(content) = tokio::fs::read_to_string("/etc/ssh/sshd_config").await {
        for line in content.lines() {
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix("Port ")
                && let Ok(port) = rest.trim().parse::<u16>()
            {
                return port;
            }
        }
    }
    22
}

async fn is_already_configured(ctx: &InstallContext) -> bool {
    if ctx.dry_run {
        return false;
    }

    if !crate::system::command_exists("ufw") {
        return false;
    }

    let output = tokio::process::Command::new("ufw")
        .arg("status")
        .output()
        .await;

    if let Ok(out) = output {
        let text = String::from_utf8_lossy(&out.stdout);
        return text.contains("Status: active") && text.contains("443/tcp");
    }

    false
}
