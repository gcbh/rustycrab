use std::net::SocketAddr;
use std::sync::Arc;

use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    // --- Data directory ---
    let data_dir = dirs::data_local_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("openclaw");
    std::fs::create_dir_all(&data_dir)?;

    // --- Master key for credential encryption ---
    // In production: read from OS keychain. Fallback to env var.
    // NEVER store alongside the database.
    let master_key = std::env::var("OPENCLAW_MASTER_KEY")
        .unwrap_or_else(|_| {
            tracing::warn!(
                "OPENCLAW_MASTER_KEY not set — generating ephemeral key. \
                 Secrets will not survive restart."
            );
            openclaw_gateway::generate_token()
        })
        .into_bytes();

    let store = openclaw_store::Store::open(data_dir.join("db"), master_key)?;

    // --- Auth token ---
    // Read from env or generate and print.
    let auth_token = std::env::var("OPENCLAW_AUTH_TOKEN").unwrap_or_else(|_| {
        let token = openclaw_gateway::generate_token();
        tracing::info!("Generated auth token (set OPENCLAW_AUTH_TOKEN to persist):");
        // Print to stdout so it's capturable, not mixed into logs.
        println!("\n  OPENCLAW_AUTH_TOKEN={token}\n");
        token
    });

    // --- Tools ---
    let tools = openclaw_tools::builtin_tools();
    let first_tool: Arc<dyn openclaw_core::Tool> = tools.into_iter().next().unwrap();

    // --- Gateway with security middleware ---
    let state = openclaw_gateway::AppState::new(store, first_tool, auth_token);
    let app = openclaw_gateway::router(state);

    // Bind to loopback only — never 0.0.0.0.
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    tracing::info!(%addr, "OpenClaw gateway listening");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;

    Ok(())
}
