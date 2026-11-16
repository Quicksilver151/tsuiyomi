use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Gauge, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};
use ratatui_image::{
    sliced::{SignedPosition, SlicedImage},
    Resize, StatefulImage,
};
use tsuiyomi_core::{Chapter, ChapterKind, Library, ProgressStore, ReadingMode, Series};

use crate::{
    app::{
        AppState, LibraryView, MoreFocus, MoreRow, MoreSection, ReaderMode, MORE_SECTIONS,
        MORE_SETTINGS_ROWS,
    },
    layout::{
        app_layout, library_layout, more_layout, reader_bar_layout, settings_rows, tab_layouts,
        TAB_NAMES,
    },
    palette::{
        BASE, BLUE, GREEN, LAVENDER, MAUVE, OVERLAY0, OVERLAY1, PEACH, SUBTEXT1, SURFACE0,
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
            "Updates",
            "Update tracking is not implemented yet.",
        ),
        2 => placeholder(
            frame,
            layout.content,
            "History",
            "Reading history is not implemented yet.",
        ),
        3 => placeholder(
            frame,
            layout.content,
            "Browse",
            "Browsing remote sources is not implemented yet.",
        ),
        _ => draw_more(frame, layout.content, app),
    }

    if app.tab == 0 && app.view == LibraryView::Reader {
        draw_reader_bar(frame, layout.tabs, app);
    } else {
        draw_tabs(frame, layout.tabs, app.tab);
    }

    if let Some(commands) = layout.commands {
        draw_commands(frame, commands, app);
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

fn draw_reader_bar(frame: &mut Frame, area: Rect, app: &AppState) {
    let block = panel(
        Line::from(Span::styled(" reader ", Style::default().fg(OVERLAY1))),
        SURFACE1,
    );
    frame.render_widget(block, area);

    let Some(reader) = app.reader.as_ref() else {
        return;
    };
    let layout = reader_bar_layout(area);

    let previous_enabled = app.chapter_index + 1 < app.chapter_count();
    let next_enabled = app.chapter_index > 0;
    render_reader_button(
        frame,
        layout.previous,
        "p",
        "‹ prev",
        previous_enabled,
        Alignment::Center,
    );
    render_reader_button(
        frame,
        layout.next,
        "next ›",
        "n",
        next_enabled,
        Alignment::Center,
    );

    let total = reader.pages.len().max(1);
    let current = (reader.page_index + 1).min(total);
    let ratio = if total > 1 {
        reader.page_index.min(total - 1) as f64 / (total - 1) as f64
    } else {
        0.0
    };
    frame.render_widget(
        Gauge::default()
            .ratio(ratio)
            .label(format!(" {current} / {total} "))
            .gauge_style(Style::default().fg(LAVENDER).bg(SURFACE0))
            .style(Style::default().fg(TEXT)),
        layout.progress,
    );

    let (mode_key, mode_label) = match app.reader_mode {
        ReaderMode::Page => ("m", "scroll"),
        ReaderMode::Scroll => ("m", "page"),
    };
    let mode = Line::from(vec![
        Span::styled(
            mode_key,
            Style::default().fg(LAVENDER).add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!(" {mode_label}"), Style::default().fg(OVERLAY1)),
    ]);
    frame.render_widget(
        Paragraph::new(mode).alignment(Alignment::Right),
        layout.mode,
    );
}

fn render_reader_button(
    frame: &mut Frame,
    area: Rect,
    key: &'static str,
    label: &'static str,
    enabled: bool,
    alignment: Alignment,
) {
    let key_style = if enabled {
        Style::default().fg(LAVENDER).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(OVERLAY0)
    };
    let label_style = if enabled {
        Style::default().fg(TEXT)
    } else {
        Style::default().fg(OVERLAY0)
    };
    let line = Line::from(vec![
        Span::styled(key, key_style),
        Span::styled(format!(" {label}"), label_style),
    ]);
    frame.render_widget(Paragraph::new(line).alignment(alignment), area);
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

        let label = TAB_NAMES[tab.index];
        let text = center_label(label, tab.area.width);
        let style = if tab.index == active {
            Style::default().fg(LAVENDER).add_modifier(Modifier::BOLD)
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

fn draw_commands(frame: &mut Frame, area: Rect, app: &AppState) {
    let block = panel(
        Line::from(Span::styled(" commands ", Style::default().fg(OVERLAY1))),
        SURFACE1,
    );
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let items: &[(&str, &str)] = if app.tab == 0 && app.view == LibraryView::Reader {
        if app.reader_mode == ReaderMode::Scroll {
            &[
                ("q", "quit"),
                ("↑↓", "scroll"),
                ("←→", "page"),
                ("Space", "screen"),
                ("+/-", "zoom"),
                ("f", "full"),
                ("Esc", "back"),
            ]
        } else {
            &[
                ("q", "quit"),
                ("←→", "page"),
                ("f", "full"),
                ("Esc", "back"),
            ]
        }
    } else if app.tab == 4 {
        if app.more_focus == MoreFocus::Sections {
            &[
                ("q", "quit"),
                ("↑↓", "section"),
                ("→/↵", "select"),
                ("Esc", "back"),
            ]
        } else {
            &[
                ("q", "quit"),
                ("↑↓", "setting"),
                ("+/-", "change"),
                ("←", "sections"),
                ("Esc", "back"),
            ]
        }
    } else {
        &[
            ("q", "quit"),
            ("↑↓", "move"),
            ("Tab", "tabs"),
            ("r", "rescan"),
            ("c", "config"),
            ("f", "full"),
            ("Esc", "back"),
            ("↵", "open"),
        ]
    };

    let mut spans = Vec::new();
    for (position, (key, label)) in items.iter().enumerate() {
        if position > 0 {
            spans.push(Span::styled(" · ", Style::default().fg(OVERLAY0)));
        }
        spans.push(Span::styled(
            *key,
            Style::default().fg(LAVENDER).add_modifier(Modifier::BOLD),
        ));
        if !label.is_empty() {
            spans.push(Span::styled(
                format!(" {label}"),
                Style::default().fg(OVERLAY0),
            ));
        }
    }

    frame.render_widget(Paragraph::new(Line::from(spans)), inner);
}

fn draw_library(frame: &mut Frame, area: Rect, app: &mut AppState) {
    let layout = library_layout(area);

    match app.view {
        LibraryView::Reader => {
            draw_reader(frame, area, app);
            return;
        }
        LibraryView::Series => {
            draw_categories(frame, layout.categories, app);
            let category = app
                .library
                .categories
                .get(app.category_index)
                .map(String::as_str);
            draw_series_list(
                frame,
                layout.list,
                &app.library,
                category,
                &mut app.list_state,
                app.selected,
            );
        }
        LibraryView::Chapters => {
            draw_breadcrumb(frame, layout.categories, app);
            let (title, chapters) = app
                .selected_series()
                .map(|series| (series.title.clone(), series.chapters.clone()))
                .unwrap_or_else(|| ("Series".to_owned(), Vec::new()));
            draw_chapter_list(
                frame,
                layout.list,
                &title,
                &chapters,
                &app.progress,
                &mut app.chapter_list_state,
                app.chapter_index,
            );
        }
    }

    draw_details(frame, layout.details, app);
}

fn draw_categories(frame: &mut Frame, area: Rect, app: &AppState) {
    let mut spans = vec![Span::raw(" ")];
    for (index, name) in app.library.categories.iter().enumerate() {
        if index > 0 {
            spans.push(Span::styled(" · ", Style::default().fg(OVERLAY0)));
        }
        let count = app
            .library
            .series
            .iter()
            .filter(|series| in_category(series, Some(name)))
            .count();
        let style = if index == app.category_index {
            Style::default().fg(LAVENDER).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(OVERLAY1)
        };
        spans.push(Span::styled(name.clone(), style));
        spans.push(Span::styled(
            format!(" {count}"),
            Style::default().fg(OVERLAY0),
        ));
    }
    spans.push(Span::styled(
        "   [ ] categories",
        Style::default().fg(OVERLAY0),
    ));

    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn draw_reader(frame: &mut Frame, area: Rect, app: &mut AppState) {
    let mode = app.reader_mode;
    if mode == ReaderMode::Scroll {
        app.ensure_scroll_width(area.width);
        app.ensure_scroll_view(area.height.saturating_sub(1));
    }

    let protocol_name = app.protocol_name();
    let zoom = app.zoom;
    let Some(reader) = app.reader.as_mut() else {
        return;
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(area);

    let detail = match mode {
        ReaderMode::Page => protocol_name.to_owned(),
        ReaderMode::Scroll => format!("{protocol_name} {zoom}%"),
    };
    let status = Line::from(vec![
        Span::raw(" "),
        Span::styled(
            format!("{} › {}", reader.series_title, reader.chapter_title),
            Style::default().fg(LAVENDER).add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!("   [{detail}]"), Style::default().fg(OVERLAY0)),
    ]);
    frame.render_widget(Paragraph::new(status), chunks[0]);

    if let Some(error) = &reader.error {
        frame.render_widget(
            Paragraph::new(error.clone())
                .alignment(Alignment::Center)
                .style(Style::default().fg(PEACH)),
            chunks[1],
        );
        return;
    }

    match mode {
        ReaderMode::Page => {
            if let Some(protocol) = reader.protocol.as_mut() {
                frame.render_stateful_widget(
                    StatefulImage::new().resize(Resize::Fit(None)),
                    chunks[1],
                    protocol,
                );
            } else {
                placeholder_message(frame, chunks[1], "Loading page…");
            }
        }
        ReaderMode::Scroll => {
            if let Some(scroll) = reader.scroll.as_ref() {
                let area = chunks[1];
                let offset = scroll.offset;
                for (index, page) in scroll.layout.iter().enumerate() {
                    let top = i64::from(page.start) - i64::from(offset);
                    if top >= i64::from(area.height) {
                        break;
                    }
                    if top + i64::from(page.rows) <= 0 {
                        continue;
                    }

                    let Some(protocol) = scroll.protocols.get(index).and_then(Option::as_ref)
                    else {
                        continue;
                    };
                    let x = area.width.saturating_sub(page.cell_width) / 2;
                    frame.render_widget(
                        SlicedImage::new(
                            protocol,
                            SignedPosition {
                                x: x as i16,
                                y: top as i16,
                            },
                        ),
                        area,
                    );
                }
            } else {
                placeholder_message(frame, chunks[1], "Preparing scroll view…");
            }
        }
    }
}

fn placeholder_message(frame: &mut Frame, area: Rect, message: &str) {
    frame.render_widget(
        Paragraph::new(message.to_owned())
            .alignment(Alignment::Center)
            .style(Style::default().fg(OVERLAY1)),
        area,
    );
}

fn draw_breadcrumb(frame: &mut Frame, area: Rect, app: &AppState) {
    let title = app
        .selected_series()
        .map(|series| series.title.as_str())
        .unwrap_or("Series");
    let line = Line::from(vec![
        Span::raw(" "),
        Span::styled("Library", Style::default().fg(OVERLAY1)),
        Span::styled(" › ", Style::default().fg(OVERLAY0)),
        Span::styled(
            title.to_owned(),
            Style::default().fg(LAVENDER).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("  {} chapters", app.chapter_count()),
            Style::default().fg(OVERLAY0),
        ),
        Span::styled("   ↵ open · Esc back", Style::default().fg(OVERLAY0)),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

fn draw_chapter_list(
    frame: &mut Frame,
    area: Rect,
    series_title: &str,
    chapters: &[Chapter],
    progress: &ProgressStore,
    list_state: &mut ListState,
    selected: usize,
) {
    let items: Vec<ListItem> = chapters
        .iter()
        .map(|chapter| {
            let mut spans = vec![
                Span::raw(chapter.title.clone()),
                Span::styled(
                    format!("  {}", chapter_number(chapter.number)),
                    Style::default().fg(OVERLAY0),
                ),
            ];
            if let Some(entry) = progress.entry(&chapter.path) {
                spans.push(Span::styled(
                    format!("  {}/{}", entry.page + 1, entry.pages),
                    Style::default().fg(TEAL),
                ));
            }
            ListItem::new(Line::from(spans))
        })
        .collect();

    let count = chapters.len();
    if count == 0 {
        list_state.select(None);
    } else {
        list_state.select(Some(selected.min(count - 1)));
    }

    let title = Line::from(vec![
        Span::styled(
            format!(" {series_title} · Chapters "),
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

fn chapter_number(number: Option<f32>) -> String {
    match number {
        Some(number) if number.fract() == 0.0 => format!("{}", number as i64),
        Some(number) => format!("{number}"),
        None => "—".to_owned(),
    }
}

fn chapter_kind_label(kind: ChapterKind) -> &'static str {
    match kind {
        ChapterKind::Directory => "Folder",
        ChapterKind::Zip => "CBZ/ZIP",
        ChapterKind::Rar => "RAR/CBR",
        ChapterKind::Epub => "EPUB",
    }
}

fn draw_series_list(
    frame: &mut Frame,
    area: Rect,
    library: &Library,
    category: Option<&str>,
    list_state: &mut ListState,
    selected: usize,
) {
    let series: Vec<&Series> = library
        .series
        .iter()
        .filter(|series| in_category(series, category))
        .collect();

    let items: Vec<ListItem> = series
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

    let count = series.len();
    if count == 0 {
        list_state.select(None);
    } else {
        list_state.select(Some(selected.min(count - 1)));
    }

    let category_name = category.unwrap_or("All");
    let title = Line::from(vec![
        Span::styled(
            format!(" Library · {category_name} "),
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

fn in_category(series: &Series, category: Option<&str>) -> bool {
    category.is_none_or(|category| series.categories.iter().any(|item| item == category))
}

fn draw_details(frame: &mut Frame, area: Rect, app: &AppState) {
    let lines = match app.view {
        LibraryView::Series => match app.selected_series() {
            Some(series) => series_lines(series),
            None => empty_lines(app),
        },
        LibraryView::Chapters | LibraryView::Reader => chapter_lines(app),
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

fn chapter_lines(app: &AppState) -> Vec<Line<'static>> {
    let Some(chapter) = app.selected_chapter() else {
        let mut lines = vec![Line::from(Span::styled(
            "No chapters found.",
            Style::default().fg(SUBTEXT1),
        ))];
        if let Some(status) = &app.status {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                status.clone(),
                Style::default().fg(PEACH),
            )));
        }
        return lines;
    };

    let series = app
        .selected_series()
        .map(|series| series.title.clone())
        .unwrap_or_default();
    let mut lines = vec![
        Line::from(Span::styled(
            chapter.title.clone(),
            Style::default().fg(LAVENDER).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        field("Series", &series, TEXT),
        field("Number", &chapter_number(chapter.number), GREEN),
        field("Type", chapter_kind_label(chapter.kind), BLUE),
        field("Path", &chapter.path.display().to_string(), OVERLAY1),
    ];

    if let Some(entry) = app.progress.entry(&chapter.path) {
        lines.push(field(
            "Progress",
            &format!("{}/{}", entry.page + 1, entry.pages),
            TEAL,
        ));
    }

    if let Some(status) = &app.status {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            status.clone(),
            Style::default().fg(PEACH),
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

fn draw_more(frame: &mut Frame, area: Rect, app: &AppState) {
    let layout = more_layout(area);
    draw_more_sections(frame, layout.sections, app);

    match app.more_section {
        MoreSection::Settings => draw_more_settings(frame, layout.content, app),
        MoreSection::Downloads => placeholder(
            frame,
            layout.content,
            "Downloads",
            "Download management is not implemented yet.",
        ),
        MoreSection::Categories => draw_more_categories(frame, layout.content, app),
        MoreSection::Statistics => draw_more_statistics(frame, layout.content, app),
        MoreSection::Storage => draw_more_storage(frame, layout.content, app),
        MoreSection::About => draw_more_about(frame, layout.content, app),
    }
}

fn draw_more_sections(frame: &mut Frame, area: Rect, app: &AppState) {
    let focused = app.more_focus == MoreFocus::Sections;
    let items: Vec<ListItem> = MORE_SECTIONS
        .iter()
        .map(|section| {
            let style = if *section == app.more_section {
                Style::default()
                    .fg(LAVENDER)
                    .bg(if focused { SURFACE0 } else { BASE })
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(SUBTEXT1)
            };
            ListItem::new(format!(" {} ", section.title())).style(style)
        })
        .collect();

    let list = List::new(items).block(panel(
        Line::from(Span::styled(
            " More ",
            Style::default().fg(LAVENDER).add_modifier(Modifier::BOLD),
        )),
        if focused { LAVENDER } else { SURFACE1 },
    ));
    frame.render_widget(list, area);
}

fn draw_more_settings(frame: &mut Frame, area: Rect, app: &AppState) {
    let block = panel(
        Line::from(Span::styled(
            " Settings ",
            Style::default().fg(LAVENDER).add_modifier(Modifier::BOLD),
        )),
        more_content_border(app),
    );
    frame.render_widget(&block, area);

    let rows = settings_rows(area, MORE_SETTINGS_ROWS.len());
    for (row_index, entry) in MORE_SETTINGS_ROWS.iter().enumerate() {
        let Some(row) = rows.get(row_index).copied() else {
            break;
        };
        match entry {
            MoreRow::Divider(label) => draw_settings_divider(frame, row, label),
            MoreRow::Setting(index) => draw_setting_row(frame, row, *index, app),
        }
    }

    let inner_area = block.inner(area);
    if inner_area.height > MORE_SETTINGS_ROWS.len() as u16 + 1 {
        let hint = Rect::new(
            inner_area.x,
            inner_area.y + inner_area.height - 1,
            inner_area.width,
            1,
        );
        let text = if app.more_focus == MoreFocus::Sections {
            "↑↓ section · →/Enter select · Esc back"
        } else {
            "↑↓ setting · +/- change · ← sections · Esc back"
        };
        frame.render_widget(
            Paragraph::new(text).style(Style::default().fg(OVERLAY0)),
            hint,
        );
    }
}

fn draw_settings_divider(frame: &mut Frame, row: Rect, label: &str) {
    let width = row.width as usize;
    let text = format!(" {label} ");
    let text_width = text.chars().count();
    if width <= text_width + 2 {
        frame.render_widget(
            Paragraph::new(text).style(Style::default().fg(OVERLAY0)),
            row,
        );
        return;
    }

    let remaining = width - text_width;
    let left = remaining / 2;
    let right = remaining - left;
    let line = Line::from(vec![
        Span::styled("─".repeat(left), Style::default().fg(OVERLAY0)),
        Span::styled(text, Style::default().fg(OVERLAY0)),
        Span::styled("─".repeat(right), Style::default().fg(OVERLAY0)),
    ]);
    frame.render_widget(Paragraph::new(line), row);
}

fn draw_setting_row(frame: &mut Frame, row: Rect, index: usize, app: &AppState) {
    let selected = app.more_focus == MoreFocus::Content && index == app.setting_index;
    if selected {
        frame.render_widget(Block::default().style(Style::default().bg(SURFACE0)), row);
    }

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(20),
            Constraint::Min(10),
            Constraint::Length(8),
        ])
        .split(row);

    let label_style = if selected {
        Style::default().fg(LAVENDER).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(SUBTEXT1)
    };
    let value_style = if selected {
        Style::default().fg(LAVENDER)
    } else {
        Style::default().fg(OVERLAY1)
    };

    let (label, value) = setting_text(app, index);
    frame.render_widget(Paragraph::new(label).style(label_style), columns[0]);
    frame.render_widget(
        Paragraph::new(value)
            .alignment(Alignment::Right)
            .style(value_style),
        columns[2],
    );

    if let Some((ratio, _)) = setting_slider(app, index) {
        frame.render_widget(
            Gauge::default()
                .ratio(ratio)
                .label("")
                .gauge_style(Style::default().fg(LAVENDER).bg(SURFACE0))
                .style(Style::default().bg(SURFACE0)),
            columns[1],
        );
    }
}

fn more_content_border(app: &AppState) -> Color {
    if app.more_focus == MoreFocus::Content {
        LAVENDER
    } else {
        SURFACE1
    }
}

fn setting_text(app: &AppState, index: usize) -> (String, String) {
    match index {
        0 => (
            "Reading mode".to_owned(),
            reading_mode_label(app.config.reading_mode).to_owned(),
        ),
        1 => ("Zoom".to_owned(), format!("{}%", app.zoom)),
        2 => (
            "Prefetch behind".to_owned(),
            app.scroll_cache.prefetch_behind.to_string(),
        ),
        3 => (
            "Prefetch ahead".to_owned(),
            app.scroll_cache.prefetch_ahead.to_string(),
        ),
        4 => (
            "Cache margin".to_owned(),
            app.scroll_cache.cache_margin.to_string(),
        ),
        5 => (
            "Library path".to_owned(),
            app.library.root.display().to_string(),
        ),
        6 => ("Theme".to_owned(), "Catppuccin Mocha".to_owned()),
        _ => (String::new(), String::new()),
    }
}

fn setting_slider(app: &AppState, index: usize) -> Option<(f64, (u16, u16))> {
    match index {
        1 => Some(((f64::from(app.zoom) - 50.0) / 250.0, (50, 300))),
        2 => Some((app.scroll_cache.prefetch_behind as f64 / 8.0, (0, 8))),
        3 => Some((app.scroll_cache.prefetch_ahead as f64 / 8.0, (0, 8))),
        4 => Some((app.scroll_cache.cache_margin as f64 / 12.0, (0, 12))),
        _ => None,
    }
}

fn reading_mode_label(mode: ReadingMode) -> &'static str {
    match mode {
        ReadingMode::Default => "Default",
        ReadingMode::LeftToRight => "Left to right",
        ReadingMode::RightToLeft => "Right to left",
        ReadingMode::Vertical => "Vertical",
        ReadingMode::Webtoon => "Webtoon",
        ReadingMode::ContinuousVertical => "Long strip + gaps",
    }
}

fn draw_more_categories(frame: &mut Frame, area: Rect, app: &AppState) {
    let mut lines = Vec::new();
    for category in &app.library.categories {
        let count = app
            .library
            .series
            .iter()
            .filter(|series| series.categories.iter().any(|name| name == category))
            .count();
        lines.push(field(category, &format!("{count} series"), TEXT));
    }
    if lines.is_empty() {
        lines.push(Line::from("No categories found."));
    }

    let paragraph = Paragraph::new(Text::from(lines))
        .block(panel(
            Line::from(Span::styled(
                " Categories ",
                Style::default().fg(LAVENDER).add_modifier(Modifier::BOLD),
            )),
            more_content_border(app),
        ))
        .style(Style::default().fg(SUBTEXT1))
        .wrap(Wrap { trim: true });
    frame.render_widget(paragraph, area);
}

fn draw_more_statistics(frame: &mut Frame, area: Rect, app: &AppState) {
    let chapters: usize = app
        .library
        .series
        .iter()
        .map(|series| series.chapters.len())
        .sum();
    let lines = vec![
        field("Series", &app.library.series.len().to_string(), GREEN),
        field("Chapters", &chapters.to_string(), GREEN),
        field(
            "Categories",
            &app.library.categories.len().to_string(),
            GREEN,
        ),
        field("Scan errors", &app.library.errors.len().to_string(), PEACH),
    ];
    let paragraph = Paragraph::new(Text::from(lines))
        .block(panel(
            Line::from(Span::styled(
                " Statistics ",
                Style::default().fg(LAVENDER).add_modifier(Modifier::BOLD),
            )),
            more_content_border(app),
        ))
        .style(Style::default().fg(SUBTEXT1));
    frame.render_widget(paragraph, area);
}

fn draw_more_storage(frame: &mut Frame, area: Rect, app: &AppState) {
    let lines = vec![
        field("Library", &app.library.root.display().to_string(), TEXT),
        field(
            "Categories",
            &app.library
                .root
                .join("categories.json")
                .display()
                .to_string(),
            OVERLAY1,
        ),
        field("Progress", "XDG data / tsuiyomi/progress.json", OVERLAY1),
    ];
    let paragraph = Paragraph::new(Text::from(lines))
        .block(panel(
            Line::from(Span::styled(
                " Data and storage ",
                Style::default().fg(LAVENDER).add_modifier(Modifier::BOLD),
            )),
            more_content_border(app),
        ))
        .style(Style::default().fg(SUBTEXT1))
        .wrap(Wrap { trim: true });
    frame.render_widget(paragraph, area);
}

fn draw_more_about(frame: &mut Frame, area: Rect, app: &AppState) {
    let mode = match app.reader_mode {
        ReaderMode::Page => "page",
        ReaderMode::Scroll => "scroll",
    };
    let lines = vec![
        field("Version", env!("CARGO_PKG_VERSION"), TEXT),
        field("Graphics", app.protocol_name(), BLUE),
        field("Reader", mode, BLUE),
        field("Zoom", &format!("{}%", app.zoom), GREEN),
        Line::from(""),
        Line::from(Span::styled(
            "Tsuiyomi is an independent Mihon-compatible TUI.",
            Style::default().fg(OVERLAY1),
        )),
    ];
    let paragraph = Paragraph::new(Text::from(lines))
        .block(panel(
            Line::from(Span::styled(
                " About ",
                Style::default().fg(LAVENDER).add_modifier(Modifier::BOLD),
            )),
            more_content_border(app),
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
    use tsuiyomi_core::{Chapter, ChapterKind, Library, Page, PageSource, Series};

    use crate::app::ReaderState;

    use super::*;

    fn test_app() -> AppState {
        let series = Series {
            path: PathBuf::from("/tmp/library/Test Series"),
            title: "Test Series".to_owned(),
            author: Some("Author".to_owned()),
            artist: None,
            description: "A test description.".to_owned(),
            tags: vec!["fantasy".to_owned()],
            categories: vec!["Default".to_owned()],
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
            categories: vec!["Default".to_owned()],
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
        assert!(text.contains("rescan"));
        assert!(text.contains("open"));
        assert!(text.contains("┬"));
        assert!(text.contains("┴"));
        assert!(text.contains("History"));
        assert!(text.contains("Browse"));
        assert!(text.contains("More"));
        assert!(text.contains("Default"));
        assert!(text.contains("[ ]"));
        assert!(text.contains("Library · Default"));
        assert!(!text.contains("1 Library"));
        assert!(text.contains("Test Series"));
        assert!(text.contains("Details"));
    }

    #[test]
    fn chapter_view_renders_the_chapter_list() {
        let mut app = test_app();
        app.open();
        let text = render(&mut app, 80, 24);

        assert!(text.contains("Chapters"));
        assert!(text.contains("Chapter 1"));
        assert!(text.contains("Folder"));
        assert!(text.contains("Esc back"));
    }

    #[test]
    fn reader_view_renders_status_and_error() {
        let mut app = test_app();
        app.view = LibraryView::Reader;
        app.reader = Some(ReaderState {
            series_title: "Test Series".to_owned(),
            chapter_title: "Chapter 1".to_owned(),
            chapter_path: PathBuf::from("/tmp/library/Chapter 1"),
            pages: vec![Page {
                name: "1.png".to_owned(),
                source: PageSource::File(PathBuf::from("/tmp/1.png")),
            }],
            page_sizes: Vec::new(),
            page_index: 0,
            protocol: None,
            scroll: None,
            pending_scroll: None,
            error: Some("Reader error".to_owned()),
        });

        let text = render(&mut app, 80, 24);

        assert!(text.contains("1 / 1"));
        assert!(text.contains("Reader error"));
        assert!(text.contains("page"));
        assert!(text.contains("prev"));
        assert!(text.contains("next"));
        assert!(text.contains("Esc back"));
        assert!(!text.contains("Browse"));
    }

    #[test]
    fn more_settings_renders() {
        let mut app = test_app();
        app.tab = 4;
        let text = render(&mut app, 80, 30);

        assert!(text.contains("More"));
        assert!(text.contains("Settings"));
        assert!(text.contains("Reading mode"));
        assert!(text.contains("Zoom"));
        assert!(text.contains("Prefetch behind"));
        assert!(text.contains("Reader"));
        assert!(text.contains("Library"));
        assert!(text.contains("Appearance"));
        assert!(text.contains("Data and storage"));
    }

    #[test]
    fn selected_tab_uses_lavender_text_without_background() {
        let mut app = test_app();
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();

        let layout = app_layout(Rect::new(0, 0, 80, 24), false);
        let tabs = tab_layouts(layout.tabs);
        let buffer = terminal.backend().buffer();

        let selected = tabs[0].area;
        let selected_cell = &buffer[(selected.x + selected.width / 2, selected.y)];
        assert_eq!(selected_cell.style().fg, Some(LAVENDER));
        assert_eq!(selected_cell.style().bg, Some(BASE));

        let inactive = tabs[1].area;
        let inactive_cell = &buffer[(inactive.x + inactive.width / 2, inactive.y)];
        assert_eq!(inactive_cell.style().fg, Some(OVERLAY1));
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
