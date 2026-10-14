use std::{io::stdout, time::Duration};

use crossterm::{
    cursor::{MoveToColumn, MoveUp},
    execute,
    terminal::{Clear, ClearType},
};
use ratatui::{
    Frame, TerminalOptions,
    layout::{Constraint, Layout},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Paragraph, Widget},
};

#[derive(Default)]
struct State {
    buffer: String,
}

pub fn start_tui() -> anyhow::Result<()> {
    let opts = TerminalOptions {
        viewport: ratatui::Viewport::Inline(2),
    };
    let mut terminal = ratatui::init_with_options(opts);

    let tokens = vec!["123", "456", "789", "\n", "n1", "n2", "n3\n", "n4", "n5"];

    let mut state = State::default();

    for token in tokens {
        for c in token.chars() {
            if c == '\n' {
                terminal.insert_before(1, |buf| {
                    let msg = Line::from(vec![
                        Span::styled("[scroll] ", Style::default().fg(Color::Cyan)),
                        Span::raw(format!("{}", &state.buffer)),
                    ]);
                    Paragraph::new(msg).render(buf.area, buf);
                })?;
                state.buffer.clear();
            } else {
                state.buffer.push(c);
            }
        }

        terminal.draw(|frame| render(frame, &mut state))?;
        std::thread::sleep(Duration::from_millis(100));
    }
    if !state.buffer.is_empty() {
        terminal.insert_before(1, |buf| {
            let msg = Line::from(vec![
                Span::styled("[scroll] ", Style::default().fg(Color::Cyan)),
                Span::raw(format!("{}", &state.buffer)),
            ]);
            Paragraph::new(msg).render(buf.area, buf);
        })?;
        state.buffer.clear();
    }

    terminal.draw(|frame| render(frame, &mut state))?;

    ratatui::restore();
    execute!(
        stdout(),
        MoveToColumn(0),
        MoveUp(1),
        Clear(ClearType::FromCursorDown),
    )?;
    Ok(())
}

fn render(frame: &mut Frame, state: &mut State) {
    let [content, status] =
        Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).areas(frame.area());

    if state.buffer.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "[STATUS_LINE]",
                Style::default().fg(Color::Green),
            ))),
            content,
        );
    } else {
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("[dynamic] ", Style::default().fg(Color::Red)),
                Span::raw(&state.buffer),
            ])),
            content,
        );

        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "[STATUS_LINE]",
                Style::default().fg(Color::Green),
            ))),
            status,
        );
    }
}
