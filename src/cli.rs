use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "lokal", version, about = "Lokal server installer")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand)]
pub enum Command {
    /// Install Lokal server (default if no subcommand given)
    Install(InstallArgs),
    /// Pull latest images and restart services
    Update(CommonArgs),
    /// Stop services and remove installation
    Uninstall(UninstallArgs),
    /// Show service status
    Status(CommonArgs),
}

#[derive(clap::Args, Clone)]
pub struct InstallArgs {
    /// Run the full TUI flow without touching the system (works on any OS)
    #[arg(long)]
    pub dry_run: bool,

    /// Show full logs for each step (no truncation)
    #[arg(long, short)]
    pub verbose: bool,

    /// License key (or set LICENSE_KEY env var)
    #[arg(long, env = "LICENSE_KEY")]
    pub license_key: Option<String>,

    /// Installation directory
    #[arg(long, default_value = "/opt/lokal")]
    pub lokal_dir: PathBuf,

    /// Central backend URL
    #[arg(long, default_value = "https://lokal.workspace.rustyneuron.net/hub/v1")]
    pub semerkant_url: String,

    /// Docker image for alacahoyuk
    #[arg(long, default_value = "ghcr.io/rwxdash/alacahoyuk:latest")]
    pub image: String,
}

#[derive(clap::Args, Clone)]
pub struct CommonArgs {
    /// Installation directory
    #[arg(long, default_value = "/opt/lokal")]
    pub lokal_dir: PathBuf,
}

#[derive(clap::Args, Clone)]
pub struct UninstallArgs {
    /// Installation directory
    #[arg(long, default_value = "/opt/lokal")]
    pub lokal_dir: PathBuf,

    /// Skip confirmation prompt
    #[arg(long)]
    pub yes: bool,
}
