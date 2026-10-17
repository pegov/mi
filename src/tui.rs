use std::time::Duration;

use ratatui::{
    Frame, TerminalOptions,
    layout::{Constraint, Layout},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Paragraph, Widget},
};

#[derive(Default)]
pub struct TuiState {
    buffer: String,
    prompt: String,
}

pub fn render_tui(state: &mut TuiState) -> anyhow::Result<()> {
    let opts = TerminalOptions {
        viewport: ratatui::Viewport::Inline(3),
    };
    let mut terminal = ratatui::init_with_options(opts);

    let tokens = vec!["123", "456", "789", "\n", "n1", "n2", "n3\n", "n4", "n5"];

    for token in tokens {
        for c in token.chars() {
            if c == '\n' {
                terminal.insert_before(1, |buf| {
                    let msg = Line::from(vec![
                        Span::styled("[scroll] ", Style::default().fg(Color::Cyan)),
                        Span::raw(&state.buffer),
                    ]);
                    Paragraph::new(msg).render(buf.area, buf);
                })?;
                state.buffer.clear();
            } else {
                state.buffer.push(c);
            }
        }

        terminal.draw(|frame| render(frame, state))?;
        std::thread::sleep(Duration::from_millis(500));
    }
    if !state.buffer.is_empty() {
        terminal.insert_before(1, |buf| {
            let msg = Line::from(vec![
                Span::styled("[scroll] ", Style::default().fg(Color::Cyan)),
                Span::raw(&state.buffer),
            ]);
            Paragraph::new(msg).render(buf.area, buf);
        })?;
        state.buffer.clear();
    }

    terminal.draw(|frame| render(frame, state))?;

    ratatui::restore();
    Ok(())
}

fn render(frame: &mut Frame, state: &mut TuiState) {
    let [content, prompt, status] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Max(255),
        Constraint::Length(1),
    ])
    .areas(frame.area());

    let p_status = Paragraph::new(Line::from(Span::styled(
        "[STATUS_LINE]",
        Style::default().fg(Color::Green),
    )));

    let p_prompt = Paragraph::new(format!("> {}", state.prompt));

    let p_dynamic = Paragraph::new(Line::from(vec![
        Span::styled("[dynamic] ", Style::default().fg(Color::Red)),
        Span::raw(&state.buffer),
    ]));

    if state.buffer.is_empty() {
        frame.render_widget(p_prompt, prompt);
        frame.render_widget(p_status, status);
    } else {
        frame.render_widget(p_dynamic, content);
        frame.render_widget(p_prompt, prompt);
        frame.render_widget(p_status, status);
    }
}
