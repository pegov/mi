use std::fs;

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
                    "description": "Run a shell command via `bash -c ...`. Returns combined stdout+stderr plus exit code. If the output is too long, it will save full output to temp file and leave a mark `|Output is too long, ...|` in the result, use `read` tool to read it if you need to.",
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
        use std::process::Command;

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

        let mut combined = if stdout.is_empty() && stderr.is_empty() {
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

        let (out_lines, err_lines) = (lines_len(&stdout), lines_len(&stderr));
        let should_cut_stdout = out_lines > self.stdout_last_n_lines;
        let should_cut_stderr = err_lines > self.stderr_last_n_lines;

        if should_cut_stdout || should_cut_stderr {
            let temp_file_path = tempfile::NamedTempFile::new()?.into_temp_path().keep()?;

            fs::write(&temp_file_path, &combined)?;

            if !stdout.is_empty() && should_cut_stdout {
                stdout = format_cut(&stdout, out_lines, self.stdout_last_n_lines).into();
            }

            if !stderr.is_empty() && should_cut_stderr {
                stderr = format_cut(&stderr, err_lines, self.stderr_last_n_lines).into();
            }

            combined = if stdout.is_empty() {
                format!(
                    "Full output file path: {}\nExit code: {}\n{}\n",
                    temp_file_path.display(),
                    exit_code,
                    stderr,
                )
            } else if stderr.is_empty() {
                format!(
                    "Full output file path: {}\nExit code: {}\n{}",
                    temp_file_path.display(),
                    exit_code,
                    stdout,
                )
            } else {
                format!(
                    "Full output file path: {}\nExit code: {}\nSTDOUT:\n{}\nSTDERR:\n{}",
                    temp_file_path.display(),
                    exit_code,
                    stdout,
                    stderr,
                )
            };
        }

        Ok(combined)
    }
}

fn lines_len(s: &str) -> usize {
    s.lines().count()
}

fn format_cut(s: &str, was: usize, cut: usize) -> String {
    let mut lines: Vec<&str> = s.lines().rev().take(cut).collect();
    lines.reverse();
    let s = lines.join("\n");
    let n = was - cut;
    let line_str = if n == 1 {
        "1 line was"
    } else {
        &format!("{n} lines were")
    };
    format!("|Output is too long, {line_str} cut off|\n...\n{s}")
}
