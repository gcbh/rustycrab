use serde::{Deserialize, Serialize};
use tracing;

use crate::challenge::AuthChallenge;
use crate::device::{PushProviderKind, RegisteredDevice};
use crate::BioAuthError;

/// Configuration for push notification delivery.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushConfig {
    /// FCM server key (for Android devices).
    pub fcm_server_key: Option<String>,
    /// APNs key ID + team ID + bundle ID (for iOS devices).
    pub apns_key_id: Option<String>,
    pub apns_team_id: Option<String>,
    pub apns_bundle_id: Option<String>,
    /// Path to APNs .p8 private key file.
    pub apns_key_path: Option<String>,
    /// Webhook URL template for webhook-type devices.
    /// Use `{device_token}` as placeholder.
    pub webhook_url_template: Option<String>,
}

impl Default for PushConfig {
    fn default() -> Self {
        Self {
            fcm_server_key: None,
            apns_key_id: None,
            apns_team_id: None,
            apns_bundle_id: None,
            apns_key_path: None,
            webhook_url_template: None,
        }
    }
}

/// Which push provider to use for delivery.
#[derive(Debug, Clone)]
pub enum PushProvider {
    Fcm { server_key: String },
    Apns {
        key_id: String,
        team_id: String,
        bundle_id: String,
        key_path: String,
    },
    Webhook { url_template: String },
}

/// Sends push notifications to registered phones.
pub struct PushNotifier {
    config: PushConfig,
    client: reqwest::Client,
}

impl PushNotifier {
    pub fn new(config: PushConfig) -> Self {
        Self {
            config,
            client: reqwest::Client::new(),
        }
    }

    /// Send an auth challenge to the device as a push notification.
    ///
    /// The notification payload contains:
    /// - challenge_id: UUID of the challenge
    /// - nonce: the random bytes the phone must sign
    /// - resource: what credential is being requested
    /// - reason: human-readable description for the prompt
    /// - respond_url: where the phone should POST its signed response
    pub async fn send_challenge(
        &self,
        device: &RegisteredDevice,
        challenge: &AuthChallenge,
        respond_url: &str,
    ) -> Result<(), BioAuthError> {
        let payload = serde_json::json!({
            "challenge_id": challenge.id,
            "nonce": challenge.nonce,
            "resource": challenge.resource,
            "reason": challenge.reason,
            "respond_url": respond_url,
            "expires_at": challenge.expires_at.to_rfc3339(),
        });

        match device.push_provider {
            PushProviderKind::Fcm => self.send_fcm(device, &payload).await,
            PushProviderKind::Apns => self.send_apns(device, &payload).await,
            PushProviderKind::Webhook => self.send_webhook(device, &payload).await,
        }
    }

    async fn send_fcm(
        &self,
        device: &RegisteredDevice,
        payload: &serde_json::Value,
    ) -> Result<(), BioAuthError> {
        let server_key = self
            .config
            .fcm_server_key
            .as_ref()
            .ok_or_else(|| BioAuthError::PushFailed("FCM server key not configured".into()))?;

        let body = serde_json::json!({
            "to": device.push_token,
            "priority": "high",
            "data": payload,
            "notification": {
                "title": "OpenClaw Auth Request",
                "body": &payload["reason"],
            }
        });

        let resp = self
            .client
            .post("https://fcm.googleapis.com/fcm/send")
            .header("Authorization", format!("key={server_key}"))
            .json(&body)
            .send()
            .await
            .map_err(|e| BioAuthError::PushFailed(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(BioAuthError::PushFailed(format!("FCM {status}: {text}")));
        }

        tracing::info!(device_id = %device.id, "FCM push sent");
        Ok(())
    }

    async fn send_apns(
        &self,
        device: &RegisteredDevice,
        payload: &serde_json::Value,
    ) -> Result<(), BioAuthError> {
        // APNs requires HTTP/2 with a JWT bearer token.
        // For a full implementation, use the `apple-apns` or `apns-h2` crate.
        // This sends via the REST API as a baseline.
        let bundle_id = self
            .config
            .apns_bundle_id
            .as_ref()
            .ok_or_else(|| BioAuthError::PushFailed("APNs bundle ID not configured".into()))?;

        let body = serde_json::json!({
            "aps": {
                "alert": {
                    "title": "OpenClaw Auth Request",
                    "body": &payload["reason"],
                },
                "sound": "default",
                "category": "BIOAUTH_CHALLENGE",
                "content-available": 1,
            },
            "openclaw": payload,
        });

        // Production APNs endpoint
        let url = format!(
            "https://api.push.apple.com/3/device/{}",
            device.push_token
        );

        let resp = self
            .client
            .post(&url)
            .header("apns-topic", bundle_id.as_str())
            .header("apns-push-type", "alert")
            .header("apns-priority", "10")
            .json(&body)
            .send()
            .await
            .map_err(|e| BioAuthError::PushFailed(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(BioAuthError::PushFailed(format!("APNs {status}: {text}")));
        }

        tracing::info!(device_id = %device.id, "APNs push sent");
        Ok(())
    }

    async fn send_webhook(
        &self,
        device: &RegisteredDevice,
        payload: &serde_json::Value,
    ) -> Result<(), BioAuthError> {
        let url_template = self
            .config
            .webhook_url_template
            .as_ref()
            .ok_or_else(|| BioAuthError::PushFailed("webhook URL not configured".into()))?;

        let url = url_template.replace("{device_token}", &device.push_token);

        let resp = self
            .client
            .post(&url)
            .json(payload)
            .send()
            .await
            .map_err(|e| BioAuthError::PushFailed(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(BioAuthError::PushFailed(format!("webhook {status}: {text}")));
        }

        tracing::info!(device_id = %device.id, "webhook push sent");
        Ok(())
    }
}
