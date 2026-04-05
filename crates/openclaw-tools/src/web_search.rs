use async_trait::async_trait;
use openclaw_core::types::ToolSchema;
use openclaw_core::{Result, Tool};
use serde_json::{json, Value};

/// A built-in tool that searches the web and returns results with titles, URLs, and snippets.
pub struct WebSearchTool {
    client: reqwest::Client,
}

impl WebSearchTool {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }
}

impl Default for WebSearchTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for WebSearchTool {
    fn name(&self) -> &str {
        "web_search"
    }

    fn description(&self) -> &str {
        "Search the web and return a list of results with titles, URLs, and snippets."
    }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: self.name().to_string(),
            description: self.description().to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "The search query"
                    },
                    "num_results": {
                        "type": "integer",
                        "description": "Number of results to return (default: 5)"
                    }
                },
                "required": ["query"]
            }),
        }
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let query = args["query"]
            .as_str()
            .ok_or_else(|| openclaw_core::Error::ToolExecution("missing query".into()))?;
        let num_results = args["num_results"].as_u64().unwrap_or(5);

        let api_url = std::env::var("SEARCH_API_URL").ok();
        let api_key = std::env::var("SEARCH_API_KEY").ok();

        match (api_url, api_key) {
            (Some(base_url), Some(key)) => {
                let resp = self
                    .client
                    .get(&base_url)
                    .query(&[("q", query), ("num", &num_results.to_string())])
                    .header("Authorization", format!("Bearer {}", key))
                    .send()
                    .await
                    .map_err(|e| openclaw_core::Error::ToolExecution(e.to_string()))?;

                let body: Value = resp
                    .json()
                    .await
                    .map_err(|e| openclaw_core::Error::ToolExecution(e.to_string()))?;

                // Extract results - expect an array of objects with title, url, snippet
                let results = body["results"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
                    .map(|r| {
                        json!({
                            "title": r["title"].as_str().unwrap_or(""),
                            "url": r["url"].as_str().unwrap_or(""),
                            "snippet": r["snippet"].as_str().unwrap_or(""),
                        })
                    })
                    .collect::<Vec<_>>();

                Ok(json!({
                    "query": query,
                    "results": results,
                }))
            }
            _ => Err(openclaw_core::Error::ToolExecution(
                "Web search requires SEARCH_API_URL and SEARCH_API_KEY environment variables to be set.".into(),
            )),
        }
    }
}
