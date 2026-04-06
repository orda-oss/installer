mod api;
mod app;
mod cleanup;
mod cli;
mod event;
mod message;
mod model;
mod steps;
mod subcommands;
mod system;
mod templates;
mod tui;
mod view;

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use app::{App, Effect};
use clap::Parser;
use cli::{Cli, Command, InstallArgs};
use message::Message;
use model::InstallContext;
use tokio::sync::mpsc;

#[tokio::main]
async fn main() {
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install TLS provider");

    let cli = Cli::parse();

    let result = match cli.command {
        Some(Command::Update(args)) => subcommands::update::run(&args.orda_dir).await,
        Some(Command::Uninstall(args)) => {
            subcommands::uninstall::run(&args.orda_dir, args.yes).await
        }
        Some(Command::Status(args)) => subcommands::status::run(&args.orda_dir).await,
        // No subcommand defaults to install
        Some(Command::Install(args)) => run_install(args).await,
        None => {
            // Re-parse as "orda install" to get clap's default values
            let cli = Cli::parse_from(["orda", "install"]);
            if let Some(Command::Install(args)) = cli.command {
                run_install(args).await
            } else {
                unreachable!()
            }
        }
    };

    if let Err(e) = result {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}

async fn run_install(args: InstallArgs) -> Result<(), String> {
    let mut context =
        InstallContext::new(args.dry_run, args.orda_dir, args.semerkant_url, args.image);

    if let Some(key) = args.license_key {
        context.license_key = key;
    }

    let (tx, mut rx) = mpsc::channel::<Message>(256);
    let cleanup = Arc::new(cleanup::CleanupRegistry::new());
    let cancelled = Arc::new(AtomicBool::new(false));
    let mut app = App::new(context, tx.clone());
    app.verbose = args.verbose;
    app.no_cleanup = args.no_cleanup;

    let mut terminal = tui::setup().map_err(|e| format!("Failed to setup terminal: {e}"))?;
    event::spawn(tx.clone());

    let _ = tx.send(Message::AdvanceStep).await;

    loop {
        let mut max_scroll = 0;
        terminal
            .draw(|f| {
                max_scroll = view::render(&app, f);
            })
            .map_err(|e| format!("Render error: {e}"))?;
        app.max_scroll = max_scroll;

        let msg = match rx.recv().await {
            Some(m) => m,
            None => break,
        };

        let mut msgs = vec![msg];
        while let Ok(m) = rx.try_recv() {
            msgs.push(m);
        }

        let mut effects = Vec::new();
        for msg in msgs {
            if let Message::HostDetected(_, use_sudo) = &msg {
                cleanup.set_use_sudo(*use_sudo);
            }
            let effect = app.update(msg);
            if !matches!(effect, Effect::None) {
                effects.push(effect);
            }
        }

        let mut should_break = false;
        for effect in effects {
            match effect {
                Effect::None => {}
                Effect::SpawnStep(step) => {
                    let tx_step = tx.clone();
                    let ctx = app.context.clone();
                    let cleanup_ref = Arc::clone(&cleanup);
                    let cancel = Arc::clone(&cancelled);
                    tokio::spawn(async move {
                        steps::execute(step, ctx, tx_step, &cleanup_ref, &cancel).await;
                    });
                }
                Effect::SpawnParallel(step_list) => {
                    for step in step_list {
                        let tx_step = tx.clone();
                        let ctx = app.context.clone();
                        let cleanup_ref = Arc::clone(&cleanup);
                        let cancel = Arc::clone(&cancelled);
                        tokio::spawn(async move {
                            steps::execute(step, ctx, tx_step, &cleanup_ref, &cancel).await;
                        });
                    }
                }
                Effect::RunCleanup => {
                    // Signal all running steps to stop
                    cancelled.store(true, Ordering::Relaxed);
                    let tx_cleanup = tx.clone();
                    let cleanup_ref = Arc::clone(&cleanup);
                    tokio::spawn(async move {
                        cleanup_ref.rollback(&tx_cleanup).await;
                    });
                }
                Effect::Quit => {
                    should_break = true;
                    break;
                }
            }
        }
        if should_break {
            break;
        }
    }

    tui::restore().map_err(|e| format!("Failed to restore terminal: {e}"))?;
    Ok(())
}
