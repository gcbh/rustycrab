mod routes;
mod state;
mod webchat;

pub use state::AppState;

use axum::Router;

/// Build the main application router.
pub fn router(state: AppState) -> Router {
    Router::new()
        .merge(routes::api_routes())
        .merge(webchat::static_routes())
        .with_state(state)
}
