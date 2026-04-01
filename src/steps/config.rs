use tokio::sync::mpsc;

use super::StepOutcome;
use crate::{
    cleanup::{Artifact, CleanupRegistry},
    message::Message,
    model::{InstallContext, Step},
    system::{derive_health_token, extract_env_val, run_sudo, write_file},
    templates,
};

pub async fn run(
    ctx: &InstallContext,
    tx: &mpsc::Sender<Message>,
    cleanup: &CleanupRegistry,
) -> Result<StepOutcome, String> {
    let dir = &ctx.lokal_dir;

    let health_token = derive_health_token(&ctx.license_key);
    let lk_api_key = generate_lk_key();
    let lk_api_secret = generate_lk_secret();

    // .env: only create if new (preserve existing LK keys on resume)
    let env_path = dir.join(".env");
    let (final_health_token, final_lk_key, final_lk_secret) = if !ctx.dry_run && env_path.exists() {
        let _ = tx
            .send(Message::StepLog(
                Step::Configuration,
                "Using existing .env (preserving LiveKit keys)".to_string(),
            ))
            .await;
        let content = tokio::fs::read_to_string(&env_path)
            .await
            .unwrap_or_default();
        let ht = extract_env_val(&content, "HEALTH_TOKEN").unwrap_or(health_token.clone());
        let key = extract_env_val(&content, "LIVEKIT_API_KEY").unwrap_or(lk_api_key.clone());
        let secret =
            extract_env_val(&content, "LIVEKIT_API_SECRET").unwrap_or(lk_api_secret.clone());
        (ht, key, secret)
    } else {
        let content = templates::render_env(
            &ctx.license_key,
            &ctx.semerkant_url,
            &health_token,
            &lk_api_key,
            &lk_api_secret,
        );
        write_file(&env_path, &content, ctx.dry_run, ctx.use_sudo).await?;
        if !ctx.dry_run {
            cleanup.record(Artifact::FileCreated(env_path.clone()));
            let env_str = env_path.to_string_lossy();
            let _ = run_sudo(
                Step::Configuration,
                tx,
                false,
                ctx.use_sudo,
                "chmod",
                &["600", &env_str],
            )
            .await;
        }
        let _ = tx
            .send(Message::StepLog(
                Step::Configuration,
                "Wrote .env".to_string(),
            ))
            .await;
        (health_token, lk_api_key, lk_api_secret)
    };

    let _ = tx
        .send(Message::KeysGenerated {
            health_token: final_health_token,
            lk_api_key: final_lk_key.clone(),
            lk_api_secret: final_lk_secret.clone(),
        })
        .await;

    // livekit.yaml (only if new)
    let lk_path = dir.join("livekit.yaml");
    if ctx.dry_run || !lk_path.exists() {
        let content = templates::render_livekit_yaml(&final_lk_key, &final_lk_secret);
        write_file(&lk_path, &content, ctx.dry_run, ctx.use_sudo).await?;
        if !ctx.dry_run {
            cleanup.record(Artifact::FileCreated(lk_path));
        }
        let _ = tx
            .send(Message::StepLog(
                Step::Configuration,
                "Wrote livekit.yaml".to_string(),
            ))
            .await;
    }

    // Caddyfile (always regenerate)
    let caddy_path = dir.join("Caddyfile");
    let domain = ctx
        .domain
        .as_deref()
        .ok_or("No domain available for Caddyfile")?;
    let content = templates::render_caddyfile(domain);
    write_file(&caddy_path, &content, ctx.dry_run, ctx.use_sudo).await?;
    if !ctx.dry_run {
        cleanup.record(Artifact::FileCreated(caddy_path));
    }
    let _ = tx
        .send(Message::StepLog(
            Step::Configuration,
            "Wrote Caddyfile".to_string(),
        ))
        .await;

    // docker-compose.yml (always regenerate)
    let compose_path = dir.join("docker-compose.yml");
    let content = templates::render_docker_compose(&ctx.image, ctx.lokal_uid, ctx.lokal_gid);
    write_file(&compose_path, &content, ctx.dry_run, ctx.use_sudo).await?;
    if !ctx.dry_run {
        cleanup.record(Artifact::FileCreated(compose_path));
    }
    let _ = tx
        .send(Message::StepLog(
            Step::Configuration,
            "Wrote docker-compose.yml".to_string(),
        ))
        .await;

    if !ctx.dry_run {
        let lokal_dir_str = dir.to_string_lossy();
        let _ = run_sudo(
            Step::Configuration,
            tx,
            false,
            ctx.use_sudo,
            "chown",
            &["-R", "lokal:lokal", &lokal_dir_str],
        )
        .await;
    }

    let _ = tx
        .send(Message::StepLog(
            Step::Configuration,
            "Configuration complete".to_string(),
        ))
        .await;

    Ok(StepOutcome::Done)
}

const ALPHANUMERIC: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";

fn random_alphanumeric(len: usize) -> String {
    use rand::RngExt;
    let mut rng = rand::rng();
    (0..len)
        .map(|_| ALPHANUMERIC[rng.random_range(0..ALPHANUMERIC.len())] as char)
        .collect()
}

fn generate_lk_key() -> String {
    format!("API{}", random_alphanumeric(16))
}

fn generate_lk_secret() -> String {
    random_alphanumeric(48)
}
