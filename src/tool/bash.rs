use std::{fs, path::Path, process::Command};

use serde::Deserialize;

use super::Tool;

#[derive(Deserialize)]
struct BashToolArgs {
    command: String,
    cwd: Option<String>,
}

pub struct BashTool {
    stdout_last_n_lines: usize,
    stderr_last_n_lines: usize,
}

impl BashTool {
    pub fn new(stdout_n: usize, stderr_n: usize) -> Self {
        Self {
            stdout_last_n_lines: stdout_n,
            stderr_last_n_lines: stderr_n,
        }
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
                    "description": "Run a shell command via `bash -c ...`. Returns combined stdout+stderr plus exit code. If the output is too long, it will save full output to temp file and leave a mark `|Omitted N lines(s)|` in the result, use `read` tool to read it if you need to.",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "command": {
                                "type": "string",
                                "description": "shell command to run"
                            },
                            "cwd": {
                                "type": "string",
                                "description": "Working directory to run the command in. Omitted or empty uses the current directory"
                            }
                        },
                        "required": ["command"]
                    }
                }
            }
        }
    }

    fn call(&mut self, args: &str) -> anyhow::Result<String> {
        let args: BashToolArgs = serde_json::from_str(args)?;
        let mut cmd = Command::new("bash");
        cmd.arg("-c").arg(&args.command);
        if let Some(cwd) = args.cwd.filter(|cwd| !cwd.is_empty()) {
            cmd.current_dir(cwd);
        }
        let output = cmd.output()?;

        let mut stdout = String::from_utf8_lossy(&output.stdout);
        let mut stderr = String::from_utf8_lossy(&output.stderr);
        let exit_code = output.status.code().unwrap_or(-1);

        let should_cut_stdout = stdout.lines().count() > self.stdout_last_n_lines;
        let should_cut_stderr = stderr.lines().count() > self.stderr_last_n_lines;

        let combined = if should_cut_stdout || should_cut_stderr {
            let temp_file_path = tempfile::NamedTempFile::new()?.into_temp_path().keep()?;
            fs::write(
                &temp_file_path,
                [stdout.as_ref(), stderr.as_ref()].join("\n"),
            )?;

            if !stdout.is_empty() && should_cut_stdout {
                stdout = format_cut(&stdout, self.stdout_last_n_lines).into();
            }

            if !stderr.is_empty() && should_cut_stderr {
                stderr = format_cut(&stderr, self.stderr_last_n_lines).into();
            }

            format_output(Some(&temp_file_path), exit_code, &stdout, &stderr)
        } else {
            format_output(None, exit_code, &stdout, &stderr)
        };

        Ok(combined)
    }
}

fn format_output(path: Option<&Path>, exit_code: i32, stdout: &str, stderr: &str) -> String {
    let header = match path {
        Some(path) => format!(
            "Full output file path: {}\nExit code: {}",
            path.display(),
            exit_code
        ),
        None => format!("Exit code: {}", exit_code),
    };
    if stdout.is_empty() && stderr.is_empty() {
        header
    } else if stdout.is_empty() {
        format!("{}\n{}", header, stderr)
    } else if stderr.is_empty() {
        format!("{}\n{}", header, stdout)
    } else {
        format!("{}\nSTDOUT:\n{}\nSTDERR:\n{}", header, stdout, stderr)
    }
}

fn format_cut(s: &str, cut: usize) -> String {
    let lines: Vec<&str> = s.lines().collect();
    let n = lines.len() - cut;
    let s = lines[lines.len() - cut..].join("\n");
    let line_str = if n == 1 {
        "1 line".to_string()
    } else {
        format!("{n} lines")
    };
    format!("|Omitted {line_str}|\n...\n{s}")
}
