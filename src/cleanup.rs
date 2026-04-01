use std::{
    path::PathBuf,
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use tokio::sync::mpsc;

use crate::{message::Message, model::Step};

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub enum Artifact {
    FileCreated(PathBuf),
    DirectoryCreated(PathBuf),
    SystemUserCreated(String),
    UfwEnabled,
    UfwRuleAdded(String),
    Fail2banJailCreated(PathBuf),
    SysctlConfCreated(PathBuf),
    DaemonJsonCreated(PathBuf),
    DaemonJsonModified(PathBuf, String),
    DockerComposeUp(PathBuf),
}

pub struct CleanupRegistry {
    artifacts: Mutex<Vec<Artifact>>,
    use_sudo: AtomicBool,
}

impl CleanupRegistry {
    pub fn new() -> Self {
        Self {
            artifacts: Mutex::new(Vec::new()),
            use_sudo: AtomicBool::new(false),
        }
    }

    pub fn set_use_sudo(&self, val: bool) {
        self.use_sudo.store(val, Ordering::Relaxed);
    }

    pub fn record(&self, artifact: Artifact) {
        if let Ok(mut artifacts) = self.artifacts.lock() {
            artifacts.push(artifact);
        }
    }

    pub async fn rollback(&self, tx: &mpsc::Sender<Message>) {
        let artifacts: Vec<Artifact> = {
            let mut guard = self.artifacts.lock().unwrap_or_else(|e| e.into_inner());
            guard.drain(..).rev().collect()
        };

        if artifacts.is_empty() {
            let _ = tx
                .send(Message::StepLog(
                    Step::Complete,
                    "Nothing to clean up.".to_string(),
                ))
                .await;
            let _ = tx.send(Message::CleanupComplete).await;
            return;
        }

        let _ = tx
            .send(Message::StepLog(
                Step::Complete,
                format!("Rolling back {} artifacts...", artifacts.len()),
            ))
            .await;

        for artifact in &artifacts {
            let desc = format!("  Reverting: {artifact:?}");
            let _ = tx.send(Message::StepLog(Step::Complete, desc)).await;

            match artifact {
                Artifact::DockerComposeUp(dir) => {
                    let cf = dir.join("docker-compose.yml");
                    self.run(
                        "docker",
                        &["compose", "-f", &cf.to_string_lossy(), "down", "--volumes"],
                    )
                    .await;
                }
                Artifact::FileCreated(path)
                | Artifact::SysctlConfCreated(path)
                | Artifact::DaemonJsonCreated(path)
                | Artifact::Fail2banJailCreated(path) => {
                    self.run("rm", &["-f", &path.to_string_lossy()]).await;
                }
                Artifact::DirectoryCreated(path) => {
                    self.run("rm", &["-rf", &path.to_string_lossy()]).await;
                }
                Artifact::SystemUserCreated(name) => {
                    self.run("userdel", &[name]).await;
                }
                Artifact::UfwEnabled => {
                    self.run("ufw", &["disable"]).await;
                }
                Artifact::UfwRuleAdded(rule) => {
                    self.run("ufw", &["delete", "allow", rule]).await;
                }
                Artifact::DaemonJsonModified(path, original) => {
                    // Use write_file which handles sudo
                    let _ = crate::system::write_file(
                        path,
                        original,
                        false,
                        self.use_sudo.load(Ordering::Relaxed),
                    )
                    .await;
                    self.run("systemctl", &["restart", "docker"]).await;
                }
            }
        }

        let _ = tx.send(Message::CleanupComplete).await;
    }

    async fn run(&self, program: &str, args: &[&str]) {
        let use_sudo = self.use_sudo.load(Ordering::Relaxed);
        if use_sudo {
            let mut sudo_args = vec![program];
            sudo_args.extend_from_slice(args);
            let _ = tokio::process::Command::new("sudo")
                .args(&sudo_args)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .await;
        } else {
            let _ = tokio::process::Command::new(program)
                .args(args)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .await;
        }
    }
}
