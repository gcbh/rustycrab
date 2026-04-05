use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::Utc;
use serde::Deserialize;
use uuid::Uuid;

use openclaw_bioauth::{BioAuthResponse, PushProviderKind, RegisteredDevice};

use crate::AppState;

pub fn bioauth_routes() -> Router<AppState> {
    Router::new()
        .route("/api/bioauth/devices", post(register_device))
        .route("/api/bioauth/devices", get(list_devices))
        .route("/api/bioauth/respond", post(handle_response))
        .route("/api/bioauth/status", get(auth_status))
        .route("/api/bioauth/request-secret", post(request_secret))
}

/// Request body for registering a new device.
#[derive(Deserialize)]
struct RegisterDeviceRequest {
    /// Human-readable label (e.g. "Alice's iPhone").
    label: String,
    /// Ed25519 public key (hex-encoded, 32 bytes).
    public_key_hex: String,
    /// Push notification token (FCM registration ID or APNs device token).
    push_token: String,
    /// Push provider: "fcm", "apns", or "webhook".
    push_provider: PushProviderKind,
}

/// Register a phone for biometric authentication.
///
/// The phone generates an Ed25519 keypair in its Secure Enclave / TEE
/// during setup, and sends the public key here along with its push token.
async fn register_device(
    State(state): State<AppState>,
    Json(body): Json<RegisterDeviceRequest>,
) -> Result<Json<RegisteredDevice>, StatusCode> {
    let gate = state.bioauth.as_ref().ok_or(StatusCode::NOT_FOUND)?;

    let device = RegisteredDevice {
        id: Uuid::new_v4(),
        label: body.label,
        public_key_hex: body.public_key_hex,
        push_token: body.push_token,
        push_provider: body.push_provider,
        registered_at: Utc::now(),
        last_auth_at: None,
    };

    gate.devices
        .register(&device)
        .map_err(|e| {
            tracing::error!("device registration failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    tracing::info!(device_id = %device.id, label = %device.label, "device registered for bioauth");
    Ok(Json(device))
}

/// List all registered devices.
async fn list_devices(
    State(state): State<AppState>,
) -> Result<Json<Vec<RegisteredDevice>>, StatusCode> {
    let gate = state.bioauth.as_ref().ok_or(StatusCode::NOT_FOUND)?;
    gate.devices
        .list()
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// Handle a signed response from the phone.
///
/// This endpoint is called by the phone app after the user authenticates
/// with Face ID / fingerprint. The phone signs the challenge nonce with
/// its private key and sends the signature here.
///
/// This endpoint bypasses bearer auth since the phone authenticates
/// cryptographically via the Ed25519 signature.
async fn handle_response(
    State(state): State<AppState>,
    Json(body): Json<BioAuthResponse>,
) -> Result<StatusCode, (StatusCode, String)> {
    let gate = state
        .bioauth
        .as_ref()
        .ok_or((StatusCode::NOT_FOUND, "bioauth not enabled".into()))?;

    gate.verify_response(&body).map_err(|e| {
        tracing::warn!("bioauth verification failed: {e}");
        (StatusCode::UNAUTHORIZED, e.to_string())
    })?;

    Ok(StatusCode::OK)
}

/// Request body for bioauth-gated secret access.
#[derive(Deserialize)]
struct RequestSecretBody {
    /// The secret name to access (e.g. "anthropic_api_key").
    secret_name: String,
    /// Human-readable reason shown on the phone (e.g. "Agent needs API key to call Claude").
    reason: String,
}

/// Request a secret that requires biometric phone approval.
///
/// This endpoint:
/// 1. Sends a push notification to the user's phone
/// 2. Waits for the user to authenticate with Face ID / fingerprint
/// 3. Returns the decrypted secret only after phone approval
///
/// If bioauth is not configured, or no devices are registered, returns 404.
/// The request blocks (up to 2 minutes) until the phone responds.
async fn request_secret(
    State(state): State<AppState>,
    Json(body): Json<RequestSecretBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let gate = state
        .bioauth
        .as_ref()
        .ok_or((StatusCode::NOT_FOUND, "bioauth not enabled".into()))?;

    // Request biometric auth from the phone.
    gate.request_auth(&body.secret_name, &body.reason)
        .await
        .map_err(|e| {
            let status = match &e {
                openclaw_bioauth::BioAuthError::Denied => StatusCode::FORBIDDEN,
                openclaw_bioauth::BioAuthError::Timeout => StatusCode::REQUEST_TIMEOUT,
                openclaw_bioauth::BioAuthError::NoDevicesRegistered => StatusCode::NOT_FOUND,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            (status, e.to_string())
        })?;

    // Phone approved — retrieve the secret.
    let value = state
        .store
        .secrets()
        .get(&body.secret_name)
        .map_err(|e| (StatusCode::NOT_FOUND, e.to_string()))?;

    tracing::info!(secret = %body.secret_name, "secret released after bioauth approval");

    Ok(Json(serde_json::json!({
        "secret_name": body.secret_name,
        "value": value,
    })))
}

/// Check whether bioauth is enabled and how many devices are registered.
async fn auth_status(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let gate = state.bioauth.as_ref().ok_or(StatusCode::NOT_FOUND)?;
    let device_count = gate
        .devices
        .list()
        .map(|d| d.len())
        .unwrap_or(0);

    Ok(Json(serde_json::json!({
        "enabled": true,
        "devices_registered": device_count,
    })))
}
