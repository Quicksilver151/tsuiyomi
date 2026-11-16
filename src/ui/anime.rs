use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Frame,
};

use super::state::AnimeState;

pub fn render_panel(frame: &mut Frame, area: Rect, state: &AnimeState) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(50),
            Constraint::Length(1),
            Constraint::Percentage(50),
        ])
        .split(area);

    let items: Vec<ListItem> = state
        .list
        .iter()
        .enumerate()
        .map(|(index, anime)| {
            let style = if index == state.current {
                Style::default().bg(Color::Red).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(anime.title.clone()).style(style)
        })
        .collect();

    let list = List::new(items).block(panel("Library"));
    frame.render_widget(list, chunks[0]);

    let description = match state.list.get(state.current) {
        Some(anime) => format!("{}\n\n{}", anime.title, anime.description),
        None => state
            .status
            .clone()
            .unwrap_or_else(|| "No series found.".to_owned()),
    };
    let paragraph = Paragraph::new(description)
        .block(panel("Description"))
        .wrap(Wrap { trim: true });
    frame.render_widget(paragraph, chunks[2]);
}

fn panel(title: &'static str) -> Block<'static> {
    Block::default().title(title).borders(Borders::ALL)
}
