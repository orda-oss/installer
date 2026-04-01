use tokio::sync::mpsc;

use super::StepOutcome;
use crate::{
    api,
    message::Message,
    model::{InstallContext, Step},
};

pub async fn run(ctx: &InstallContext, tx: &mpsc::Sender<Message>) -> Result<StepOutcome, String> {
    let _ = tx
        .send(Message::StepLog(
            Step::Register,
            "Registering with central server...".to_string(),
        ))
        .await;

    let client = reqwest::Client::new();
    let data = api::prepare(&client, &ctx.semerkant_url, &ctx.license_key, ctx.dry_run).await?;

    match data.domain {
        Some(domain) => {
            let _ = tx
                .send(Message::StepLog(
                    Step::Register,
                    "Server registered".to_string(),
                ))
                .await;
            let _ = tx.send(Message::DomainResolved(domain)).await;
        }
        None => {
            return Err("No domain provisioned. Contact support.".to_string());
        }
    }

    Ok(StepOutcome::Done)
}
