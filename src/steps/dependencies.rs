use std::sync::atomic::AtomicBool;

use tokio::sync::mpsc;

use super::{StepOutcome, is_cancelled};
use crate::{
    cleanup::{Artifact, CleanupRegistry},
    message::Message,
    model::{InstallContext, Step},
    system::{command_exists, run_cmd, run_sudo, write_file},
};

pub async fn run(
    ctx: &InstallContext,
    tx: &mpsc::Sender<Message>,
    cleanup: &CleanupRegistry,
    cancelled: &AtomicBool,
) -> Result<StepOutcome, String> {
    install_docker(ctx, tx).await?;
    is_cancelled(cancelled)?;
    tune_docker_daemon(ctx, tx, cleanup).await?;
    is_cancelled(cancelled)?;
    tune_sysctl(ctx, tx, cleanup).await?;
    is_cancelled(cancelled)?;
    install_jq(ctx, tx).await?;
    is_cancelled(cancelled)?;
    install_chrony(ctx, tx).await?;

    let _ = tx
        .send(Message::StepLog(
            Step::Dependencies,
            "Dependencies ready".to_string(),
        ))
        .await;
    Ok(StepOutcome::Done)
}

async fn install_docker(ctx: &InstallContext, tx: &mpsc::Sender<Message>) -> Result<(), String> {
    if !ctx.dry_run && command_exists("docker") {
        let _ = tx
            .send(Message::StepLog(
                Step::Dependencies,
                "Docker already installed".to_string(),
            ))
            .await;
        return Ok(());
    }

    let _ = tx
        .send(Message::StepLog(
            Step::Dependencies,
            "Installing Docker...".to_string(),
        ))
        .await;

    if ctx.dry_run {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        let _ = tx
            .send(Message::StepLog(
                Step::Dependencies,
                "Docker installed".to_string(),
            ))
            .await;
        return Ok(());
    }

    let _ = run_cmd(
        Step::Dependencies,
        tx,
        false,
        "sh",
        &["-c", "curl -fsSL https://get.docker.com | sh"],
    )
    .await;

    let _ = run_sudo(
        Step::Dependencies,
        tx,
        false,
        ctx.use_sudo,
        "systemctl",
        &["start", "docker"],
    )
    .await;

    if !command_exists("docker") {
        return Err("Docker installation failed. Install Docker manually and re-run.".to_string());
    }

    let _ = tx
        .send(Message::StepLog(
            Step::Dependencies,
            "Docker installed".to_string(),
        ))
        .await;
    Ok(())
}

async fn tune_docker_daemon(
    ctx: &InstallContext,
    tx: &mpsc::Sender<Message>,
    cleanup: &CleanupRegistry,
) -> Result<(), String> {
    let daemon_json = std::path::Path::new("/etc/docker/daemon.json");

    if ctx.dry_run {
        let _ = tx
            .send(Message::StepLog(
                Step::Dependencies,
                "  [dry-run] would tune Docker daemon".to_string(),
            ))
            .await;
        return Ok(());
    }

    let desired = serde_json::json!({
        "userland-proxy": false,
        "log-driver": "local",
        "log-opts": {"max-size": "10m", "max-file": "3"},
        "default-ulimits": {"nofile": {"Hard": 65536, "Soft": 65536}},
        "features": {"containerd-snapshotter": true}
    });

    if !daemon_json.exists() {
        let content = serde_json::to_string_pretty(&desired).unwrap();
        write_file(daemon_json, &content, false, ctx.use_sudo).await?;
        cleanup.record(Artifact::DaemonJsonCreated(daemon_json.to_path_buf()));
    } else {
        let existing = tokio::fs::read_to_string(daemon_json)
            .await
            .unwrap_or_default();
        if existing.contains("\"userland-proxy\"") {
            let _ = tx
                .send(Message::StepLog(
                    Step::Dependencies,
                    "Docker daemon already tuned".to_string(),
                ))
                .await;
            return Ok(());
        }

        let backup = existing.clone();
        let mut cfg: serde_json::Value =
            serde_json::from_str(&existing).unwrap_or(serde_json::json!({}));

        if let (Some(cfg_obj), Some(desired_obj)) = (cfg.as_object_mut(), desired.as_object()) {
            for (k, v) in desired_obj {
                if !cfg_obj.contains_key(k) {
                    cfg_obj.insert(k.clone(), v.clone());
                }
            }
            cfg_obj.insert("userland-proxy".to_string(), serde_json::json!(false));
        }

        let merged = serde_json::to_string_pretty(&cfg).unwrap();
        write_file(daemon_json, &merged, false, ctx.use_sudo).await?;
        cleanup.record(Artifact::DaemonJsonModified(
            daemon_json.to_path_buf(),
            backup,
        ));
    }

    let _ = run_sudo(
        Step::Dependencies,
        tx,
        false,
        ctx.use_sudo,
        "systemctl",
        &["restart", "docker"],
    )
    .await;
    let _ = tx
        .send(Message::StepLog(
            Step::Dependencies,
            "Docker daemon tuned".to_string(),
        ))
        .await;

    Ok(())
}

