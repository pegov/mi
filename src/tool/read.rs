use std::path::Path;

use serde::Deserialize;

use super::Tool;

#[derive(Deserialize)]
pub struct ReadArgs {
    path: String,
}

pub struct Read;

impl Read {
    fn parse_args(&self, args: &str) -> anyhow::Result<ReadArgs> {
        Ok(serde_json::from_str(args)?)
    }
}

impl Tool for Read {
    fn name(&self) -> &str {
        "read"
    }

    fn json(&self) -> serde_json::Value {
        serde_json::json! {
            {
                "type": "function",
                "function": {
                    "name": "read",
                    "description": "read file by path (relative or absolute)",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "path": {
                                "type": "string",
                                "description": "path to the file"
                            }
                        },
                        "required": ["path"]
                    }
                }
            }
        }
    }

    fn validate_args(&self, args: &str) -> anyhow::Result<()> {
        let _ = self.parse_args(args)?;
        Ok(())
    }

    fn note(&self, args: &str) -> anyhow::Result<String> {
        let args = self.parse_args(args)?;
        Ok(format!("[read] {}", args.path))
    }

    fn call(&mut self, args: &str) -> anyhow::Result<String> {
        let args = self.parse_args(args)?;
        let path = Path::new(&args.path);
        let content = std::fs::read_to_string(path)?;
        Ok(content)
    }
}
