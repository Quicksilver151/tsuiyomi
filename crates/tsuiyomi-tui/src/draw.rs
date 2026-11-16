use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, BorderType, Borders, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};
use tsuiyomi_core::{Library, Series};

use crate::{
    app::AppState,
    layout::{app_layout, library_layout, tab_layouts},
};

const HINTS: &str = "q quit   ↑/↓ move   ←/→ tabs   r refresh   click or scroll";

pub fn draw(frame: &mut Frame, app: &mut AppState) {
    let area = frame.area();
    let layout = app_layout(area);

    draw_tabs(frame, layout.tabs, app.tab);

    match app.tab {
        0 => draw_library(frame, layout.content, app),
        1 => placeholder(
            frame,
            layout.content,
            "Search",
            "Search is not implemented yet.",
        ),
        2 => placeholder(
            frame,
            layout.content,
            "Updates",
            "Update tracking is not implemented yet.",
        ),
        _ => draw_config(frame, layout.content, app),
    }

    draw_footer(frame, layout.footer, app);
}

fn draw_tabs(frame: &mut Frame, area: Rect, active: usize) {
    let mut spans = Vec::new();
    for tab in tab_layouts(area) {
        let style = if tab.index == active {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Gray)
        };
        spans.push(Span::styled(tab.label, style));
        spans.push(Span::raw(" "));
    }
    spans.push(Span::raw("  "));
    spans.push(Span::styled(
        "tsuiyomi",
        Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::ITALIC),
    ));

    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn draw_library(frame: &mut Frame, area: Rect, app: &mut AppState) {
    let layout = library_layout(area);
    draw_series_list(
        frame,
        layout.list,
        &app.library,
        &mut app.list_state,
        app.selected,
    );
    draw_details(frame, layout.details, app);
}

fn draw_series_list(
    frame: &mut Frame,
    area: Rect,
    library: &Library,
    list_state: &mut ListState,
    selected: usize,
) {
    let items: Vec<ListItem> = library
        .series
        .iter()
        .map(|series| {
            let title = Line::from(vec![
                Span::raw(series.title.clone()),
                Span::styled(
                    format!("  {} ch", series.chapters.len()),
                    Style::default().fg(Color::DarkGray),
                ),
            ]);
            ListItem::new(title)
        })
        .collect();

    let count = library.series.len();
    if count == 0 {
        list_state.select(None);
    } else {
        list_state.select(Some(selected.min(count - 1)));
    }

    let list = List::new(items)
        .block(panel(format!(" Library ({count}) ")))
        .highlight_style(
            Style::default()
                .fg(Color::White)
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("› ");

    frame.render_stateful_widget(list, area, list_state);
}

fn draw_details(frame: &mut Frame, area: Rect, app: &AppState) {
    let lines = match app.selected_series() {
        Some(series) => series_lines(series),
        None => empty_lines(app),
    };

    let paragraph = Paragraph::new(Text::from(lines))
        .block(panel(" Details "))
        .wrap(Wrap { trim: true });
    frame.render_widget(paragraph, area);
}

fn series_lines(series: &Series) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from(Span::styled(
            series.title.clone(),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
    ];

    if let Some(author) = &series.author {
        lines.push(field("Author", author));
    }
    if let Some(artist) = &series.artist {
        lines.push(field("Artist", artist));
    }
    if !series.tags.is_empty() {
        lines.push(field("Tags", &series.tags.join(", ")));
    }
    lines.push(field("Chapters", &series.chapters.len().to_string()));
    if let Some(latest) = series.chapters.first() {
        lines.push(field("Latest", &latest.title));
    }

    lines.push(Line::from(""));
    if series.description.trim().is_empty() {
        lines.push(Line::from(Span::styled(
            "No description.",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        lines.push(Line::from(series.description.clone()));
    }

    if !series.warnings.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!("{} metadata warning(s)", series.warnings.len()),
            Style::default().fg(Color::Yellow),
        )));
    }

    lines
}

fn empty_lines(app: &AppState) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from("No series found.")];

    if let Some(status) = &app.status {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            status.clone(),
            Style::default().fg(Color::Yellow),
        )));
    }
    if !app.library.root.as_os_str().is_empty() {
        lines.push(Line::from(""));
        lines.push(field("Library", &app.library.root.display().to_string()));
    }

    lines
}

fn field(label: &str, value: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{label:<9}"), Style::default().fg(Color::DarkGray)),
        Span::raw(value.to_owned()),
    ])
}

fn draw_config(frame: &mut Frame, area: Rect, app: &AppState) {
    let mut lines = vec![
        Line::from(Span::styled(
            "Config",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        field("Library", &app.library.root.display().to_string()),
        Line::from(""),
        Line::from(Span::styled(
            "Editing is not implemented yet. Set TSUIYOMI_LIBRARY for now.",
            Style::default().fg(Color::DarkGray),
        )),
    ];
    if !app.library.errors.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!("{} unreadable series", app.library.errors.len()),
            Style::default().fg(Color::Yellow),
        )));
    }

    let paragraph = Paragraph::new(Text::from(lines))
        .block(panel(" Config "))
        .wrap(Wrap { trim: true });
    frame.render_widget(paragraph, area);
}

fn placeholder(frame: &mut Frame, area: Rect, title: &'static str, message: &'static str) {
    let block = panel(title);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    frame.render_widget(
        Paragraph::new(message)
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::DarkGray)),
        inner,
    );
}

fn draw_footer(frame: &mut Frame, area: Rect, app: &AppState) {
    let (text, style) = match &app.status {
        Some(status) => (status.as_str(), Style::default().fg(Color::Yellow)),
        None => (HINTS, Style::default().fg(Color::DarkGray)),
    };
    frame.render_widget(Paragraph::new(text).style(style), area);
}

fn panel(title: impl Into<Line<'static>>) -> Block<'static> {
    Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
}
