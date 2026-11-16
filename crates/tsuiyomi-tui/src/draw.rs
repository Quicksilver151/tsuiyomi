use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};
use tsuiyomi_core::{Library, Series};

use crate::{
    app::AppState,
    layout::{app_layout, library_layout, tab_layouts, TAB_NAMES},
    palette::{
        BASE, BLUE, GREEN, LAVENDER, MAUVE, OVERLAY0, OVERLAY1, PEACH, RED, SUBTEXT1, SURFACE0,
        SURFACE1, TEAL, TEXT, YELLOW,
    },
};

pub fn draw(frame: &mut Frame, app: &mut AppState) {
    let area = frame.area();
    frame.render_widget(Block::default().style(Style::default().bg(BASE)), area);

    let layout = app_layout(area, app.fullscreen);

    if let Some(header) = layout.header {
        draw_header(frame, header);
    }

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

    draw_tabs(frame, layout.tabs, app.tab);

    if let Some(commands) = layout.commands {
        draw_commands(frame, commands);
    }
}

fn draw_header(frame: &mut Frame, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(SURFACE1))
        .style(Style::default().bg(BASE));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    frame.render_widget(
        Paragraph::new("Tsuiyomi")
            .alignment(Alignment::Center)
            .style(Style::default().fg(LAVENDER).add_modifier(Modifier::BOLD)),
        inner,
    );
}

fn draw_tabs(frame: &mut Frame, area: Rect, active: usize) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(SURFACE1))
        .style(Style::default().bg(BASE));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let layouts = tab_layouts(area);
    let mut spans = Vec::new();
    for (position, tab) in layouts.iter().enumerate() {
        if position > 0 {
            spans.push(Span::styled("│", Style::default().fg(SURFACE1)));
        }

        let label = format!("{} {}", tab.index + 1, TAB_NAMES[tab.index]);
        let text = center_label(&label, tab.area.width);
        let style = if tab.index == active {
            Style::default()
                .fg(BASE)
                .bg(LAVENDER)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(OVERLAY1)
        };
        spans.push(Span::styled(text, style));
    }

    frame.render_widget(Paragraph::new(Line::from(spans)), inner);

    // Join the tab separators into the top and bottom borders.
    let buffer = frame.buffer_mut();
    let bottom = area.y + area.height - 1;
    for tab in layouts.iter().skip(1) {
        let x = tab.area.x - 1;
        buffer[(x, area.y)].set_symbol("┬");
        buffer[(x, bottom)].set_symbol("┴");
    }
}

fn draw_commands(frame: &mut Frame, area: Rect) {
    let block = panel(
        Line::from(Span::styled(" commands ", Style::default().fg(OVERLAY1))),
        SURFACE1,
    );
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let line = Line::from(vec![
        Span::raw(" "),
        key("q"),
        Span::raw("uit"),
        separator(),
        key("c"),
        Span::raw("onfig"),
        separator(),
        key("f"),
        Span::raw("ullscreen"),
        separator(),
        key("Esc"),
        Span::raw("ape"),
    ]);
    frame.render_widget(Paragraph::new(line), inner);
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
                    Style::default().fg(OVERLAY0),
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

    let title = Line::from(vec![
        Span::styled(
            " Library ",
            Style::default().fg(LAVENDER).add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!("{count} "), Style::default().fg(OVERLAY0)),
    ]);

    let list = List::new(items)
        .block(panel(title, LAVENDER))
        .style(Style::default().fg(TEXT))
        .highlight_style(
            Style::default()
                .fg(LAVENDER)
                .bg(SURFACE0)
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
        .block(panel(
            Line::from(Span::styled(
                " Details ",
                Style::default().fg(BLUE).add_modifier(Modifier::BOLD),
            )),
            SURFACE1,
        ))
        .style(Style::default().fg(SUBTEXT1))
        .wrap(Wrap { trim: true });
    frame.render_widget(paragraph, area);
}

fn series_lines(series: &Series) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from(Span::styled(
            series.title.clone(),
            Style::default().fg(LAVENDER).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
    ];

    if let Some(author) = &series.author {
        lines.push(field("Author", author, PEACH));
    }
    if let Some(artist) = &series.artist {
        lines.push(field("Artist", artist, MAUVE));
    }
    if !series.tags.is_empty() {
        lines.push(field("Tags", &series.tags.join(", "), TEAL));
    }
    lines.push(field("Chapters", &series.chapters.len().to_string(), GREEN));
    if let Some(latest) = series.chapters.first() {
        lines.push(field("Latest", &latest.title, BLUE));
    }

    lines.push(Line::from(""));
    if series.description.trim().is_empty() {
        lines.push(Line::from(Span::styled(
            "No description.",
            Style::default().fg(OVERLAY1),
        )));
    } else {
        lines.push(Line::from(series.description.clone()));
    }

    if !series.warnings.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!("{} metadata warning(s)", series.warnings.len()),
            Style::default().fg(YELLOW),
        )));
    }

    lines
}

fn empty_lines(app: &AppState) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from(Span::styled(
        "No series found.",
        Style::default().fg(SUBTEXT1),
    ))];

    if let Some(status) = &app.status {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            status.clone(),
            Style::default().fg(PEACH),
        )));
    }
    if !app.library.root.as_os_str().is_empty() {
        lines.push(Line::from(""));
        lines.push(field(
            "Library",
            &app.library.root.display().to_string(),
            OVERLAY1,
        ));
    }

    lines
}

