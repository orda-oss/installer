use std::path::Path;

use tokio::sync::mpsc;

use super::StepOutcome;
use crate::{
    message::Message,
    model::{InstallContext, SecurityChoice, Step},
    system::{command_exists, run_sudo, write_file},
};

pub async fn run(ctx: &InstallContext, tx: &mpsc::Sender<Message>) -> Result<StepOutcome, String> {
    let ssh_port = detect_ssh_port(ctx).await;
    let _ = tx.send(Message::SshPortDetected(ssh_port)).await;

    if is_already_configured(ctx).await {
        let _ = tx
            .send(Message::StepLog(
                Step::Security,
                "Security already configured (ufw active with Orda rules)".to_string(),
            ))
            .await;
        return Ok(StepOutcome::Done);
    }

    // First run: wait for user to pick install/skip
    if ctx.security_choice == SecurityChoice::NotAskedYet {
        return Ok(StepOutcome::WaitingForInput);
    }

    // Second run: apply the choice
    if ctx.security_choice == SecurityChoice::InstallFirewall {
        apply_firewall(ctx, tx, ssh_port).await?;
        apply_fail2ban(ctx, tx, ssh_port).await?;
        let _ = tx
            .send(Message::StepLog(
                Step::Security,
                "Security configured".to_string(),
            ))
            .await;
    } else {
        let _ = tx
            .send(Message::StepLog(
                Step::Security,
                "Skipped security setup".to_string(),
            ))
            .await;
    }

    Ok(StepOutcome::Done)
}

async fn apply_firewall(
    ctx: &InstallContext,
    tx: &mpsc::Sender<Message>,
    ssh_port: u16,
) -> Result<(), String> {
    // Install ufw if not present
    if !command_exists("ufw") {
        let _ = tx
            .send(Message::StepLog(
                Step::Security,
                "Installing ufw...".to_string(),
            ))
            .await;
        run_sudo(
            Step::Security,
            tx,
            ctx.dry_run,
            ctx.use_sudo,
            "apt-get",
            &["install", "-y", "-qq", "ufw"],
        )
        .await?;
    }

    if !ctx.dry_run && !command_exists("ufw") {
        return Err("Failed to install ufw".to_string());
    }

    let _ = tx
        .send(Message::StepLog(
            Step::Security,
            "Configuring firewall rules...".to_string(),
        ))
        .await;

    let ssh_rule = format!("{}/tcp", ssh_port);

    // Set defaults
    run_sudo(
        Step::Security,
        tx,
        ctx.dry_run,
        ctx.use_sudo,
        "ufw",
        &["default", "deny", "incoming"],
    )
    .await?;

    run_sudo(
        Step::Security,
        tx,
        ctx.dry_run,
        ctx.use_sudo,
        "ufw",
        &["default", "allow", "outgoing"],
    )
    .await?;

    // Allow required ports
    for rule in &[
        &ssh_rule as &str,
        "80/tcp",
        "443/tcp",
        "7881/tcp",
        "50000:51000/udp",
    ] {
        run_sudo(
            Step::Security,
            tx,
            ctx.dry_run,
            ctx.use_sudo,
            "ufw",
            &["allow", rule],
        )
        .await?;
    }

    // Enable
    run_sudo(
        Step::Security,
        tx,
        ctx.dry_run,
        ctx.use_sudo,
        "ufw",
        &["--force", "enable"],
    )
    .await?;

    let _ = tx
        .send(Message::StepLog(
            Step::Security,
            format!(
                "UFW enabled: ssh({}), 80, 443, 7881, 50000-51000/udp",
                ssh_port
            ),
        ))
        .await;

    Ok(())
}

async fn apply_fail2ban(
    ctx: &InstallContext,
    tx: &mpsc::Sender<Message>,
    ssh_port: u16,
) -> Result<(), String> {
    if !command_exists("fail2ban-client") {
        let _ = tx
            .send(Message::StepLog(
                Step::Security,
                "Installing fail2ban...".to_string(),
            ))
            .await;
        run_sudo(
            Step::Security,
            tx,
            ctx.dry_run,
            ctx.use_sudo,
            "apt-get",
            &["install", "-y", "-qq", "fail2ban"],
        )
        .await?;
    }

    if !ctx.dry_run && !command_exists("fail2ban-client") {
        // Non-fatal: log and continue
        let _ = tx
            .send(Message::StepLog(
                Step::Security,
                "fail2ban not available, skipping".to_string(),
            ))
            .await;
        return Ok(());
    }

    let jail_conf = format!(
        "[sshd]\nenabled = true\nport = {}\nmaxretry = 5\nfindtime = 3600\nbantime = 86400\n",
        ssh_port
    );

    let jail_path = Path::new("/etc/fail2ban/jail.d/orda.conf");
    write_file(jail_path, &jail_conf, ctx.dry_run, ctx.use_sudo).await?;

    run_sudo(
        Step::Security,
        tx,
        ctx.dry_run,
        ctx.use_sudo,
        "systemctl",
        &["enable", "fail2ban"],
    )
    .await?;

    run_sudo(
        Step::Security,
        tx,
        ctx.dry_run,
        ctx.use_sudo,
        "systemctl",
        &["restart", "fail2ban"],
    )
    .await?;

    let _ = tx
        .send(Message::StepLog(
            Step::Security,
            format!(
                "fail2ban: SSH jail active (port {}, 5 attempts/1h = 24h ban)",
                ssh_port
            ),
        ))
        .await;

    Ok(())
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

    if !command_exists("ufw") {
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