async fn tune_sysctl(
    ctx: &InstallContext,
    tx: &mpsc::Sender<Message>,
    cleanup: &CleanupRegistry,
) -> Result<(), String> {
    let sysctl_conf = std::path::Path::new("/etc/sysctl.d/99-lokal.conf");

    if ctx.dry_run || sysctl_conf.exists() {
        return Ok(());
    }

    let content = "\
# Lokal: larger UDP socket buffers for LiveKit media traffic
net.core.rmem_max=26214400
net.core.wmem_max=26214400
";

    write_file(sysctl_conf, content, false, ctx.use_sudo).await?;
    cleanup.record(Artifact::SysctlConfCreated(sysctl_conf.to_path_buf()));

    let _ = run_sudo(
        Step::Dependencies,
        tx,
        false,
        ctx.use_sudo,
        "sysctl",
        &["--system"],
    )
    .await;
    Ok(())
}

async fn install_jq(ctx: &InstallContext, tx: &mpsc::Sender<Message>) -> Result<(), String> {
    if !ctx.dry_run && command_exists("jq") {
        return Ok(());
    }

    let _ = tx
        .send(Message::StepLog(
            Step::Dependencies,
            "Installing jq...".to_string(),
        ))
        .await;

    if ctx.dry_run {
        return Ok(());
    }

    let _ = run_cmd(
        Step::Dependencies,
        tx,
        false,
        "sh",
        &[
            "-c",
            "apt-get update -qq && apt-get install -y -qq jq || yum install -y -q jq",
        ],
    )
    .await;

    if !command_exists("jq") {
        return Err("jq installation failed. Install it manually: apt install jq".to_string());
    }

    Ok(())
}

async fn install_chrony(ctx: &InstallContext, tx: &mpsc::Sender<Message>) -> Result<(), String> {
    if !ctx.dry_run && (command_exists("chronyd") || command_exists("ntpd")) {
        return Ok(());
    }

    let _ = tx
        .send(Message::StepLog(
            Step::Dependencies,
            "Installing chrony...".to_string(),
        ))
        .await;

    if ctx.dry_run {
        return Ok(());
    }

    // Best-effort
    let _ = run_cmd(
        Step::Dependencies,
        tx,
        false,
        "sh",
        &[
            "-c",
            "apt-get install -y -qq chrony || yum install -y -q chrony || true",
        ],
    )
    .await;

    let _ = run_sudo(
        Step::Dependencies,
        tx,
        false,
        ctx.use_sudo,
        "systemctl",
        &["enable", "chrony"],
    )
    .await;
    let _ = run_sudo(
        Step::Dependencies,
        tx,
        false,
        ctx.use_sudo,
        "systemctl",
        &["start", "chrony"],
    )
    .await;

    Ok(())
}
