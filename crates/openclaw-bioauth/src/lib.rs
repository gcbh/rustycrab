//! Push-based biometric phone authentication for OpenClaw.
//!
//! When the agent needs access to protected credentials, this module
//! sends a push notification to the user's registered phone. The user
//! authenticates with Face ID / fingerprint, and the phone signs the
//! challenge with its device keypair and POSTs the response back.
//!
//! Flow:
//! 1. Agent requests a protected secret → BioAuthGate intercepts
//! 2. Gateway creates a challenge and sends push notification to phone
//! 3. Phone prompts biometric auth, signs the challenge
//! 4. Phone POSTs signed response to /api/bioauth/respond
//! 5. Gateway verifies signature, unlocks the credential

mod challenge;
mod device;
mod error;
mod gate;
mod push;

pub use challenge::{AuthChallenge, ChallengeStore};
pub use device::{PushProviderKind, RegisteredDevice, DeviceStore};
pub use error::BioAuthError;
pub use gate::{BioAuthGate, BioAuthResponse};
pub use push::{PushConfig, PushProvider, PushNotifier};
