use std::path::Path;

use crate::system::command_output;

pub async fn run(orda_dir: &Path, skip_confirm: bool) -> Result<(), String> {
    if !orda_dir.exists() {
        return Err(format!("No installation found at {}", orda_dir.display()));
    }

    if !skip_confirm {
        eprintln!(
            "This will stop all services and remove {}",
            orda_dir.display()
        );
        eprint!("Type 'yes' to confirm: ");

        let mut input = String::new();
        std::io::stdin()
            .read_line(&mut input)
            .map_err(|e| format!("Failed to read input: {e}"))?;

        if input.trim() != "yes" {
            println!("Cancelled.");
            return Ok(());
        }
    }

    let compose_file = orda_dir.join("docker-compose.yml");
    if compose_file.exists() {
        let compose_str = compose_file.to_string_lossy();
        println!("Stopping services...");
        let _ = tokio::process::Command::new("docker")
            .args(["compose", "-f", &compose_str, "down", "--volumes"])
            .status()
            .await;
    }

    let is_root = command_output("id", &["-u"])
        .map(|s| s == "0")
        .unwrap_or(false);

    let dir_str = orda_dir.to_string_lossy();
    println!("Removing {dir_str}...");

    let status = if is_root {
        tokio::process::Command::new("rm")
            .args(["-rf", &dir_str])
            .status()
            .await
    } else {
        tokio::process::Command::new("sudo")
            .args(["rm", "-rf", &dir_str])
            .status()
            .await
    };

    if let Ok(s) = status
        && !s.success()
    {
        return Err(format!("Failed to remove {dir_str}"));
    }

    println!("Uninstalled. Firewall rules and installed packages were left in place.");
    Ok(())
}
