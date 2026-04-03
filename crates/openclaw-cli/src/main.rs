use std::net::SocketAddr;
use std::sync::Arc;

use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let data_dir = dirs::data_local_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("openclaw");
    std::fs::create_dir_all(&data_dir)?;

    let store = openclaw_store::Store::open(data_dir.join("db"))?;

    // Collect built-in tools; the first one is used as the placeholder trait object.
    let tools = openclaw_tools::builtin_tools();
    let first_tool: Arc<dyn openclaw_core::Tool> = tools.into_iter().next().unwrap();

    let state = openclaw_gateway::AppState::new(store, first_tool);
    let app = openclaw_gateway::router(state);

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    tracing::info!(%addr, "OpenClaw gateway listening");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
