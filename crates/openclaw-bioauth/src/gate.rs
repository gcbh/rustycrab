use ed25519_dalek::{Signature, Verifier};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Notify;
use tokio::time::{timeout, Duration};
use uuid::Uuid;

use crate::challenge::{AuthChallenge, ChallengeStatus, ChallengeStore};
use crate::device::DeviceStore;
use crate::push::PushNotifier;
use crate::BioAuthError;

/// The response payload sent by the phone after biometric auth.
#[derive(Debug, Serialize, Deserialize)]
pub struct BioAuthResponse {
    /// The challenge ID being responded to.
    pub challenge_id: Uuid,
    /// The device ID that signed this.
    pub device_id: Uuid,
    /// Ed25519 signature of the challenge nonce (hex-encoded).
    pub signature: String,
    /// Whether the user approved or denied.
    pub approved: bool,
}

/// Central coordinator for biometric authentication flows.
///
/// The gate manages the full lifecycle:
/// 1. Creates challenges when credentials are requested
/// 2. Sends push notifications to the user's phone
/// 3. Waits for the signed response
/// 4. Verifies the cryptographic proof and unlocks access
pub struct BioAuthGate {
    pub challenges: Arc<ChallengeStore>,
    pub devices: DeviceStore,
    push: PushNotifier,
    /// Notification channel to wake waiters when a challenge is resolved.
    notify: Arc<Notify>,
    /// Base URL for constructing the respond_url sent to phones.
    /// e.g. "https://your-server.example.com" or a tunnel URL.
    pub gateway_url: String,
}

impl BioAuthGate {
    pub fn new(
        devices: DeviceStore,
        push: PushNotifier,
        gateway_url: String,
    ) -> Self {
        Self {
            challenges: Arc::new(ChallengeStore::new()),
            devices,
            push,
            notify: Arc::new(Notify::new()),
            gateway_url,
        }
    }

    /// Request biometric authentication for accessing a protected resource.
    ///
    /// This sends a push to the first registered device, then blocks until
    /// the phone responds or the challenge expires (2 minute timeout).
    pub async fn request_auth(
        &self,
        resource: &str,
        reason: &str,
    ) -> Result<(), BioAuthError> {
        let devices = self.devices.list()?;
        let device = devices.first().ok_or(BioAuthError::NoDevicesRegistered)?;

        let challenge = AuthChallenge::new(
            resource.to_string(),
            reason.to_string(),
            device.id,
        );

        let challenge_id = challenge.id;
        let respond_url = format!("{}/api/bioauth/respond", self.gateway_url);

        // Store the pending challenge.
        self.challenges.insert(challenge.clone());

        // Send push notification to phone.
        self.push
            .send_challenge(device, &challenge, &respond_url)
            .await?;

        tracing::info!(
            challenge_id = %challenge_id,
            device = %device.label,
            resource = %resource,
            "bioauth challenge sent, waiting for response"
        );

        // Wait for the phone to respond (up to 2 minutes).
        let result = timeout(Duration::from_secs(120), async {
            loop {
                self.notify.notified().await;
                if let Some(c) = self.challenges.get(challenge_id) {
                    match c.status {
                        ChallengeStatus::Approved => return Ok(()),
                        ChallengeStatus::Denied => return Err(BioAuthError::Denied),
                        ChallengeStatus::Expired => return Err(BioAuthError::ChallengeExpired),
                        ChallengeStatus::Pending => continue,
                    }
                } else {
                    return Err(BioAuthError::ChallengeNotFound);
                }
            }
        })
        .await;

        // Clean up.
        self.challenges.remove(challenge_id);

        match result {
            Ok(inner) => inner,
            Err(_) => Err(BioAuthError::Timeout),
        }
    }

    /// Verify and process a response from the phone.
    ///
    /// Called by the gateway route handler when the phone POSTs back.
    pub fn verify_response(&self, response: &BioAuthResponse) -> Result<(), BioAuthError> {
        // Look up the challenge.
        let challenge = self
            .challenges
            .get(response.challenge_id)
            .ok_or(BioAuthError::ChallengeNotFound)?;

        if challenge.is_expired() {
            return Err(BioAuthError::ChallengeExpired);
        }

        if !response.approved {
            self.challenges
                .resolve(response.challenge_id, ChallengeStatus::Denied)?;
            self.notify.notify_waiters();
            return Err(BioAuthError::Denied);
        }

        // Verify the device is registered and matches the challenge.
        let device = self.devices.get(response.device_id)?;
        if device.id != challenge.device_id {
            return Err(BioAuthError::InvalidSignature(
                "device ID does not match challenge".into(),
            ));
        }

        // Verify the Ed25519 signature over the nonce.
        let verifying_key = device.verifying_key()?;
        let nonce_bytes = hex::decode(&challenge.nonce)
            .map_err(|e| BioAuthError::InvalidSignature(format!("nonce decode: {e}")))?;
        let sig_bytes = hex::decode(&response.signature)
            .map_err(|e| BioAuthError::InvalidSignature(format!("sig decode: {e}")))?;
        let sig_array: [u8; 64] = sig_bytes
            .try_into()
            .map_err(|_| BioAuthError::InvalidSignature("signature must be 64 bytes".into()))?;
        let signature = Signature::from_bytes(&sig_array);

        verifying_key
            .verify(&nonce_bytes, &signature)
            .map_err(|e| BioAuthError::InvalidSignature(format!("verification failed: {e}")))?;

        // Signature valid — mark approved.
        self.challenges
            .resolve(response.challenge_id, ChallengeStatus::Approved)?;
        self.notify.notify_waiters();

        // Update device last-auth timestamp.
        let _ = self.devices.touch(device.id);

        tracing::info!(
            challenge_id = %response.challenge_id,
            device_id = %device.id,
            "bioauth approved"
        );

        Ok(())
    }
}
