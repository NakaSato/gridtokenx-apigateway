use anyhow::Result;
use hmac::{Hmac, Mac};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::time::Duration;
use tracing::{error, info, warn};

/// Webhook event payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookPayload {
    pub event_id: String,
    pub event_type: String,
    pub timestamp: String,
    pub data: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
}

/// Webhook Dispatcher Service
#[derive(Clone)]
pub struct WebhookService {
    client: Client,
    webhook_url: Option<String>,
    webhook_secret: Option<String>,
}

impl WebhookService {
    pub fn new(webhook_url: Option<String>, webhook_secret: Option<String>) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap_or_default();

        Self {
            client,
            webhook_url,
            webhook_secret,
        }
    }

    /// Send webhook notification
    pub async fn send_webhook(&self, event_type: &str, data: serde_json::Value) -> Result<()> {
        let url = match &self.webhook_url {
            Some(url) => url,
            None => return Ok(()), // Webhook disabled
        };

        let event_id = uuid::Uuid::new_v4().to_string();
        let timestamp = chrono::Utc::now().to_rfc3339();

        let mut payload = WebhookPayload {
            event_id,
            event_type: event_type.to_string(),
            timestamp,
            data,
            signature: None,
        };

        // Sign payload if secret is provided
        if let Some(secret) = &self.webhook_secret {
            let signature = self.sign_payload(&payload, secret)?;
            payload.signature = Some(signature);
        }

        // Send request with retries
        let mut attempts = 0;
        let max_retries = 3;
        let mut backoff = Duration::from_millis(500);

        loop {
            attempts += 1;
            match self.client.post(url).json(&payload).send().await {
                Ok(res) => {
                    if res.status().is_success() {
                        info!("Webhook sent successfully for event {}", payload.event_type);
                        return Ok(());
                    } else {
                        warn!(
                            "Webhook failed with status {}: {}",
                            res.status(),
                            res.text().await.unwrap_or_default()
                        );
                    }
                }
                Err(e) => {
                    warn!("Webhook request failed: {}", e);
                }
            }

            if attempts >= max_retries {
                error!("Failed to send webhook after {} attempts", max_retries);
                break;
            }

            tokio::time::sleep(backoff).await;
            backoff *= 2;
        }

        Ok(())
    }

    /// Sign payload using HMAC-SHA256
    fn sign_payload(&self, payload: &WebhookPayload, secret: &str) -> Result<String> {
        // Create a canonical string representation for signing
        // We'll sign the event_id + timestamp + event_type
        // In a real app, you might want to sign the full JSON body
        let data_to_sign = format!(
            "{}.{}.{}",
            payload.event_id, payload.timestamp, payload.event_type
        );

        type HmacSha256 = Hmac<Sha256>;
        let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
            .map_err(|e| anyhow::anyhow!("Invalid HMAC secret: {}", e))?;

        mac.update(data_to_sign.as_bytes());
        let result = mac.finalize();
        let code_bytes = result.into_bytes();

        Ok(hex::encode(code_bytes))
    }
}
