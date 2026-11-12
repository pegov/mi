use serde::{Deserialize, Serialize};

use super::{
    Tool,
    common::{WebToolKind, web_request},
};

#[derive(Deserialize, Serialize)]
struct FetchArgs {
    urls: Vec<String>,
}

pub struct FetchTool {
    pub base_url: String,
    pub client: reqwest::blocking::Client,
}

impl Tool for FetchTool {
    fn name(&self) -> &str {
        WebToolKind::Fetch.as_str()
    }

    fn json(&self) -> serde_json::Value {
        serde_json::json! {
                {
                    "type": "function",
                    "function": {
                        "name": WebToolKind::Fetch.as_str(),
        "description": "Fetch one or more web pages (up to 20). Returns a JSON array with one object per URL, each containing url, and either title (string; empty when unavailable) and content (string containing the page content as Markdown), or error (string) when that URL failed.",
                        "parameters": {
                            "type": "object",
                            "properties": {
                                "urls": {
                                    "type": "array",
                                    "items": {
                                        "type": "string",
                                        "maxLength": 8192,
                                        "description": "HTTP or HTTPS URL with a host and no username or password"
                                    },
                                    "minItems": 1,
                                    "maxItems": 20
                                }
                            },
                            "required": [
                                "urls"
                            ],
                            "additionalProperties": false
                        }
                    }
                }
            }
    }

    fn call(&mut self, args: &str) -> anyhow::Result<String> {
        let args: FetchArgs = serde_json::from_str(args)?;
        anyhow::ensure!(!args.urls.is_empty(), "urls must not be empty");
        anyhow::ensure!(
            args.urls.len() <= 20,
            "urls must be <= 20, got {}",
            args.urls.len()
        );
        let mut results = Vec::with_capacity(args.urls.len());
        for url in &args.urls {
            let mut value = match web_request(
                &self.client,
                &self.base_url,
                WebToolKind::Fetch,
                &serde_json::json!({ "url": url }),
            ) {
                Ok(body) => serde_json::from_str::<serde_json::Value>(&body)
                    .unwrap_or(serde_json::Value::String(body)),
                Err(error) => serde_json::json!({ "error": error.to_string() }),
            };
            if let serde_json::Value::Object(ref mut map) = value {
                map.insert("url".into(), serde_json::json!(url));
            } else {
                value = serde_json::json!({ "url": url, "content": value });
            }
            results.push(value);
        }
        Ok(serde_json::to_string(&results)?)
    }
}
