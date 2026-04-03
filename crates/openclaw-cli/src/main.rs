use std::net::SocketAddr;
use std::sync::Arc;

use openclaw_core::model::ModelProvider;
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
    let auth_token = std::env::var("OPENCLAW_AUTH_TOKEN").unwrap_or_else(|_| {
        let token = openclaw_gateway::generate_token();
        tracing::info!("Generated auth token (set OPENCLAW_AUTH_TOKEN to persist):");
        println!("\n  OPENCLAW_AUTH_TOKEN={token}\n");
        token
    });

    // --- Model provider ---
    let provider: Arc<dyn ModelProvider> = match std::env::var("OPENCLAW_PROVIDER")
        .unwrap_or_else(|_| "anthropic".to_string())
        .to_lowercase()
        .as_str()
    {
        "ollama" => {
            let model = std::env::var("OLLAMA_MODEL").unwrap_or_else(|_| "qwen3:32b".to_string());
            let base_url = std::env::var("OLLAMA_BASE_URL")
                .unwrap_or_else(|_| "http://localhost:11434".to_string());
            tracing::info!(%model, %base_url, "using Ollama provider");
            let p = openclaw_providers::OllamaProvider::new(model).with_base_url(base_url);
            Arc::new(p)
        }
        _ => {
            let api_key = std::env::var("ANTHROPIC_API_KEY").unwrap_or_else(|_| {
                // Try reading from the encrypted store.
                if let Ok(key) = store.secrets().get("anthropic_api_key") {
                    return key;
                }
                tracing::error!(
                    "ANTHROPIC_API_KEY not set. Set it or store via the secrets API."
                );
                String::new()
            });
            let model = std::env::var("ANTHROPIC_MODEL")
                .unwrap_or_else(|_| "claude-sonnet-4-20250514".to_string());
            tracing::info!(%model, "using Anthropic provider");
            let p = openclaw_providers::AnthropicProvider::new(api_key).with_model(model);
            Arc::new(p)
        }
    };

    // --- Tools ---
    let tools = openclaw_tools::builtin_tools();
    let first_tool: Arc<dyn openclaw_core::Tool> = tools.into_iter().next().unwrap();

    // --- Log provider status ---
    tracing::info!(provider = provider.name(), "model provider configured");

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
