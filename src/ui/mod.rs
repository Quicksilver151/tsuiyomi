pub mod anime;
pub mod config;
pub mod input;
pub mod state;

pub use state::*;

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Tabs},
    Frame,
};

pub const TAB_COUNT: usize = 4;

pub fn ui(frame: &mut Frame, app_state: &AppState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(frame.area());

    let tabs = vec![
        tab_line("Anime", '1'),
        tab_line("Manga", '2'),
        tab_line("Updates", '3'),
        tab_line("Config", '4'),
    ];
    let tabs_widget = Tabs::new(tabs)
        .select(app_state.tab)
        .highlight_style(Style::default().fg(Color::Blue))
        .block(Block::default().borders(Borders::ALL).title("Tabs"));
    frame.render_widget(tabs_widget, chunks[0]);

    match app_state.tab {
        0 => anime::render_panel(frame, chunks[1], &app_state.anime),
        3 => config::render_panel(frame, chunks[1]),
        _ => placeholder(frame, chunks[1], "Not implemented yet."),
    }
}

fn tab_line(name: &str, key: char) -> Line<'static> {
    Line::from(vec![
        Span::raw(format!("{name} [")),
        Span::styled(
            key.to_string(),
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::UNDERLINED | Modifier::BOLD),
        ),
        Span::raw("]"),
    ])
}

fn placeholder(frame: &mut Frame, area: Rect, text: &'static str) {
    let widget = Paragraph::new(text).block(Block::default().borders(Borders::ALL));
    frame.render_widget(widget, area);
}
