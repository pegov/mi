use serde::{Deserialize, Serialize};

use super::{
    Tool,
    common::{WebToolKind, web_request},
};

#[derive(Deserialize, Serialize)]
struct SearchArgs {
    query: String,
    #[serde(default = "default_count")]
    count: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    ori_question: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    need_content: Option<bool>,
}

fn default_count() -> u32 {
    5
}

pub struct SearchTool {
    pub base_url: String,
    pub client: reqwest::blocking::Client,
}

impl Tool for SearchTool {
    fn name(&self) -> &str {
        WebToolKind::Search.as_str()
    }

    fn json(&self) -> serde_json::Value {
        serde_json::json! {
            {
                "type": "function",
                "function": {
                    "name": WebToolKind::Search.as_str(),
                    "description": "Search the web for live information. Supports site:, -site:, lang:, after:, before:, and quoted phrases. Returns raw JSON with a results array (empty if no matches). Each result contains title, url, snippet (short summary), summary (long summary), content (page content; empty when need_content is false or content is unavailable), rerank_score (relevance from 0 to 3; higher is better), authority (integer from 0 to 4; higher is more authoritative, -1 means unknown), site_name, site_icon, and date (RFC 3339 publication time). site_name, site_icon, and date may be null.",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "query": {
                                "type": "string",
                                "minLength": 1,
                                "maxLength": 128
                            },
                            "count": {
                                "type": "integer",
                                "minimum": 1,
                                "maximum": 15,
                                "description": "Number of results (default 5)"
                            },
                            "ori_question": {
                                "type": "string",
                                "minLength": 1,
                                "maxLength": 128,
                                "description": "Original question to improve query understanding"
                            },
                            "need_content": {
                                "type": "boolean",
                                "description": "Include page content in results (default false)"
                            }
                        },
                        "required": [
                            "query"
                        ],
                        "additionalProperties": false
                    }
                }
            }
        }
    }

    fn call(&mut self, args: &str) -> anyhow::Result<String> {
        let args: SearchArgs = serde_json::from_str(args)?;
        anyhow::ensure!(args.count <= 15, "count must be <= 15, got {}", args.count);
        web_request(&self.client, &self.base_url, WebToolKind::Search, &args)
    }
}
