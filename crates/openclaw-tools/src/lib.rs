mod http_request;

pub use http_request::HttpRequestTool;

/// Collect all built-in tools into a Vec.
pub fn builtin_tools() -> Vec<std::sync::Arc<dyn openclaw_core::Tool>> {
    vec![std::sync::Arc::new(HttpRequestTool::new())]
}
