use std::path::Path;

use serde::Deserialize;

use super::{Tool, tool::format_lines};

#[derive(Deserialize)]
pub struct ReadArgs {
    path: String,
    from_line: Option<usize>,
    to_line: Option<usize>,
    show_line_numbers: Option<bool>,
}

pub struct Read;

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
                            },
                            "from_line": {
                                "type": "integer",
                                "description": "first line to read (1-based, inclusive)"
                            },
                            "to_line": {
                                "type": "integer",
                                "description": "last line to read (1-based, inclusive)"
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
        let args: ReadArgs = serde_json::from_str(args)?;
        let path = Path::new(&args.path);
        let content = std::fs::read_to_string(path)?;

        if args.from_line.is_none()
            && args.to_line.is_none()
            && !args.show_line_numbers.unwrap_or(false)
        {
            return Ok(content);
        }
        if args.from_line == Some(0) || args.to_line == Some(0) {
            anyhow::bail!("line numbers start at 1");
        }

        let lines: Vec<&str> = content.lines().collect();
        let start = args.from_line.unwrap_or(1);
        if let Some(end) = args.to_line {
            if start > end {
                anyhow::bail!("from_line must not be greater than to_line");
            }
        }

        let first = (start - 1).min(lines.len());
        let last = args.to_line.unwrap_or(lines.len()).min(lines.len());
        let selected = lines.get(first..last).unwrap_or(&[]);
        Ok(format_lines(
            selected,
            start,
            args.show_line_numbers.unwrap_or(false),
        ))
    }
}
