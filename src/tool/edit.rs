use std::{fs, path::Path};

use serde::Deserialize;
use similar::TextDiff;

use super::Tool;

#[derive(Deserialize)]
struct EditToolArgs {
    path: String,
    old_string: String,
    new_string: String,
    #[serde(default)]
    replace_all: bool,
}

pub struct EditTool;

impl EditTool {
    fn parse_args(&self, args: &str) -> anyhow::Result<EditToolArgs> {
        Ok(serde_json::from_str(args)?)
    }
}

impl Tool for EditTool {
    fn name(&self) -> &str {
        "edit"
    }

    fn json(&self) -> serde_json::Value {
        serde_json::json! {
            {
                "type": "function",
                "function": {
                    "name": "edit",
                    "description": "Replace an exact string in a file. The `old_string` must match a byte sequence in the file exactly once unless `replace_all` is true. Returns a diff of the change.",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "path": {
                                "type": "string",
                                "description": "path to the file"
                            },
                            "old_string": {
                                "type": "string",
                                "description": "Exact text to find. Must be unique unless replace_all is set."
                            },
                            "new_string": {
                                "type": "string",
                                "description": "replacement text"
                            },
                            "replace_all": {
                                "type": "boolean",
                                "description": "replace every occurrence instead of requiring uniqueness"
                            },
                        },
                        "required": ["path", "old_string", "new_string"]
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
            "[edit] {} old_string_len={} new_string_len={}",
            args.path,
            args.old_string.len(),
            args.new_string.len(),
        ))
    }

    fn call(&mut self, args: &str) -> anyhow::Result<String> {
        let args = self.parse_args(args)?;
        let path = Path::new(&args.path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let content = fs::read_to_string(path)?;
        let count = content.matches(&args.old_string).count();

        if count == 0 {
            return Ok("old_string not found in file".into());
        }

        if !args.replace_all && count > 1 {
            return Ok(format!(
                "old_string occurs {} times; set replace_all=true to replace all or provide more context to disambiguate",
                count,
            ));
        }

        let new_content = content.replace(&args.old_string, &args.new_string);
        if content == new_content {
            return Ok("no changes made (old_string and new_string are identical)".into());
        }

        fs::write(path, &new_content)?;

        let diff = TextDiff::from_lines(&content, &new_content)
            .unified_diff()
            .to_string();
        Ok(diff)
    }
}
