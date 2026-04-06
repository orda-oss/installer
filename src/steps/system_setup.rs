use tokio::sync::mpsc;

use super::StepOutcome;
use crate::{
    cleanup::{Artifact, CleanupRegistry},
    message::Message,
    model::{InstallContext, Step},
    system::{command_output, run_sudo},
};

pub async fn run(
    ctx: &InstallContext,
    tx: &mpsc::Sender<Message>,
    cleanup: &CleanupRegistry,
) -> Result<StepOutcome, String> {
    let user_exists = if ctx.dry_run {
        false
    } else {
        command_output("id", &["-u", "orda"]).is_some()
    };

    if !user_exists {
        let _ = tx
            .send(Message::StepLog(
                Step::SystemSetup,
                "Creating service user...".to_string(),
            ))
            .await;

        let out = run_sudo(
            Step::SystemSetup,
            tx,
            ctx.dry_run,
            ctx.use_sudo,
            "useradd",
            &[
                "--system",
                "--no-create-home",
                "--shell",
                "/usr/sbin/nologin",
                "orda",
            ],
        )
        .await?;

        if !out.success && !ctx.dry_run {
            return Err("Failed to create service user 'orda'".to_string());
        }
        cleanup.record(Artifact::SystemUserCreated("orda".to_string()));
    }

    let (uid, gid) = if ctx.dry_run {
        (1000, 1000)
    } else {
        let uid: u32 = command_output("id", &["-u", "orda"])
            .and_then(|s| s.parse().ok())
            .ok_or("Failed to get orda UID")?;
        let gid: u32 = command_output("id", &["-g", "orda"])
            .and_then(|s| s.parse().ok())
            .ok_or("Failed to get orda GID")?;
        (uid, gid)
    };

    let _ = tx.send(Message::UidResolved(uid, gid)).await;

    let orda_dir = ctx.orda_dir.to_string_lossy();
    let tls_dir = ctx.orda_dir.join("tls");
    let data_dir = ctx.orda_dir.join("data");
    let dir_created = !ctx.orda_dir.exists() || ctx.dry_run;

    let _ = tx
        .send(Message::StepLog(
            Step::SystemSetup,
            format!("Creating directories at {orda_dir}"),
        ))
        .await;

    let tls_str = tls_dir.to_string_lossy();
    let data_str = data_dir.to_string_lossy();
    run_sudo(
        Step::SystemSetup,
        tx,
        ctx.dry_run,
        ctx.use_sudo,
        "mkdir",
        &["-p", &tls_str, &data_str],
    )
    .await?;

    if dir_created {
        cleanup.record(Artifact::DirectoryCreated(ctx.orda_dir.clone()));
    }

    run_sudo(
        Step::SystemSetup,
        tx,
        ctx.dry_run,
        ctx.use_sudo,
        "chown",
        &["-R", "orda:orda", &orda_dir],
    )
    .await?;

    let _ = tx
        .send(Message::StepLog(
            Step::SystemSetup,
            "System setup complete".to_string(),
        ))
        .await;

    Ok(StepOutcome::Done)
}
