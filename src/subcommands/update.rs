use std::path::Path;

pub async fn run(lokal_dir: &Path) -> Result<(), String> {
    let compose_file = lokal_dir.join("docker-compose.yml");
    if !compose_file.exists() {
        return Err(format!(
            "No installation found at {}. Run 'lokal install' first.",
            lokal_dir.display()
        ));
    }

    println!("Updating Lokal at {}...", lokal_dir.display());

    let compose_str = compose_file.to_string_lossy();

    let status = tokio::process::Command::new("docker")
        .args(["compose", "-f", &compose_str, "pull"])
        .status()
        .await
        .map_err(|e| format!("Failed to pull images: {e}"))?;

    if !status.success() {
        return Err("Failed to pull images".to_string());
    }

    let status = tokio::process::Command::new("docker")
        .args(["compose", "-f", &compose_str, "up", "-d"])
        .status()
        .await
        .map_err(|e| format!("Failed to restart services: {e}"))?;

    if !status.success() {
        return Err("Failed to restart services".to_string());
    }

    println!("Update complete");
    Ok(())
}
