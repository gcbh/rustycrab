use async_trait::async_trait;
use openclaw_core::types::ToolSchema;
use openclaw_core::{Result, Tool};
use serde_json::{json, Value};

/// A built-in tool that fetches a URL and returns the page content as clean text.
pub struct WebFetchTool {
    client: reqwest::Client,
}

impl WebFetchTool {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }
}

impl Default for WebFetchTool {
    fn default() -> Self {
        Self::new()
    }
}

/// Strip HTML to plain text: remove script/style blocks, tags, decode entities, collapse whitespace.
fn html_to_text(html: &str) -> String {
    let mut text = html.to_string();

    // Remove <script>...</script> blocks (case-insensitive, including content)
    while let Some(start) = text.to_lowercase().find("<script") {
        if let Some(end) = text.to_lowercase()[start..].find("</script>") {
            text.replace_range(start..start + end + "</script>".len(), " ");
        } else {
            break;
        }
    }

    // Remove <style>...</style> blocks
    while let Some(start) = text.to_lowercase().find("<style") {
        if let Some(end) = text.to_lowercase()[start..].find("</style>") {
            text.replace_range(start..start + end + "</style>".len(), " ");
        } else {
            break;
        }
    }

    // Strip all HTML tags
    let mut result = String::with_capacity(text.len());
    let mut in_tag = false;
    for ch in text.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => result.push(ch),
            _ => {}
        }
    }

    // Decode basic HTML entities
    let result = result
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&nbsp;", " ");

    // Collapse whitespace
    let mut collapsed = String::with_capacity(result.len());
    let mut prev_whitespace = false;
    for ch in result.chars() {
        if ch.is_whitespace() {
            if !prev_whitespace {
                collapsed.push(if ch == '\n' { '\n' } else { ' ' });
            }
            prev_whitespace = true;
        } else {
            collapsed.push(ch);
            prev_whitespace = false;
        }
    }

    collapsed.trim().to_string()
}

#[async_trait]
impl Tool for WebFetchTool {
    fn name(&self) -> &str {
        "web_fetch"
    }

    fn description(&self) -> &str {
        "Fetch a URL and return the page content as clean text."
    }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: self.name().to_string(),
            description: self.description().to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "url": {
                        "type": "string",
                        "description": "The URL to fetch"
                    },
                    "max_length": {
                        "type": "integer",
                        "description": "Maximum length of returned content in characters (default: 50000)"
                    }
                },
                "required": ["url"]
            }),
        }
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let url = args["url"]
            .as_str()
            .ok_or_else(|| openclaw_core::Error::ToolExecution("missing url".into()))?;
        let max_length = args["max_length"].as_u64().unwrap_or(50000) as usize;

        let resp = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| openclaw_core::Error::ToolExecution(e.to_string()))?;

        let body = resp
            .text()
            .await
            .map_err(|e| openclaw_core::Error::ToolExecution(e.to_string()))?;

        let mut content = html_to_text(&body);
        if content.len() > max_length {
            content.truncate(max_length);
        }

        let content_length = content.len();

        Ok(json!({
            "url": url,
            "content": content,
            "content_length": content_length,
        }))
    }
}
