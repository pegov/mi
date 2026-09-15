use std::io::Write;

#[derive(Clone, Default, PartialEq)]
pub enum Mode {
    #[default]
    None,
    Reasoning,
    ToolCall,
    Tool,
    Content,
}

#[derive(Clone, Default)]
pub struct Printer {
    mode: Mode,
}

pub const ANSI_RESET: &str = "\x1b[0m";
pub const ANSI_RESET_FAINT: &str = "\x1b[22m";
pub const ANSI_FAINT: &str = "\x1b[2m";
pub const ANSI_BOLD: &str = "\x1b[1m";
pub const ANSI_GREEN: &str = "\x1b[32m";
pub const ANSI_BLUE: &str = "\x1b[34m";
pub const ANSI_LIGHT_BLUE: &str = "\x1b[94m";
pub const ANSI_BRIGHT_BLUE: &str = "\x1b[94m";
pub const ANSI_MAGENTA: &str = "\x1b[35m";
pub const ANSI_CYAN: &str = "\x1b[36m";
pub const ANSI_BRIGHT_CYAN: &str = "\x1b[96m";
pub const ANSI_BACKTICK: &str = ANSI_CYAN;
pub const ANSI_COLOR_24: &str = "\x1b[38;5;24m";
pub const ANSI_COLOR_30: &str = "\x1b[38;5;30m";
pub const ANSI_COLOR_37: &str = "\x1b[38;5;37m";
pub const ANSI_TOOL: &str = ANSI_COLOR_37;
pub const ANSI_HIDE_CURSOR: &str = "\x1b[?25l";
pub const ANSI_SHOW_CURSOR: &str = "\x1b[?25h";

impl Printer {
    pub fn set_mode(&mut self, mode: Mode) {
        self.mode = mode;
    }

    pub fn hide_cursor(&self) -> anyhow::Result<()> {
        let mut stdout = std::io::stdout().lock();
        stdout.write_all(ANSI_HIDE_CURSOR.as_bytes())?;
        stdout.write_all(ANSI_BOLD.as_bytes())?;
        stdout.flush()?;
        Ok(())
    }

    pub fn show_cursor(&self) -> anyhow::Result<()> {
        let mut stdout = std::io::stdout().lock();
        stdout.write_all(ANSI_SHOW_CURSOR.as_bytes())?;
        stdout.flush()?;
        Ok(())
    }

    pub fn tool(&mut self, tool_name: &str, tool_args: &str) -> anyhow::Result<()> {
        {
            let mut stdout = std::io::stdout().lock();
            stdout.write_all(ANSI_TOOL.as_bytes())?;
            writeln!(stdout, "[{}] {}", tool_name, tool_args)?;
        }
        self.reset()?;
        Ok(())
    }

    pub fn print(&mut self, mode: Mode, content: &str) -> anyhow::Result<()> {
        let mut stdout = std::io::stdout().lock();

        match (self.mode.clone(), mode.clone()) {
            (Mode::None, Mode::Content) => {}
            (Mode::None, Mode::Reasoning) => {
                stdout.write_all("<THINK>\n".as_bytes())?;
            }
            (Mode::Content, Mode::Content) => {}
            (Mode::Content, Mode::Reasoning) => {
                stdout.write_all("\n<THINK>\n".as_bytes())?;
            }
            (Mode::Reasoning, Mode::Content) => {
                // TODO: if no new line before, i should add it here
                stdout.write_all("</THINK>\n".as_bytes())?;
            }
            (Mode::Reasoning, Mode::Reasoning) => {}
            _ => {}
        }

        self.mode = mode;
        stdout.write_all(content.as_bytes())?;
        stdout.flush()?;
        Ok(())
    }

    pub fn reset(&mut self) -> anyhow::Result<()> {
        self.mode = Mode::None;
        let mut stdout = std::io::stdout().lock();
        stdout.write_all(ANSI_RESET.as_bytes())?;
        stdout.write_all(ANSI_RESET_FAINT.as_bytes())?;
        stdout.flush()?;
        Ok(())
    }

    pub fn new_line(&mut self) -> anyhow::Result<()> {
        std::io::stdout().write_all("\n".as_bytes())?;
        Ok(())
    }
}

impl Drop for Printer {
    fn drop(&mut self) {
        let _ = self.reset();
    }
}
