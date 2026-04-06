use tokio::sync::mpsc;

use super::StepOutcome;
use crate::{
    message::Message,
    model::{InstallContext, Step},
    system::{extract_env_val, validate_license_key},
};

pub async fn run(ctx: &InstallContext, tx: &mpsc::Sender<Message>) -> Result<StepOutcome, String> {
    if !ctx.license_key.is_empty() {
        if !validate_license_key(&ctx.license_key) {
            return Err("Invalid license key".to_string());
        }
        let _ = tx
            .send(Message::StepLog(
                Step::License,
                "License key provided via flag".to_string(),
            ))
            .await;
        let _ = tx
            .send(Message::LicenseKeySet(ctx.license_key.clone()))
            .await;
        return Ok(StepOutcome::Done);
    }

    // Resuming from existing .env
    let env_path = ctx.orda_dir.join(".env");
    if !ctx.dry_run
        && env_path.exists()
        && let Ok(contents) = tokio::fs::read_to_string(&env_path).await
        && let Some(key) = extract_env_val(&contents, "LICENSE_KEY")
        && !key.is_empty()
        && validate_license_key(&key)
    {
        let _ = tx
            .send(Message::StepLog(
                Step::License,
                "Using license key from existing config".to_string(),
            ))
            .await;
        let _ = tx.send(Message::LicenseKeySet(key)).await;
        return Ok(StepOutcome::Done);
    }

    Ok(StepOutcome::WaitingForInput)
}
