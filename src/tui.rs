use std::io;

use crossterm::{
    ExecutableCommand,
    event::{self, KeyboardEnhancementFlags},
};
use ratatui::{
    DefaultTerminal, TerminalOptions, Viewport,
    layout::{Constraint, Layout},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Paragraph, Widget},
};

use crate::printer::Mode;

pub struct Tui {
    pub terminal: DefaultTerminal,
    pub prompt: String,
    pub status: String,
    pub accepting_input: bool,
    buffer: String,
    mode: Mode,
    style: Style,
}

fn wrap_lines(text: &str, width: u16, style: Style) -> Vec<Line<'static>> {
    let width = usize::from(width.max(1));
    let mut lines = Vec::new();
    for line in text.split('\n') {
        let mut buffer = String::new();
        let mut used = 0;
        for c in line.chars() {
            if c.is_control() && c != '\t' {
                continue;
            }
            let text = if c == '\t' {
                " ".repeat(8 - used % 8)
            } else {
                c.to_string()
            };
            for c in text.chars() {
                let size = Span::raw(c.to_string()).width();
                if used + size > width && !buffer.is_empty() {
                    lines.push(Line::styled(std::mem::take(&mut buffer), style));
                    used = 0;
                }
                buffer.push(c);
                used += size;
            }
        }
        lines.push(Line::styled(buffer, style));
    }
    lines
}

impl Tui {
    pub fn new(status: String) -> anyhow::Result<Self> {
        let terminal = ratatui::try_init_with_options(TerminalOptions {
            viewport: Viewport::Inline(3),
        })?;
        let tui = Self {
            terminal,
            prompt: String::new(),
            status,
            accepting_input: false,
            buffer: String::new(),
            mode: Mode::None,
            style: Style::default(),
        };
        io::stdout().execute(event::PushKeyboardEnhancementFlags(
            KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES,
        ))?;
        io::stdout().execute(event::EnableBracketedPaste)?;
        Ok(tui)
    }

    pub fn draw(&mut self) -> anyhow::Result<()> {
        self.terminal.draw(|frame| {
            let [content, prompt, status] = Layout::vertical([
                Constraint::Length(1),
                Constraint::Min(1),
                Constraint::Length(1),
            ])
            .areas(frame.area());
            let dynamic = wrap_lines(&self.buffer, content.width, self.style);
            let offset = dynamic.len() - usize::from(content.height).min(dynamic.len());
            frame.render_widget(Paragraph::new(dynamic[offset..].to_vec()), content);
            let input = wrap_lines(
                &format!("> {}", self.prompt),
                prompt.width,
                Style::default(),
            );
            let offset = input.len() - usize::from(prompt.height).min(input.len());
            if self.accepting_input && prompt.width > 0 && prompt.height > 0 {
                let x = input
                    .last()
                    .map_or(0, Line::width)
                    .min(usize::from(prompt.width - 1));
                let y = (input.len() - offset - 1) as u16;
                frame.set_cursor_position((prompt.x + x as u16, prompt.y + y));
            }
            frame.render_widget(Paragraph::new(input[offset..].to_vec()), prompt);
            frame.render_widget(
                Paragraph::new(self.status.as_str()).style(Style::default().fg(Color::Green)),
                status,
            );
        })?;
        Ok(())
    }

    pub fn write(&mut self, text: &str) -> anyhow::Result<()> {
        self.terminal.autoresize()?;
        for part in text.split_inclusive('\n') {
            if let Some(line) = part.strip_suffix('\n') {
                self.buffer.push_str(line);
                let lines = wrap_lines(&self.buffer, self.terminal.size()?.width, self.style);
                for chunk in lines.chunks(usize::from(u16::MAX)) {
                    self.terminal.insert_before(chunk.len() as u16, |buf| {
                        Paragraph::new(chunk.to_vec()).render(buf.area, buf);
                    })?;
                }
                self.buffer.clear();
            } else {
                self.buffer.push_str(part);
            }
        }
        self.draw()
    }

    pub fn flush(&mut self) -> anyhow::Result<()> {
        if !self.buffer.is_empty() {
            self.write("\n")?;
        }
        Ok(())
    }

    pub fn print(&mut self, mode: Mode, text: &str) -> anyhow::Result<()> {
        if mode != self.mode {
            self.flush()?;
            if self.mode == Mode::Reasoning {
                self.write("</THINK>\n")?;
            }
            self.style = Style::default();
            if mode == Mode::Reasoning {
                self.write("<THINK>\n")?;
            }
            self.mode = mode;
        }
        self.write(text)
    }

    pub fn finish(&mut self) -> anyhow::Result<()> {
        self.flush()?;
        if self.mode == Mode::Reasoning {
            self.write("</THINK>\n")?;
        }
        self.mode = Mode::None;
        self.style = Style::default();
        self.draw()
    }

    pub fn tool(&mut self, name: &str, args: &str) -> anyhow::Result<()> {
        self.finish()?;
        self.style = Style::default().fg(Color::Indexed(37));
        self.write(&format!("[{name}] {args}\n"))?;
        self.style = Style::default();
        Ok(())
    }
}

impl Drop for Tui {
    fn drop(&mut self) {
        let _ = self.finish();
        let _ = self.terminal.clear();
        let _ = self.terminal.show_cursor();
        let _ = io::stdout().execute(event::PopKeyboardEnhancementFlags);
        let _ = io::stdout().execute(event::DisableBracketedPaste);
        ratatui::restore();
    }
}
