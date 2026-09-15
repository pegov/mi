use std::{fs::File, io::Write, path::Path};

use serde::Deserialize;

use super::Tool;

#[derive(Deserialize)]
struct WriteToolArgs {
    path: String,
    content: String,
}

pub struct WriteTool;

impl WriteTool {
    fn parse_args(&self, args: &str) -> anyhow::Result<WriteToolArgs> {
        Ok(serde_json::from_str(args)?)
    }
}

impl Tool for WriteTool {
    fn name(&self) -> &str {
        "write"
    }

    fn json(&self) -> serde_json::Value {
        serde_json::json! {
            {
                "type": "function",
                "function": {
                    "name": "write",
                    "description": "Write a file, replacing it entirely (creating it if needed). Parent directories are created automatically.",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "path": {
                                "type": "string",
                                "description": "path to the file"
                            },
                            "content": {
                                "type": "string",
                                "description": "full new contents of the file"
                            }
                        },
                        "required": ["path", "content"]
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
        Ok(format!(
            "[write] {} content_len={}",
            args.path,
            args.content.len()
        ))
    }

    fn call(&mut self, args: &str) -> anyhow::Result<String> {
        let args = self.parse_args(args)?;
        let path = Path::new(&args.path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = File::create(path)?;
        file.write_all(args.content.as_bytes())?;
        Ok("ok".into())
    }
}
