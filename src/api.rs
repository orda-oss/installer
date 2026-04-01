use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct PrepareData {
    pub domain: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CertificateData {
    pub certificate: String,
    pub private_key: String,
    pub expires_at: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ApiResponse<T> {
    data: T,
}

#[derive(Debug, Deserialize)]
struct ApiErrorResponse {
    data: Option<ApiErrorData>,
}

#[derive(Debug, Deserialize)]
struct ApiErrorData {
    messages: Option<Vec<ApiErrorMessage>>,
}

#[derive(Debug, Deserialize)]
struct ApiErrorMessage {
    errors: Vec<String>,
}

pub async fn prepare(
    client: &reqwest::Client,
    semerkant_url: &str,
    license_key: &str,
    dry_run: bool,
) -> Result<PrepareData, String> {
    if dry_run {
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        return Ok(PrepareData {
            domain: Some("dry-run.example.com".to_string()),
        });
    }

    let url = format!("{semerkant_url}/provision/prepare");
    let res = client
        .post(&url)
        .bearer_auth(license_key)
        .header("Content-Type", "application/json")
        .send()
        .await
        .map_err(|e| format!("Failed to reach server: {e}"))?;

    let status = res.status();
    if !status.is_success() {
        let text = res.text().await.unwrap_or_default();
        let msg = parse_api_error(&text).unwrap_or_else(|| match status.as_u16() {
            401 => "Invalid license key".to_string(),
            403 => "IP address mismatch. Contact support if you moved servers.".to_string(),
            _ => format!("Registration failed (HTTP {status})"),
        });
        return Err(msg);
    }

    let resp: ApiResponse<PrepareData> = res
        .json()
        .await
        .map_err(|e| format!("Invalid response: {e}"))?;

    Ok(resp.data)
}

pub async fn fetch_certificate(
    client: &reqwest::Client,
    semerkant_url: &str,
    license_key: &str,
    dry_run: bool,
) -> Result<CertificateData, String> {
    if dry_run {
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        return Ok(CertificateData {
            certificate: "-----BEGIN CERTIFICATE-----\ndry-run\n-----END CERTIFICATE-----"
                .to_string(),
            private_key: "-----BEGIN PRIVATE KEY-----\ndry-run\n-----END PRIVATE KEY-----"
                .to_string(),
            expires_at: Some("2099-12-31T23:59:59Z".to_string()),
        });
    }

    let url = format!("{semerkant_url}/provision/certificate");
    let res = client
        .get(&url)
        .bearer_auth(license_key)
        .timeout(std::time::Duration::from_secs(90))
        .send()
        .await
        .map_err(|e| format!("Failed to fetch certificate: {e}"))?;

    let status = res.status();
    if !status.is_success() {
        let text = res.text().await.unwrap_or_default();
        let msg = parse_api_error(&text).unwrap_or_else(|| match status.as_u16() {
            404 => {
                "No certificate available yet. It may still be issuing -- try again in a minute."
                    .to_string()
            }
            503 => "Certificate issuance in progress. Try again in a minute.".to_string(),
            _ => format!("Certificate fetch failed (HTTP {status})"),
        });
        return Err(msg);
    }

    let resp: ApiResponse<CertificateData> = res
        .json()
        .await
        .map_err(|e| format!("Invalid certificate response: {e}"))?;

    Ok(resp.data)
}

fn parse_api_error(body: &str) -> Option<String> {
    let resp: ApiErrorResponse = serde_json::from_str(body).ok()?;
    let messages = resp.data?.messages?;
    let errors: Vec<&str> = messages
        .iter()
        .flat_map(|m| m.errors.iter().map(|s| s.as_str()))
        .collect();
    if errors.is_empty() {
        None
    } else {
        Some(errors.join("; "))
    }
}
