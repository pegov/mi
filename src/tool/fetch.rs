use serde::{Deserialize, Serialize};

use super::{
    Tool,
    common::{WebToolKind, web_request},
};

#[derive(Deserialize, Serialize)]
struct FetchArgs {
    url: String,
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
                    "description": "Fetch a web page. Returns raw JSON with title (string; empty when unavailable) and content (string containing the page content as Markdown).",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "url": {
                                "type": "string",
                                "maxLength": 8192,
                                "description": "HTTP or HTTPS URL with a host and no username or password"
                            }
                        },
                        "required": [
                            "url"
                        ],
                        "additionalProperties": false
                    }
                }
            }
        }
    }

    fn call(&mut self, args: &str) -> anyhow::Result<String> {
        let args: FetchArgs = serde_json::from_str(args)?;
        web_request(&self.client, &self.base_url, WebToolKind::Fetch, &args)
    }
}
