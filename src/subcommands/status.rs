use std::path::Path;

use crate::system::{derive_health_token, extract_env_val};

pub async fn run(orda_dir: &Path) -> Result<(), String> {
    let compose_file = orda_dir.join("docker-compose.yml");
    if !compose_file.exists() {
        return Err(format!("No installation found at {}", orda_dir.display()));
    }

    println!("Orda Status");
    println!("============");
    println!("Directory: {}", orda_dir.display());
    println!();

    let compose_str = compose_file.to_string_lossy();
    let output = tokio::process::Command::new("docker")
        .args(["compose", "-f", &compose_str, "ps"])
        .output()
        .await
        .map_err(|e| format!("Failed to check status: {e}"))?;

    println!("Containers:");
    println!("{}", String::from_utf8_lossy(&output.stdout));

    let env_path = orda_dir.join(".env");
    if let Ok(content) = tokio::fs::read_to_string(&env_path).await
        && let Some(license_key) = extract_env_val(&content, "LICENSE_KEY")
    {
        let health_token = derive_health_token(&license_key);

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(3))
            .danger_accept_invalid_certs(true)
            .build()
            .map_err(|e| format!("Failed to create HTTP client: {e}"))?;

        let mut healthy = false;
        for url in &["https://localhost/health", "http://localhost/health"] {
            if let Ok(resp) = client.get(*url).bearer_auth(&health_token).send().await
                && resp.status().is_success()
            {
                println!("Health: OK");
                healthy = true;
                break;
            }
        }
        if !healthy {
            println!("Health: UNREACHABLE");
        }
    }

    let data_dir = orda_dir.join("data");
    if data_dir.exists() {
        let data_str = data_dir.to_string_lossy();
        let output = tokio::process::Command::new("du")
            .args(["-sh", &data_str])
            .output()
            .await;

        if let Ok(out) = output {
            let text = String::from_utf8_lossy(&out.stdout);
            if let Some(size) = text.split_whitespace().next() {
                println!("Data size: {size}");
            }
        }
    }

    Ok(())
}
