use std::path::Path;

use serde::Deserialize;

use super::{Tool, tool::format_lines};

#[derive(Deserialize)]
struct TailArgs {
    path: String,
    lines: Option<usize>,
    show_line_numbers: Option<bool>,
}

pub struct Tail;

impl Tail {
    fn parse_args(&self, args: &str) -> anyhow::Result<TailArgs> {
        Ok(serde_json::from_str(args)?)
    }
}

impl Tool for Tail {
    fn name(&self) -> &str {
        "tail"
    }

    fn json(&self) -> serde_json::Value {
        serde_json::json! {
            {
                "type": "function",
                "function": {
                    "name": "tail",
                    "description": "read the end of a file",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "path": {
                                "type": "string",
                                "description": "path to the file"
                            },
                            "lines": {
                                "type": "integer",
                                "description": "number of lines (default 10)"
                            },
                            "show_line_numbers": {
                                "type": "boolean",
                                "description": "show line numbers"
                            }
                        },
                        "required": ["path"]
                    }
                }
            }
        }
    }

    fn call(&mut self, args: &str) -> anyhow::Result<String> {
        let args = self.parse_args(args)?;
        let content = std::fs::read_to_string(Path::new(&args.path))?;
        let lines: Vec<&str> = content.lines().collect();
        let count = args.lines.unwrap_or(10);
        let start = lines.len() - count.min(lines.len());
        Ok(format_lines(
            &lines[start..],
            start + 1,
            args.show_line_numbers.unwrap_or(false),
        ))
    }
}
