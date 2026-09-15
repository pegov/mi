use serde::Deserialize;

use super::Tool;

#[derive(Deserialize)]
struct BashToolArgs {
    command: String,
}

pub struct BashTool;

impl BashTool {
    fn parse_args(&self, args: &str) -> anyhow::Result<BashToolArgs> {
        Ok(serde_json::from_str(args)?)
    }
}

impl Tool for BashTool {
    fn name(&self) -> &str {
        "bash"
    }

    fn json(&self) -> serde_json::Value {
        serde_json::json! {
            {
                "type": "function",
                "function": {
                    "name": "bash",
                    "description": "Run a shell command via `bash -c ...`. Returns combined stdout+stderr plus exit code.",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "command": {
                                "type": "string",
                                "description": "shell command to run"
                            }
                        },
                        "required": ["command"]
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
        Ok(format!("[bash] {}", args.command))
    }

    fn call(&mut self, args: &str) -> anyhow::Result<String> {
        use std::process::Command;

        let args = self.parse_args(args)?;
        let output = Command::new("bash").arg("-c").arg(&args.command).output()?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let exit_code = output.status.code().unwrap_or(-1);

        let combined = if stdout.is_empty() && stderr.is_empty() {
            format!("Exit code: {}", exit_code)
        } else if stdout.is_empty() {
            format!("Exit code: {}\n{}", exit_code, stderr)
        } else if stderr.is_empty() {
            format!("Exit code: {}\n{}", exit_code, stdout)
        } else {
            format!(
                "Exit code: {}\nSTDOUT:\n{}\nSTDERR:\n{}",
                exit_code, stdout, stderr
            )
        };

        Ok(combined)
    }
}
