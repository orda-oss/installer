use tokio::sync::mpsc;

use super::StepOutcome;
use crate::{
    message::Message,
    model::{InstallContext, Step},
    system::write_file,
    templates,
};

pub async fn run(ctx: &InstallContext, tx: &mpsc::Sender<Message>) -> Result<StepOutcome, String> {
    let readme_path = ctx.orda_dir.join("README.txt");
    let content = templates::render_readme(&ctx.server_address, &ctx.orda_dir);
    let _ = write_file(&readme_path, &content, ctx.dry_run, ctx.use_sudo).await;

    let _ = tx
        .send(Message::StepLog(
            Step::Complete,
            "Server installed".to_string(),
        ))
        .await;

    Ok(StepOutcome::Done)
}