fn field(label: &str, value: &str, value_color: Color) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{label:<9}"), Style::default().fg(OVERLAY1)),
        Span::styled(value.to_owned(), Style::default().fg(value_color)),
    ])
}

fn draw_config(frame: &mut Frame, area: Rect, app: &AppState) {
    let mut lines = vec![
        Line::from(Span::styled(
            "Config",
            Style::default().fg(LAVENDER).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        field("Library", &app.library.root.display().to_string(), TEXT),
        Line::from(""),
        Line::from(Span::styled(
            "Editing is not implemented yet. Set TSUIYOMI_LIBRARY for now.",
            Style::default().fg(OVERLAY1),
        )),
    ];
    if !app.library.errors.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!("{} unreadable series", app.library.errors.len()),
            Style::default().fg(RED),
        )));
    }

    let paragraph = Paragraph::new(Text::from(lines))
        .block(panel(
            Line::from(Span::styled(
                " Config ",
                Style::default().fg(LAVENDER).add_modifier(Modifier::BOLD),
            )),
            LAVENDER,
        ))
        .style(Style::default().fg(SUBTEXT1))
        .wrap(Wrap { trim: true });
    frame.render_widget(paragraph, area);
}

fn placeholder(frame: &mut Frame, area: Rect, title: &'static str, message: &'static str) {
    let block = panel(
        Line::from(Span::styled(
            format!(" {title} "),
            Style::default().fg(LAVENDER).add_modifier(Modifier::BOLD),
        )),
        LAVENDER,
    );
    let inner = block.inner(area);
    frame.render_widget(block, area);
    frame.render_widget(
        Paragraph::new(message)
            .alignment(Alignment::Center)
            .style(Style::default().fg(OVERLAY1)),
        inner,
    );
}

fn key(text: &'static str) -> Span<'static> {
    Span::styled(
        text,
        Style::default().fg(LAVENDER).add_modifier(Modifier::BOLD),
    )
}

fn separator() -> Span<'static> {
    Span::styled(" │ ", Style::default().fg(SURFACE1))
}

fn center_label(label: &str, width: u16) -> String {
    let width = width as usize;
    let length = label.chars().count();
    if length >= width {
        return label.chars().take(width).collect();
    }

    let left = (width - length) / 2;
    let right = width - length - left;
    format!("{}{}{}", " ".repeat(left), label, " ".repeat(right))
}

fn panel(title: Line<'static>, border: Color) -> Block<'static> {
    Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border))
        .style(Style::default().bg(BASE))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use ratatui::{backend::TestBackend, Terminal};
    use tsuiyomi_core::{Chapter, ChapterKind, Library, Series};

    use super::*;

    fn test_app() -> AppState {
        let series = Series {
            path: PathBuf::from("/tmp/library/Test Series"),
            title: "Test Series".to_owned(),
            author: Some("Author".to_owned()),
            artist: None,
            description: "A test description.".to_owned(),
            tags: vec!["fantasy".to_owned()],
            chapters: vec![Chapter {
                path: PathBuf::from("/tmp/library/Test Series/Chapter 1"),
                title: "Chapter 1".to_owned(),
                number: Some(1.0),
                kind: ChapterKind::Directory,
            }],
            warnings: Vec::new(),
        };
        let library = Library {
            root: PathBuf::from("/tmp/library"),
            series: vec![series],
            errors: Vec::new(),
        };
        AppState::new(library, None)
    }

    fn render(app: &mut AppState, width: u16, height: u16) -> String {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| draw(frame, app)).unwrap();

        let buffer = terminal.backend().buffer();
        let mut text = String::new();
        for y in 0..height {
            for x in 0..width {
                text.push_str(buffer[(x, y)].symbol());
            }
            text.push('\n');
        }
        text
    }

    #[test]
    fn renders_header_content_tabs_and_commands() {
        let mut app = test_app();
        let text = render(&mut app, 80, 24);
        println!("{text}");

        assert!(text.contains("Tsuiyomi"));
        assert!(text.contains("commands"));
        assert!(text.contains("quit"));
        assert!(text.contains("fullscreen"));
        assert!(text.contains("┬"));
        assert!(text.contains("┴"));
        assert!(text.contains("Test Series"));
        assert!(text.contains("Details"));
    }

    #[test]
    fn fullscreen_hides_header_and_commands() {
        let mut app = test_app();
        app.toggle_fullscreen();
        let text = render(&mut app, 80, 24);

        assert!(!text.contains("Tsuiyomi"));
        assert!(!text.contains("commands"));
        assert!(text.contains("Library"));
    }

    #[test]
    fn empty_library_shows_status_without_panicking() {
        let library = Library {
            root: PathBuf::from("/tmp/library"),
            ..Library::default()
        };
        let mut app = AppState::new(library, Some("Set TSUIYOMI_LIBRARY".to_owned()));
        let text = render(&mut app, 80, 24);

        assert!(text.contains("No series found."));
        assert!(text.contains("Set TSUIYOMI_LIBRARY"));
    }
}
