use chrono::{DateTime, Utc};
use ed25519_dalek::VerifyingKey;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::BioAuthError;

/// A phone that has been paired for biometric authentication.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisteredDevice {
    /// Unique device ID (assigned at registration).
    pub id: Uuid,
    /// Human-readable label (e.g. "Alice's iPhone").
    pub label: String,
    /// Ed25519 public key (hex-encoded, 32 bytes).
    /// The phone holds the private key in its Secure Enclave / TEE.
    pub public_key_hex: String,
    /// Push notification token (FCM or APNs).
    pub push_token: String,
    /// Which push provider this device uses.
    pub push_provider: PushProviderKind,
    /// When this device was registered.
    pub registered_at: DateTime<Utc>,
    /// When this device last successfully authenticated.
    pub last_auth_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PushProviderKind {
    /// Firebase Cloud Messaging (Android).
    Fcm,
    /// Apple Push Notification service (iOS).
    Apns,
    /// Generic webhook — the phone app polls or uses a custom channel.
    Webhook,
}

impl RegisteredDevice {
    /// Parse the stored public key into a verifying key.
    pub fn verifying_key(&self) -> Result<VerifyingKey, BioAuthError> {
        let bytes = hex::decode(&self.public_key_hex)
            .map_err(|e| BioAuthError::InvalidSignature(format!("bad hex: {e}")))?;
        let key_bytes: [u8; 32] = bytes
            .try_into()
            .map_err(|_| BioAuthError::InvalidSignature("key must be 32 bytes".into()))?;
        VerifyingKey::from_bytes(&key_bytes)
            .map_err(|e| BioAuthError::InvalidSignature(format!("invalid ed25519 key: {e}")))
    }
}

/// In-memory store of registered devices (backed by serde JSON in sled).
#[derive(Clone)]
pub struct DeviceStore {
    tree: sled::Tree,
}

impl DeviceStore {
    pub fn new(tree: sled::Tree) -> Self {
        Self { tree }
    }

    /// Open a device store from a sled database path.
    pub fn open(path: impl AsRef<std::path::Path>) -> Result<Self, BioAuthError> {
        let db = sled::open(path).map_err(|e| BioAuthError::Storage(e.to_string()))?;
        let tree = db
            .open_tree("devices")
            .map_err(|e| BioAuthError::Storage(e.to_string()))?;
        Ok(Self { tree })
    }

    /// Register a new device. Returns the assigned device ID.
    pub fn register(&self, device: &RegisteredDevice) -> Result<(), BioAuthError> {
        let json = serde_json::to_vec(device)
            .map_err(|e| BioAuthError::Storage(e.to_string()))?;
        self.tree
            .insert(device.id.as_bytes(), json)
            .map_err(|e: sled::Error| BioAuthError::Storage(e.to_string()))?;
        Ok(())
    }

    /// Get a device by ID.
    pub fn get(&self, id: Uuid) -> Result<RegisteredDevice, BioAuthError> {
        let raw = self
            .tree
            .get(id.as_bytes())
            .map_err(|e: sled::Error| BioAuthError::Storage(e.to_string()))?
            .ok_or_else(|| BioAuthError::DeviceNotFound(id.to_string()))?;
        serde_json::from_slice(&raw).map_err(|e| BioAuthError::Storage(e.to_string()))
    }

    /// List all registered devices.
    pub fn list(&self) -> Result<Vec<RegisteredDevice>, BioAuthError> {
        let mut devices = Vec::new();
        for entry in self.tree.iter() {
            let (_, val) = entry.map_err(|e: sled::Error| BioAuthError::Storage(e.to_string()))?;
            let device: RegisteredDevice =
                serde_json::from_slice(&val).map_err(|e| BioAuthError::Storage(e.to_string()))?;
            devices.push(device);
        }
        Ok(devices)
    }

    /// Remove a device.
    pub fn remove(&self, id: Uuid) -> Result<(), BioAuthError> {
        self.tree
            .remove(id.as_bytes())
            .map_err(|e: sled::Error| BioAuthError::Storage(e.to_string()))?;
        Ok(())
    }

    /// Update the last-auth timestamp for a device.
    pub fn touch(&self, id: Uuid) -> Result<(), BioAuthError> {
        let mut device = self.get(id)?;
        device.last_auth_at = Some(Utc::now());
        self.register(&device)
    }
}
