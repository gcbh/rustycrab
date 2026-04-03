use openclaw_store::Store;
use std::sync::Arc;

/// Shared application state threaded through axum handlers.
#[derive(Clone)]
pub struct AppState {
    pub store: Store,
    pub tools: Arc<dyn openclaw_core::Tool>,
}

impl AppState {
    pub fn new(store: Store, tools: Arc<dyn openclaw_core::Tool>) -> Self {
        Self { store, tools }
    }
}
