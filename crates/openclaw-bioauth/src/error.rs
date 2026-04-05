use thiserror::Error;

#[derive(Debug, Error)]
pub enum BioAuthError {
    #[error("no devices registered — pair a phone first")]
    NoDevicesRegistered,

    #[error("challenge not found or expired")]
    ChallengeNotFound,

    #[error("challenge expired")]
    ChallengeExpired,

    #[error("invalid signature: {0}")]
    InvalidSignature(String),

    #[error("device not found: {0}")]
    DeviceNotFound(String),

    #[error("push notification failed: {0}")]
    PushFailed(String),

    #[error("authentication denied by user")]
    Denied,

    #[error("authentication timed out")]
    Timeout,

    #[error("storage error: {0}")]
    Storage(String),
}
