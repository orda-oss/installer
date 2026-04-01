mod complete;
mod config;
mod dependencies;
mod dns;
mod launch;
mod license;
mod preflight;
mod register;
mod security;
mod system_setup;
mod tls;

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use tokio::sync::mpsc;

use crate::{
    cleanup::CleanupRegistry,
    message::Message,
    model::{InstallContext, Step},
};

pub enum StepOutcome {
    Done,
    WaitingForInput,
}

pub fn is_cancelled(cancelled: &AtomicBool) -> Result<(), String> {
    if cancelled.load(Ordering::Relaxed) {
        Err("Cancelled".to_string())
    } else {
        Ok(())
    }
}

pub async fn execute(
    step: Step,
    ctx: InstallContext,
    tx: mpsc::Sender<Message>,
    cleanup: &CleanupRegistry,
    cancelled: &Arc<AtomicBool>,
) {
    let _ = tx.send(Message::StepStarted(step)).await;

    // Check cancellation before starting
    if cancelled.load(Ordering::Relaxed) {
        return;
    }

    let result = match step {
        Step::Preflight => preflight::run(&ctx, &tx).await,
        Step::License => license::run(&ctx, &tx).await,
        Step::Register => register::run(&ctx, &tx).await,
        Step::Dependencies => dependencies::run(&ctx, &tx, cleanup, cancelled).await,
        Step::Network => dns::run(&ctx, &tx, cancelled).await,
        Step::SystemSetup => system_setup::run(&ctx, &tx, cleanup).await,
        Step::Tls => tls::run(&ctx, &tx, cleanup).await,
        Step::Security => security::run(&ctx, &tx).await,
        Step::Configuration => config::run(&ctx, &tx, cleanup).await,
        Step::Launch => launch::run(&ctx, &tx, cleanup, cancelled).await,
        Step::Complete => complete::run(&ctx, &tx).await,
    };

    // Don't send completion if cancelled
    if cancelled.load(Ordering::Relaxed) {
        return;
    }

    match result {
        Ok(StepOutcome::Done) => {
            let _ = tx.send(Message::StepCompleted(step)).await;
        }
        Ok(StepOutcome::WaitingForInput) => {
            let _ = tx.send(Message::WaitingForInput(step)).await;
        }
        Err(e) if e == "Cancelled" => {}
        Err(e) => {
            let _ = tx.send(Message::StepFailed(step, e)).await;
        }
    }
}
