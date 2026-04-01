use tokio::sync::mpsc;

use super::StepOutcome;
use crate::{
    api,
    cleanup::{Artifact, CleanupRegistry},
    message::Message,
    model::{InstallContext, Step},
    system::{run_sudo, write_file},
};

pub async fn run(
    ctx: &InstallContext,
    tx: &mpsc::Sender<Message>,
    cleanup: &CleanupRegistry,
) -> Result<StepOutcome, String> {
    let _ = tx
        .send(Message::StepLog(
            Step::Tls,
            "Fetching TLS certificate...".to_string(),
        ))
        .await;

    let client = reqwest::Client::new();
    let cert_data =
        api::fetch_certificate(&client, &ctx.semerkant_url, &ctx.license_key, ctx.dry_run).await?;

    let cert_path = ctx.lokal_dir.join("tls/cert.pem");
    let key_path = ctx.lokal_dir.join("tls/key.pem");

    write_file(
        &cert_path,
        &cert_data.certificate,
        ctx.dry_run,
        ctx.use_sudo,
    )
    .await?;
    write_file(&key_path, &cert_data.private_key, ctx.dry_run, ctx.use_sudo).await?;

    if !ctx.dry_run {
        cleanup.record(Artifact::FileCreated(cert_path));
        cleanup.record(Artifact::FileCreated(key_path.clone()));

        let key_str = key_path.to_string_lossy();
        let _ = run_sudo(
            Step::Tls,
            tx,
            false,
            ctx.use_sudo,
            "chmod",
            &["600", &key_str],
        )
        .await;
    }

    if let Some(expires) = &cert_data.expires_at {
        let _ = tx
            .send(Message::StepLog(
                Step::Tls,
                format!("TLS certificate ready (expires: {expires})"),
            ))
            .await;
    } else {
        let _ = tx
            .send(Message::StepLog(
                Step::Tls,
                "TLS certificate ready".to_string(),
            ))
            .await;
    }

    Ok(StepOutcome::Done)
}
