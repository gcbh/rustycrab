use chrono::{DateTime, Duration, Utc};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use uuid::Uuid;

use crate::BioAuthError;

/// A time-limited challenge sent to the phone for signing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthChallenge {
    /// Unique challenge ID.
    pub id: Uuid,
    /// Random nonce the phone must sign (hex-encoded, 32 bytes).
    pub nonce: String,
    /// What credential is being requested (e.g. "anthropic_api_key").
    pub resource: String,
    /// Human-readable reason shown on the phone prompt.
    pub reason: String,
    /// Which device should respond.
    pub device_id: Uuid,
    /// When this challenge was created.
    pub created_at: DateTime<Utc>,
    /// When this challenge expires.
    pub expires_at: DateTime<Utc>,
    /// Current status.
    pub status: ChallengeStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChallengeStatus {
    /// Waiting for the phone to respond.
    Pending,
    /// User approved via biometric auth.
    Approved,
    /// User explicitly denied.
    Denied,
    /// Challenge expired without a response.
    Expired,
}

impl AuthChallenge {
    /// Create a new challenge with a 2-minute expiry.
    pub fn new(resource: String, reason: String, device_id: Uuid) -> Self {
        let mut rng = rand::thread_rng();
        let nonce_bytes: [u8; 32] = rng.gen();

        Self {
            id: Uuid::new_v4(),
            nonce: hex::encode(nonce_bytes),
            resource,
            reason,
            device_id,
            created_at: Utc::now(),
            expires_at: Utc::now() + Duration::seconds(120),
            status: ChallengeStatus::Pending,
        }
    }

    /// Check whether this challenge has expired.
    pub fn is_expired(&self) -> bool {
        Utc::now() > self.expires_at
    }
}

/// In-memory store for pending challenges.
///
/// Challenges are short-lived (2 minutes) so we keep them in memory
/// rather than persisting to sled. Expired challenges are cleaned up
/// on access.
pub struct ChallengeStore {
    pending: Mutex<HashMap<Uuid, AuthChallenge>>,
}

impl ChallengeStore {
    pub fn new() -> Self {
        Self {
            pending: Mutex::new(HashMap::new()),
        }
    }

    /// Insert a new pending challenge.
    pub fn insert(&self, challenge: AuthChallenge) {
        let mut map = self.pending.lock().unwrap();
        // Opportunistically clean up expired entries.
        map.retain(|_, c| !c.is_expired());
        map.insert(challenge.id, challenge);
    }

    /// Retrieve a challenge by ID (returns None if expired or missing).
    pub fn get(&self, id: Uuid) -> Option<AuthChallenge> {
        let map = self.pending.lock().unwrap();
        map.get(&id).and_then(|c| {
            if c.is_expired() {
                None
            } else {
                Some(c.clone())
            }
        })
    }

    /// Mark a challenge as approved or denied. Returns the updated challenge.
    pub fn resolve(
        &self,
        id: Uuid,
        status: ChallengeStatus,
    ) -> Result<AuthChallenge, BioAuthError> {
        let mut map = self.pending.lock().unwrap();
        let challenge = map.get_mut(&id).ok_or(BioAuthError::ChallengeNotFound)?;

        if challenge.is_expired() {
            challenge.status = ChallengeStatus::Expired;
            return Err(BioAuthError::ChallengeExpired);
        }

        challenge.status = status;
        Ok(challenge.clone())
    }

    /// Remove a resolved challenge.
    pub fn remove(&self, id: Uuid) {
        let mut map = self.pending.lock().unwrap();
        map.remove(&id);
    }
}

impl Default for ChallengeStore {
    fn default() -> Self {
        Self::new()
    }
}
