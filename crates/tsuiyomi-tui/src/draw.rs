use std::path::PathBuf;

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect, Size},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Gauge, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};
use ratatui_image::{
    protocol::StatefulProtocol,
    sliced::{SignedPosition, SlicedImage},
    Resize, StatefulImage,
};
use tsuiyomi_core::{
    Chapter, ChapterDate, ChapterKind, LibrarySort, ProgressStore, ReadingMode, Series, Theme,
};

use crate::{
    app::{
        AppState, BrowseView, LibraryView, MoreFocus, MoreRow, MoreSection, PickerKind,
        PickerState, ReaderMode, SettingId, SettingsPage, MORE_SECTIONS, SETTINGS_PAGES,
    },
    layout::{
        app_layout, grid_columns, grid_row_offset, grid_tile_rect, grid_visible_rows,
        library_grid_area, library_layout, more_layout, reader_bar_layout, setting_columns,
        settings_rows, tab_layouts, tile_cover_height, TAB_NAMES, TILE_TEXT_HEIGHT,
    },
    palette::{
        accent_choices, c_accent, c_base, c_blue, c_green, c_mauve, c_overlay0, c_overlay1,
        c_peach, c_subtext1, c_surface0, c_surface1, c_teal, c_text, c_yellow, palette_for,
        parse_hex, theme_label, THEMES,
    },
};

pub fn draw(frame: &mut Frame, app: &mut AppState) {
    let area = frame.area();
    frame.render_widget(Block::default().style(Style::default().bg(c_base())), area);

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
        3 => {
            if app.view == LibraryView::Series {
                draw_browse(frame, layout.content, app);
            } else {
                draw_library(frame, layout.content, app);
            }
        }
        _ => draw_more(frame, layout.content, app),
    }

    if app.view == LibraryView::Reader && (app.tab == 0 || app.tab == 3) {
        draw_reader_bar(frame, layout.tabs, app);
    } else {
        draw_tabs(frame, layout.tabs, app.tab);
    }

    if let Some(commands) = layout.commands {
        draw_commands(frame, commands, app);
    }

    draw_picker(frame, app);
}

fn draw_header(frame: &mut Frame, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(c_surface1()))
        .style(Style::default().bg(c_base()));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    frame.render_widget(
        Paragraph::new("Tsuiyomi")
            .alignment(Alignment::Center)
            .style(Style::default().fg(c_accent()).add_modifier(Modifier::BOLD)),
        inner,
    );
}

fn draw_reader_bar(frame: &mut Frame, area: Rect, app: &AppState) {
    let block = panel(
        Line::from(Span::styled(" reader ", Style::default().fg(c_overlay1()))),
        c_surface1(),
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
            .gauge_style(Style::default().fg(c_accent()).bg(c_surface0()))
            .style(Style::default().fg(c_text())),
        layout.progress,
    );

    let (mode_key, mode_label) = match app.reader_mode {
        ReaderMode::Page => ("m", "scroll"),
        ReaderMode::Scroll => ("m", "page"),
    };
    let mode = Line::from(vec![
        Span::styled(
            mode_key,
            Style::default().fg(c_accent()).add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!(" {mode_label}"), Style::default().fg(c_overlay1())),
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
        Style::default().fg(c_accent()).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(c_overlay0())
    };
    let label_style = if enabled {
        Style::default().fg(c_text())
    } else {
        Style::default().fg(c_overlay0())
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
        .border_style(Style::default().fg(c_surface1()))
        .style(Style::default().bg(c_base()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let layouts = tab_layouts(area);
    let mut spans = Vec::new();
    for (position, tab) in layouts.iter().enumerate() {
        if position > 0 {
            spans.push(Span::styled("│", Style::default().fg(c_surface1())));
        }

        let label = TAB_NAMES[tab.index];
        let text = center_label(label, tab.area.width);
        let style = if tab.index == active {
            Style::default().fg(c_accent()).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(c_overlay1())
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
        Line::from(Span::styled(
            " commands ",
            Style::default().fg(c_overlay1()),
        )),
        c_surface1(),
    );
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let items: &[(&str, &str)] =
        if app.view == LibraryView::Reader && (app.tab == 0 || app.tab == 3) {
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
        } else if app.tab == 3 && app.view == LibraryView::Chapters {
            &[
                ("q", "quit"),
                ("↑↓", "chapter"),
                ("↵", "read"),
                ("Esc", "back"),
            ]
        } else if app.tab == 3 {
            &[
                ("q", "quit"),
                ("↑↓", "move"),
                ("↵", "open"),
                ("r", "rescan"),
                ("Esc", "back"),
            ]
        } else if app.tab == 4 {
            if app.more_focus == MoreFocus::Sections {
                &[
                    ("q", "quit"),
                    ("↑↓", "section"),
                    ("↵", "enter"),
                    ("Esc", "back"),
                ]
            } else if app.more_section == MoreSection::Settings
                && app.settings_page == SettingsPage::Main
            {
                &[
                    ("q", "quit"),
                    ("↑↓", "category"),
                    ("↵", "open"),
                    ("Esc", "back"),
                ]
            } else {
                &[
                    ("q", "quit"),
                    ("↑↓", "setting"),
                    ("←→", "change"),
                    ("↵", "action"),
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
            spans.push(Span::styled(" · ", Style::default().fg(c_overlay0())));
        }
        spans.push(Span::styled(
            *key,
            Style::default().fg(c_accent()).add_modifier(Modifier::BOLD),
        ));
        if !label.is_empty() {
            spans.push(Span::styled(
                format!(" {label}"),
                Style::default().fg(c_overlay0()),
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
            let indices = app.visible_indices();
            let (columns, offset) = draw_series_grid(
                frame,
                library_grid_area(area),
                app,
                &indices,
                app.selected,
                app.library_grid_offset,
            );
            app.library_columns = columns;
            app.library_grid_offset = offset;
            return;
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

fn draw_series_grid(
    frame: &mut Frame,
    area: Rect,
    app: &mut AppState,
    indices: &[usize],
    selected: usize,
    current_offset: usize,
) -> (usize, usize) {
    let columns = grid_columns(area, app.config.thumbnail_width);
    let cover_height = tile_cover_height(app.config.thumbnail_width, area.height);
    let tile_height = cover_height + TILE_TEXT_HEIGHT;
    let selected = selected.min(indices.len().saturating_sub(1));
    if indices.is_empty() {
        frame.render_widget(
            Paragraph::new("No series found.")
                .alignment(Alignment::Center)
                .style(Style::default().fg(c_overlay1())),
            Rect::new(area.x, area.y, area.width, area.height.min(1)),
        );
        if let Some(status) = &app.status {
            if area.height > 1 {
                frame.render_widget(
                    Paragraph::new(status.clone())
                        .alignment(Alignment::Center)
                        .style(Style::default().fg(c_peach())),
                    Rect::new(area.x, area.y + 1, area.width, 1),
                );
            }
        }
        return (columns, 0);
    }
    let visible_rows = grid_visible_rows(area, tile_height);
    let offset = grid_row_offset(
        area,
        columns,
        selected,
        current_offset,
        indices.len(),
        tile_height,
    );
    let start = offset.saturating_mul(columns);
    let end = (offset.saturating_add(visible_rows))
        .saturating_mul(columns)
        .min(indices.len());

    let covers: Vec<PathBuf> = indices[start..end]
        .iter()
        .filter_map(|index| app.library.series[*index].cover.clone())
        .collect();
    for cover in covers {
        app.ensure_tile_cover(&cover);
    }

    for (position, series_index) in indices[start..end].iter().enumerate() {
        let index = start + position;
        let series = &app.library.series[*series_index];
        let tile = grid_tile_rect(area, columns, offset, index, tile_height);
        let is_selected = index == selected;
        if is_selected {
            frame.render_widget(
                Block::default().style(Style::default().bg(c_surface0())),
                tile,
            );
        }
        let cover_area = Rect::new(tile.x, tile.y, tile.width, cover_height);
        let mut rendered_cover = false;
        if let Some(path) = &series.cover {
            if let Some(protocol) = app.tile_covers.get_mut(path) {
                render_centered_image(frame, cover_area, protocol);
                rendered_cover = true;
            }
        }
        if !rendered_cover {
            let initial = series
                .title
                .chars()
                .next()
                .map(|character| character.to_uppercase().collect::<String>())
                .unwrap_or_default();
            if cover_area.height > 0 {
                frame.render_widget(
                    Paragraph::new(initial)
                        .alignment(Alignment::Center)
                        .style(Style::default().fg(c_accent()).add_modifier(Modifier::BOLD)),
                    Rect::new(
                        cover_area.x,
                        cover_area.y + cover_area.height / 2,
                        cover_area.width,
                        1,
                    ),
                );
            }
        }

        let title_area = Rect::new(tile.x, tile.y + cover_height, tile.width, TILE_TEXT_HEIGHT);
        let title_style = if is_selected {
            Style::default().fg(c_accent()).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(c_text())
        };
        let count_area = Rect::new(title_area.x, title_area.y + 1, title_area.width, 1);
        let title = truncate_text(&series.title, title_area.width);
        frame.render_widget(
            Paragraph::new(title)
                .alignment(Alignment::Center)
                .style(title_style),
            Rect::new(title_area.x, title_area.y, title_area.width, 1),
        );
        frame.render_widget(
            Paragraph::new(format!("{} ch", series.chapters.len()))
                .alignment(Alignment::Center)
                .style(Style::default().fg(c_overlay0())),
            count_area,
        );
    }

    (columns, offset)
}

fn render_centered_image(frame: &mut Frame, area: Rect, protocol: &mut StatefulProtocol) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let resize = Resize::Fit(None);
    let size = protocol.size_for(resize.clone(), Size::new(area.width, area.height));
    let image_area = Rect::new(
        area.x + area.width.saturating_sub(size.width) / 2,
        area.y + area.height.saturating_sub(size.height) / 2,
        size.width.min(area.width),
        size.height.min(area.height),
    );
    frame.render_stateful_widget(StatefulImage::new().resize(resize), image_area, protocol);
}

fn truncate_text(text: &str, width: u16) -> String {
    let width = width as usize;
    if text.chars().count() <= width {
        return text.to_owned();
    }
    let mut truncated: String = text.chars().take(width.saturating_sub(1)).collect();
    truncated.push('…');
    truncated
}

fn draw_categories(frame: &mut Frame, area: Rect, app: &AppState) {
    let mut spans = vec![Span::raw(" ")];
    for (index, name) in app.library.categories.iter().enumerate() {
        if index > 0 {
            spans.push(Span::styled(" · ", Style::default().fg(c_overlay0())));
        }
        let count = app
            .library
            .series
            .iter()
            .filter(|series| in_category(series, Some(name)))
            .count();
        let style = if index == app.category_index {
            Style::default().fg(c_accent()).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(c_overlay1())
        };
        spans.push(Span::styled(name.clone(), style));
        spans.push(Span::styled(
            format!(" {count}"),
            Style::default().fg(c_overlay0()),
        ));
    }
    spans.push(Span::styled(
        "   [ ] categories",
        Style::default().fg(c_overlay0()),
    ));

    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn draw_browse(frame: &mut Frame, area: Rect, app: &mut AppState) {
    draw_browse_path(frame, Rect::new(area.x, area.y, area.width, 1), app);
    let content = library_grid_area(area);
    match app.browse_view {
        BrowseView::Sources => draw_browse_sources(frame, content, app),
        BrowseView::Local => {
            let indices = app.browse_local_indices();
            let (columns, offset) = draw_series_grid(
                frame,
                content,
                app,
                &indices,
                app.browse_selection,
                app.browse_grid_offset,
            );
            app.browse_columns = columns;
            app.browse_grid_offset = offset;
        }
        BrowseView::Details => draw_browse_detail(frame, content, app),
    }
}

fn draw_browse_path(frame: &mut Frame, area: Rect, app: &AppState) {
    let (view, count) = match app.browse_view {
        BrowseView::Sources => ("Sources", "1 source".to_owned()),
        BrowseView::Local | BrowseView::Details => (
            "Local source",
            format!("{} series", app.library.series.len()),
        ),
    };
    let line = Line::from(vec![
        Span::styled(
            format!(" Browse · {view} "),
            Style::default().fg(c_accent()).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            display_path(&app.library.root),
            Style::default().fg(c_text()),
        ),
        Span::styled(format!("  {count}"), Style::default().fg(c_overlay0())),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

fn draw_browse_sources(frame: &mut Frame, area: Rect, app: &mut AppState) {
    app.browse_list_state.select(Some(0));
    let item = ListItem::new(Line::from(vec![
        Span::styled("Local source", Style::default().fg(c_text())),
        Span::styled(
            format!("  {} series", app.library.series.len()),
            Style::default().fg(c_overlay0()),
        ),
    ]));
    let list = List::new([item])
        .block(panel(
            Line::from(Span::styled(
                " Sources ",
                Style::default().fg(c_accent()).add_modifier(Modifier::BOLD),
            )),
            c_accent(),
        ))
        .highlight_style(
            Style::default()
                .fg(c_accent())
                .bg(c_surface0())
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("› ");
    frame.render_stateful_widget(list, area, &mut app.browse_list_state);
}

fn draw_browse_detail(frame: &mut Frame, area: Rect, app: &mut AppState) {
    let Some(cover) = app
        .browse_detail_series()
        .map(|series| series.cover.clone())
    else {
        draw_browse_empty(frame, area, "Series not found. Rescan the library.");
        return;
    };
    app.ensure_cover_path(cover);

    let Some(series) = app.browse_detail_series() else {
        draw_browse_empty(frame, area, "Series not found. Rescan the library.");
        return;
    };
    let cover_initial = series
        .title
        .chars()
        .next()
        .map(|character| character.to_uppercase().collect::<String>())
        .unwrap_or_default();
    let mut lines = series_lines(series);
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Enter opens chapters · Esc goes back.",
        Style::default().fg(c_overlay0()),
    )));

    let block = panel(
        Line::from(Span::styled(
            " Series ",
            Style::default().fg(c_accent()).add_modifier(Modifier::BOLD),
        )),
        c_accent(),
    );
    let inner = block.inner(area);
    frame.render_widget(&block, area);

    let (cover_area, text_area) = if inner.width >= 30 {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(inner.width / 3), Constraint::Min(0)])
            .split(inner);
        (Some(chunks[0]), chunks[1])
    } else {
        (None, inner)
    };

    if let Some(cover_area) = cover_area {
        if let Some(protocol) = app.cover.as_mut() {
            render_centered_image(frame, cover_area, protocol);
        } else if cover_area.height > 0 {
            frame.render_widget(
                Paragraph::new(cover_initial)
                    .alignment(Alignment::Center)
                    .style(Style::default().fg(c_accent()).add_modifier(Modifier::BOLD)),
                Rect::new(
                    cover_area.x,
                    cover_area.y + cover_area.height / 2,
                    cover_area.width,
                    1,
                ),
            );
        }
    }

    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .style(Style::default().fg(c_subtext1()))
            .wrap(Wrap { trim: true }),
        text_area,
    );
}

fn draw_browse_empty(frame: &mut Frame, area: Rect, message: &str) {
    let paragraph = Paragraph::new(message)
        .block(panel(
            Line::from(Span::styled(
                " Series ",
                Style::default().fg(c_accent()).add_modifier(Modifier::BOLD),
            )),
            c_surface1(),
        ))
        .alignment(Alignment::Center)
        .style(Style::default().fg(c_overlay1()));
    frame.render_widget(paragraph, area);
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
            Style::default().fg(c_accent()).add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!("   [{detail}]"), Style::default().fg(c_overlay0())),
    ]);
    frame.render_widget(Paragraph::new(status), chunks[0]);

    if let Some(error) = &reader.error {
        frame.render_widget(
            Paragraph::new(error.clone())
                .alignment(Alignment::Center)
                .style(Style::default().fg(c_peach())),
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
            .style(Style::default().fg(c_overlay1())),
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
        Span::styled("Library", Style::default().fg(c_overlay1())),
        Span::styled(" › ", Style::default().fg(c_overlay0())),
        Span::styled(
            title.to_owned(),
            Style::default().fg(c_accent()).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("  {} chapters", app.chapter_count()),
            Style::default().fg(c_overlay0()),
        ),
        Span::styled("   ↵ open · Esc back", Style::default().fg(c_overlay0())),
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
                    Style::default().fg(c_overlay0()),
                ),
            ];
            if let Some(entry) = progress.entry(&chapter.path) {
                spans.push(Span::styled(
                    format!("  {}/{}", entry.page + 1, entry.pages),
                    Style::default().fg(c_teal()),
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
            Style::default().fg(c_accent()).add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!("{count} "), Style::default().fg(c_overlay0())),
    ]);

    let list = List::new(items)
        .block(panel(title, c_accent()))
        .style(Style::default().fg(c_text()))
        .highlight_style(
            Style::default()
                .fg(c_accent())
                .bg(c_surface0())
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

fn chapter_date(date: ChapterDate) -> String {
    format!("{:04}-{:02}-{:02}", date.year, date.month, date.day)
}

fn chapter_kind_label(kind: ChapterKind) -> &'static str {
    match kind {
        ChapterKind::Directory => "Folder",
        ChapterKind::Zip => "CBZ/ZIP",
        ChapterKind::Rar => "RAR/CBR",
        ChapterKind::SevenZip => "CB7/7Z",
        ChapterKind::Tar => "CBT/TAR",
        ChapterKind::Epub => "EPUB",
    }
}

fn in_category(series: &Series, category: Option<&str>) -> bool {
    category.is_none_or(|category| series.categories.iter().any(|item| item == category))
}

fn draw_details(frame: &mut Frame, area: Rect, app: &AppState) {
    let lines = chapter_lines(app);
    let paragraph = Paragraph::new(Text::from(lines))
        .block(panel(
            Line::from(Span::styled(
                " Details ",
                Style::default().fg(c_blue()).add_modifier(Modifier::BOLD),
            )),
            c_surface1(),
        ))
        .style(Style::default().fg(c_subtext1()))
        .wrap(Wrap { trim: true });
    frame.render_widget(paragraph, area);
}

fn series_lines(series: &Series) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from(Span::styled(
            series.title.clone(),
            Style::default().fg(c_accent()).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
    ];

    if let Some(author) = &series.author {
        lines.push(field("Author", author, c_peach()));
    }
    if let Some(artist) = &series.artist {
        lines.push(field("Artist", artist, c_mauve()));
    }
    if let Some(status) = &series.status {
        lines.push(field("Status", status, c_green()));
    }
    if !series.tags.is_empty() {
        lines.push(field("Tags", &series.tags.join(", "), c_teal()));
    }
    lines.push(field(
        "Chapters",
        &series.chapters.len().to_string(),
        c_green(),
    ));
    if let Some(latest) = series.chapters.first() {
        lines.push(field("Latest", &latest.title, c_blue()));
    }

    lines.push(Line::from(""));
    if series.description.trim().is_empty() {
        lines.push(Line::from(Span::styled(
            "No description.",
            Style::default().fg(c_overlay1()),
        )));
    } else {
        lines.push(Line::from(series.description.clone()));
    }

    if !series.warnings.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!("{} metadata warning(s)", series.warnings.len()),
            Style::default().fg(c_yellow()),
        )));
    }

    lines
}

fn chapter_lines(app: &AppState) -> Vec<Line<'static>> {
    let Some(chapter) = app.selected_chapter() else {
        let mut lines = vec![Line::from(Span::styled(
            "No chapters found.",
            Style::default().fg(c_subtext1()),
        ))];
        if let Some(status) = &app.status {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                status.clone(),
                Style::default().fg(c_peach()),
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
            Style::default().fg(c_accent()).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        field("Series", &series, c_text()),
        field("Number", &chapter_number(chapter.number), c_green()),
        field("Type", chapter_kind_label(chapter.kind), c_blue()),
        field("Path", &chapter.path.display().to_string(), c_overlay1()),
    ];
    if let Some(scanlator) = &chapter.scanlator {
        lines.push(field("Scanlator", scanlator, c_teal()));
    }
    if let Some(date) = chapter.date {
        lines.push(field("Date", &chapter_date(date), c_blue()));
    }

    if let Some(entry) = app.progress.entry(&chapter.path) {
        lines.push(field(
            "Progress",
            &format!("{}/{}", entry.page + 1, entry.pages),
            c_teal(),
        ));
    }

    if let Some(status) = &app.status {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            status.clone(),
            Style::default().fg(c_peach()),
        )));
    }

    lines
}

fn field(label: &str, value: &str, value_color: Color) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{label:<9}"), Style::default().fg(c_overlay1())),
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
        MoreSection::Help => draw_more_help(frame, layout.content, app),
    }
}

fn draw_more_sections(frame: &mut Frame, area: Rect, app: &AppState) {
    let focused = app.more_focus == MoreFocus::Sections;
    let items: Vec<ListItem> = MORE_SECTIONS
        .iter()
        .map(|section| {
            let style = if *section == app.more_section {
                Style::default()
                    .fg(c_accent())
                    .bg(if focused { c_surface0() } else { c_base() })
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(c_subtext1())
            };
            ListItem::new(format!(" {} ", section.title())).style(style)
        })
        .collect();

    let list = List::new(items).block(panel(
        Line::from(Span::styled(
            " More ",
            Style::default().fg(c_accent()).add_modifier(Modifier::BOLD),
        )),
        if focused { c_accent() } else { c_surface1() },
    ));
    frame.render_widget(list, area);
}

fn draw_more_settings(frame: &mut Frame, area: Rect, app: &AppState) {
    match app.settings_page {
        SettingsPage::Main => draw_settings_menu(frame, area, app),
        SettingsPage::About => draw_more_about(frame, area, app),
        page => draw_settings_page(frame, area, app, page),
    }
}

fn draw_settings_menu(frame: &mut Frame, area: Rect, app: &AppState) {
    let block = settings_panel(" Settings ", app);
    frame.render_widget(&block, area);

    let rows = settings_rows(area, SETTINGS_PAGES.len());
    for (index, page) in SETTINGS_PAGES.iter().enumerate() {
        let Some(row) = rows.get(index).copied() else {
            break;
        };
        let selected = app.more_focus == MoreFocus::Content && index == app.setting_index;
        if selected {
            frame.render_widget(
                Block::default().style(Style::default().bg(c_surface0())),
                row,
            );
        }

        let columns = setting_columns(row);
        let label_style = if selected {
            Style::default().fg(c_accent()).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(c_subtext1())
        };
        let value_style = if selected {
            Style::default().fg(c_accent())
        } else {
            Style::default().fg(c_overlay0())
        };
        frame.render_widget(Paragraph::new(page.title()).style(label_style), columns[0]);
        let value_area = Rect::new(
            columns[1].x,
            row.y,
            columns[1].width.saturating_add(columns[2].width),
            1,
        );
        frame.render_widget(
            Paragraph::new(page.summary())
                .alignment(Alignment::Right)
                .style(value_style),
            value_area,
        );
    }

    settings_hint(
        frame,
        area,
        &block,
        SETTINGS_PAGES.len(),
        "↑↓ category · Enter open · Esc back",
    );
}

fn draw_settings_page(frame: &mut Frame, area: Rect, app: &AppState, page: SettingsPage) {
    let title = format!(" Settings · {} ", page.title());
    let block = settings_panel(&title, app);
    frame.render_widget(&block, area);

    let rows = settings_rows(area, page.rows().len());
    let mut selectable = 0;
    for (row_index, entry) in page.rows().iter().enumerate() {
        let Some(row) = rows.get(row_index).copied() else {
            break;
        };
        match entry {
            MoreRow::Divider(label) => draw_settings_divider(frame, row, label),
            MoreRow::Setting(setting) => {
                draw_setting_row(frame, row, *setting, selectable, app);
                selectable += 1;
            }
            MoreRow::Note(text) => draw_settings_note(frame, row, text),
        }
    }

    let text = if app.more_focus == MoreFocus::Sections {
        "↑↓ section · Enter settings · Esc back"
    } else {
        "↑↓ setting · ←→ change · ↵ action · Esc page"
    };
    settings_hint(frame, area, &block, page.rows().len(), text);
}

fn settings_panel(title: &str, app: &AppState) -> Block<'static> {
    panel(
        Line::from(Span::styled(
            title.to_owned(),
            Style::default().fg(c_accent()).add_modifier(Modifier::BOLD),
        )),
        more_content_border(app),
    )
}

fn settings_hint(frame: &mut Frame, area: Rect, block: &Block<'_>, rows: usize, text: &str) {
    let inner_area = block.inner(area);
    if inner_area.height > rows as u16 + 1 {
        let hint = Rect::new(
            inner_area.x,
            inner_area.y + inner_area.height - 1,
            inner_area.width,
            1,
        );
        frame.render_widget(
            Paragraph::new(text).style(Style::default().fg(c_overlay0())),
            hint,
        );
    }
}

fn draw_settings_note(frame: &mut Frame, row: Rect, text: &str) {
    frame.render_widget(
        Paragraph::new(format!(" {text}")).style(Style::default().fg(c_overlay0())),
        row,
    );
}

fn draw_settings_divider(frame: &mut Frame, row: Rect, label: &str) {
    let width = row.width as usize;
    let text = format!(" {label} ");
    let text_width = text.chars().count();
    if width <= text_width + 2 {
        frame.render_widget(
            Paragraph::new(text).style(Style::default().fg(c_overlay0())),
            row,
        );
        return;
    }

    let remaining = width - text_width;
    let left = remaining / 2;
    let right = remaining - left;
    let line = Line::from(vec![
        Span::styled("─".repeat(left), Style::default().fg(c_overlay0())),
        Span::styled(text, Style::default().fg(c_overlay0())),
        Span::styled("─".repeat(right), Style::default().fg(c_overlay0())),
    ]);
    frame.render_widget(Paragraph::new(line), row);
}

fn draw_setting_row(
    frame: &mut Frame,
    row: Rect,
    setting: SettingId,
    selectable_index: usize,
    app: &AppState,
) {
    let selected = app.more_focus == MoreFocus::Content && selectable_index == app.setting_index;
    if selected {
        frame.render_widget(
            Block::default().style(Style::default().bg(c_surface0())),
            row,
        );
    }

    let columns = setting_columns(row);

    let label_style = if selected {
        Style::default().fg(c_accent()).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(c_subtext1())
    };
    let value_style = if selected {
        Style::default().fg(c_accent())
    } else {
        Style::default().fg(c_overlay1())
    };

    let (label, value) = setting_text(app, setting);
    frame.render_widget(Paragraph::new(label).style(label_style), columns[0]);

    if let Some((ratio, _)) = setting_slider(app, setting) {
        frame.render_widget(
            Paragraph::new(value)
                .alignment(Alignment::Right)
                .style(value_style),
            columns[2],
        );
        frame.render_widget(
            Gauge::default()
                .ratio(ratio)
                .label("")
                .gauge_style(Style::default().fg(c_accent()).bg(c_surface0()))
                .style(Style::default().bg(c_surface0())),
            columns[1],
        );
    } else {
        let value_area = Rect::new(
            columns[1].x,
            row.y,
            columns[1].width.saturating_add(columns[2].width),
            1,
        );
        frame.render_widget(Paragraph::new(value).style(value_style), value_area);
    }
}

fn more_content_border(app: &AppState) -> Color {
    if app.more_focus == MoreFocus::Content {
        c_accent()
    } else {
        c_surface1()
    }
}

fn setting_text(app: &AppState, setting: SettingId) -> (String, String) {
    match setting {
        SettingId::ReadingMode => (
            "Reading mode".to_owned(),
            reading_mode_label(app.config.reading_mode).to_owned(),
        ),
        SettingId::Zoom => ("Zoom".to_owned(), format!("{}%", app.zoom)),
        SettingId::PrefetchBehind => (
            "Prefetch behind".to_owned(),
            app.scroll_cache.prefetch_behind.to_string(),
        ),
        SettingId::PrefetchAhead => (
            "Prefetch ahead".to_owned(),
            app.scroll_cache.prefetch_ahead.to_string(),
        ),
        SettingId::CacheMargin => (
            "Cache margin".to_owned(),
            app.scroll_cache.cache_margin.to_string(),
        ),
        SettingId::LibraryPath => ("Library path".to_owned(), display_path(&app.library.root)),
        SettingId::DefaultCategory => (
            "Default category".to_owned(),
            app.config
                .default_category
                .clone()
                .or_else(|| app.category_name().map(str::to_owned))
                .unwrap_or_else(|| "Default".to_owned()),
        ),
        SettingId::LibrarySort => (
            "Sort order".to_owned(),
            library_sort_label(app.config.library_sort).to_owned(),
        ),
        SettingId::RescanLibrary => ("Rescan library".to_owned(), "Enter".to_owned()),
        SettingId::Theme => ("Theme".to_owned(), theme_label(app.theme).to_owned()),
        SettingId::Accent => ("Accent".to_owned(), app.accent_name()),
        SettingId::Fullscreen => (
            "Fullscreen".to_owned(),
            if app.config.fullscreen { "Yes" } else { "No" }.to_owned(),
        ),
        SettingId::ThumbnailSize => (
            "Thumbnail size".to_owned(),
            format!("{} cols", app.config.thumbnail_width),
        ),
        SettingId::Incognito => (
            "Incognito mode".to_owned(),
            if app.config.incognito { "On" } else { "Off" }.to_owned(),
        ),
    }
}

fn setting_slider(app: &AppState, setting: SettingId) -> Option<(f64, (u16, u16))> {
    match setting {
        SettingId::Zoom => Some(((f64::from(app.zoom) - 50.0) / 250.0, (50, 300))),
        SettingId::PrefetchBehind => Some((app.scroll_cache.prefetch_behind as f64 / 8.0, (0, 8))),
        SettingId::PrefetchAhead => Some((app.scroll_cache.prefetch_ahead as f64 / 8.0, (0, 8))),
        SettingId::CacheMargin => Some((app.scroll_cache.cache_margin as f64 / 12.0, (0, 12))),
        SettingId::ThumbnailSize => Some((
            (f64::from(app.config.thumbnail_width) - 8.0) / 20.0,
            (8, 28),
        )),
        _ => None,
    }
}

fn library_sort_label(sort: LibrarySort) -> &'static str {
    match sort {
        LibrarySort::Natural => "Natural",
        LibrarySort::TitleAsc => "Title A–Z",
        LibrarySort::TitleDesc => "Title Z–A",
    }
}

fn display_path(path: &std::path::Path) -> String {
    if let Some(home) = std::env::var_os("HOME") {
        if let Ok(stripped) = path.strip_prefix(home) {
            if stripped.as_os_str().is_empty() {
                return "~".to_owned();
            }
            return format!("~/{}", stripped.display());
        }
    }
    path.display().to_string()
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
        lines.push(field(category, &format!("{count} series"), c_text()));
    }
    if lines.is_empty() {
        lines.push(Line::from("No categories found."));
    }

    let paragraph = Paragraph::new(Text::from(lines))
        .block(panel(
            Line::from(Span::styled(
                " Categories ",
                Style::default().fg(c_accent()).add_modifier(Modifier::BOLD),
            )),
            more_content_border(app),
        ))
        .style(Style::default().fg(c_subtext1()))
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
        field("Series", &app.library.series.len().to_string(), c_green()),
        field("Chapters", &chapters.to_string(), c_green()),
        field(
            "Categories",
            &app.library.categories.len().to_string(),
            c_green(),
        ),
        field(
            "Scan errors",
            &app.library.errors.len().to_string(),
            c_peach(),
        ),
    ];
    let paragraph = Paragraph::new(Text::from(lines))
        .block(panel(
            Line::from(Span::styled(
                " Statistics ",
                Style::default().fg(c_accent()).add_modifier(Modifier::BOLD),
            )),
            more_content_border(app),
        ))
        .style(Style::default().fg(c_subtext1()));
    frame.render_widget(paragraph, area);
}

fn draw_more_storage(frame: &mut Frame, area: Rect, app: &AppState) {
    let lines = vec![
        field("Library", &app.library.root.display().to_string(), c_text()),
        field(
            "Categories",
            &app.library
                .root
                .join("categories.json")
                .display()
                .to_string(),
            c_overlay1(),
        ),
        field(
            "Progress",
            "XDG data / tsuiyomi/progress.json",
            c_overlay1(),
        ),
    ];
    let paragraph = Paragraph::new(Text::from(lines))
        .block(panel(
            Line::from(Span::styled(
                " Data and storage ",
                Style::default().fg(c_accent()).add_modifier(Modifier::BOLD),
            )),
            more_content_border(app),
        ))
        .style(Style::default().fg(c_subtext1()))
        .wrap(Wrap { trim: true });
    frame.render_widget(paragraph, area);
}

fn draw_more_about(frame: &mut Frame, area: Rect, app: &AppState) {
    let mode = match app.reader_mode {
        ReaderMode::Page => "page",
        ReaderMode::Scroll => "scroll",
    };
    let lines = vec![
        field("Version", env!("CARGO_PKG_VERSION"), c_text()),
        field("Graphics", app.protocol_name(), c_blue()),
        field("Reader", mode, c_blue()),
        field("Zoom", &format!("{}%", app.zoom), c_green()),
        Line::from(""),
        Line::from(Span::styled(
            "Tsuiyomi is an independent Mihon-compatible TUI.",
            Style::default().fg(c_overlay1()),
        )),
    ];
    let paragraph = Paragraph::new(Text::from(lines))
        .block(panel(
            Line::from(Span::styled(
                " About ",
                Style::default().fg(c_accent()).add_modifier(Modifier::BOLD),
            )),
            more_content_border(app),
        ))
        .style(Style::default().fg(c_subtext1()))
        .wrap(Wrap { trim: true });
    frame.render_widget(paragraph, area);
}

fn draw_more_help(frame: &mut Frame, area: Rect, app: &AppState) {
    let lines = vec![
        field("Layout", "Mihon local/ source", c_blue()),
        field(
            "Docs",
            "https://mihon.app/docs/guides/local-source/",
            c_text(),
        ),
        field("Backups", "https://mihon.app/docs/guides/backups", c_text()),
        Line::from(""),
        field("Keys", "↑↓ move · ↵ open · Esc back", c_green()),
        field("Tabs", "1–5 or Tab/Shift-Tab", c_green()),
        field(
            "Reader",
            "←→ page · Space screen · +/- zoom · m mode",
            c_green(),
        ),
        field("Browse", "Enter open · Esc back · r rescan", c_green()),
        field("Display", "f fullscreen · r rescan · c More", c_green()),
    ];
    let paragraph = Paragraph::new(Text::from(lines))
        .block(panel(
            Line::from(Span::styled(
                " Help ",
                Style::default().fg(c_accent()).add_modifier(Modifier::BOLD),
            )),
            more_content_border(app),
        ))
        .style(Style::default().fg(c_subtext1()))
        .wrap(Wrap { trim: true });
    frame.render_widget(paragraph, area);
}

fn placeholder(frame: &mut Frame, area: Rect, title: &'static str, message: &'static str) {
    let block = panel(
        Line::from(Span::styled(
            format!(" {title} "),
            Style::default().fg(c_accent()).add_modifier(Modifier::BOLD),
        )),
        c_accent(),
    );
    let inner = block.inner(area);
    frame.render_widget(block, area);
    frame.render_widget(
        Paragraph::new(message)
            .alignment(Alignment::Center)
            .style(Style::default().fg(c_overlay1())),
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

fn draw_picker(frame: &mut Frame, app: &AppState) {
    let Some(picker) = app.picker.as_ref() else {
        return;
    };
    let choices = accent_choices(&palette_for(app.theme));
    let count = match picker.kind {
        PickerKind::Theme => THEMES.len(),
        PickerKind::Accent => choices.len() + 1,
    };

    let area = frame.area();
    let width = area.width.saturating_sub(4).clamp(28, 44).min(area.width);
    let height = (count as u16 + 3).min(area.height);
    if width < 12 || height < 4 {
        return;
    }

    let popup = Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    );
    let title = match picker.kind {
        PickerKind::Theme => "Theme",
        PickerKind::Accent => "Accent",
    };
    let block = panel(
        Line::from(Span::styled(
            format!(" {title} "),
            Style::default().fg(c_accent()).add_modifier(Modifier::BOLD),
        )),
        c_accent(),
    );
    let inner = block.inner(popup);
    frame.render_widget(Clear, popup);
    frame.render_widget(&block, popup);

    let list_height = inner.height.saturating_sub(1) as usize;
    let offset = if picker.selected >= list_height {
        picker.selected + 1 - list_height
    } else {
        0
    };
    for (row_index, item_index) in (offset..count).take(list_height).enumerate() {
        let row = Rect::new(inner.x, inner.y + row_index as u16, inner.width, 1);
        let selected = item_index == picker.selected;
        let style = if selected {
            Style::default()
                .fg(c_accent())
                .bg(c_surface0())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(c_subtext1())
        };
        if selected {
            frame.render_widget(Block::default().style(style), row);
        }
        frame.render_widget(
            Paragraph::new(picker_item(app, picker, &choices, item_index, inner.width))
                .style(style),
            row,
        );
    }

    let hint = Rect::new(
        inner.x,
        inner.y + inner.height.saturating_sub(1),
        inner.width,
        1,
    );
    let text = match picker.kind {
        PickerKind::Theme => "↑↓ choose · Enter apply · Esc close",
        PickerKind::Accent if picker.editing_custom => "hex · ⌫ delete · Enter apply · Esc back",
        PickerKind::Accent => "↑↓ choose · Enter apply · Esc close",
    };
    frame.render_widget(
        Paragraph::new(text).style(Style::default().fg(c_overlay0())),
        hint,
    );
}

fn picker_item(
    app: &AppState,
    picker: &PickerState,
    choices: &[(&'static str, Color)],
    index: usize,
    width: u16,
) -> Line<'static> {
    match picker.kind {
        PickerKind::Theme => {
            let theme = THEMES[index];
            let marker = if theme == app.theme { "●" } else { " " };
            let label = format!(" {marker} {}", theme_label(theme));
            let swatches = theme_swatches(theme);
            let used = label.chars().count() + swatches.len();
            let pad = (width as usize).saturating_sub(used + 1);
            let mut spans = vec![Span::raw(label), Span::raw(" ".repeat(pad))];
            for color in swatches {
                spans.push(Span::styled("■", Style::default().fg(color)));
            }
            spans.push(Span::raw(" "));
            Line::from(spans)
        }
        PickerKind::Accent if index < choices.len() => {
            let (name, color) = choices[index];
            let current = match app.accent.as_deref() {
                None => index == 0,
                Some(value) => name.eq_ignore_ascii_case(value),
            };
            let marker = if current { "●" } else { " " };
            Line::from(vec![
                Span::raw(" "),
                Span::styled(marker, Style::default().fg(c_accent())),
                Span::raw(format!(" {:<10}", title_case(name))),
                Span::styled("██", Style::default().fg(color)),
            ])
        }
        PickerKind::Accent => {
            let shown = if picker.custom.is_empty() {
                if picker.editing_custom {
                    "▏".to_owned()
                } else {
                    "#rrggbb".to_owned()
                }
            } else {
                let mut text = picker.custom.clone();
                if picker.editing_custom {
                    text.push('▏');
                }
                text
            };
            let color = parse_hex(&picker.custom).unwrap_or(c_overlay1());
            Line::from(vec![
                Span::raw("   "),
                Span::raw(format!("{:<10}", "Custom")),
                Span::styled(shown, Style::default().fg(color)),
            ])
        }
    }
}

/// The theme's chromatic palette, shown as swatches in the theme picker.
fn theme_swatches(theme: Theme) -> [Color; 7] {
    let palette = palette_for(theme);
    [
        palette.accent,
        palette.blue,
        palette.teal,
        palette.green,
        palette.yellow,
        palette.peach,
        palette.mauve,
    ]
}

fn title_case(name: &str) -> String {
    let mut chars = name.chars();
    match chars.next() {
        Some(first) => format!("{}{}", first.to_ascii_uppercase(), chars.as_str()),
        None => String::new(),
    }
}

fn panel(title: Line<'static>, border: Color) -> Block<'static> {
    Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border))
        .style(Style::default().bg(c_base()))
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
            status: None,
            cover: None,
            categories: vec!["Default".to_owned()],
            chapters: vec![Chapter {
                path: PathBuf::from("/tmp/library/Test Series/Chapter 1"),
                title: "Chapter 1".to_owned(),
                number: Some(1.0),
                kind: ChapterKind::Directory,
                scanlator: None,
                date: None,
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
        assert!(!text.contains("1 Library"));
        assert!(text.contains("Test Series"));
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
    fn library_grid_loads_tile_covers() {
        let root = tempfile::tempdir().unwrap();
        let cover = root.path().join("cover.jpg");
        image::RgbImage::from_pixel(20, 30, image::Rgb([40, 120, 80]))
            .save(&cover)
            .unwrap();

        let mut app = test_app();
        app.set_image_picker(ratatui_image::picker::Picker::halfblocks());
        app.library.series[0].cover = Some(cover.clone());

        let _ = render(&mut app, 80, 30);
        assert!(app.tile_covers.contains_key(&cover));
    }

    #[test]
    fn browse_view_renders_sources_and_local_items() {
        let mut app = test_app();
        app.tab = 3;

        let text = render(&mut app, 80, 30);
        assert!(text.contains("Browse · Sources"));
        assert!(text.contains("Sources"));
        assert!(text.contains("Local source"));

        app.browse_open();
        let text = render(&mut app, 80, 30);
        assert!(text.contains("Browse · Local source"));
        assert!(text.contains("Test Series"));
        assert!(text.contains("1 ch"));

        app.browse_open();
        let text = render(&mut app, 80, 30);
        assert!(text.contains("Series"));
        assert!(text.contains("Test Series"));
        assert!(text.contains("Enter opens chapters"));

        app.browse_open();
        let text = render(&mut app, 80, 30);
        assert_eq!(app.tab, 3);
        assert_eq!(app.view, LibraryView::Chapters);
        assert!(text.contains("Chapters"));
        assert!(text.contains("Chapter 1"));
    }

    #[test]
    fn details_pane_renders_with_a_custom_cover() {
        let root = tempfile::tempdir().unwrap();
        let cover = root.path().join("cover.jpg");
        image::RgbImage::from_pixel(20, 30, image::Rgb([120, 80, 160]))
            .save(&cover)
            .unwrap();

        let mut app = test_app();
        app.set_image_picker(ratatui_image::picker::Picker::halfblocks());
        app.library.series[0].cover = Some(cover);
        app.tab = 3;
        app.browse_view = BrowseView::Details;
        app.browse_detail = Some(app.library.series[0].path.clone());

        let text = render(&mut app, 100, 30);
        assert!(app.cover.is_some());
        assert!(text.contains("Test Series"));
        assert!(text.contains("Author"));
    }

    #[test]
    fn more_settings_renders_the_mihon_categories() {
        let mut app = test_app();
        app.tab = 4;
        let text = render(&mut app, 80, 24);

        assert!(text.contains("More"));
        assert!(text.contains("Settings"));
        assert!(text.contains("Appearance"));
        assert!(text.contains("Library"));
        assert!(text.contains("Reader"));
        assert!(text.contains("Downloads"));
        assert!(text.contains("Tracking"));
        assert!(text.contains("Browse"));
        assert!(text.contains("Security"));
        assert!(text.contains("Advanced"));
        assert!(text.contains("About"));
        assert!(text.contains("Theme · accent"));
    }

    #[test]
    fn settings_pages_render_their_controls() {
        let mut app = test_app();
        app.tab = 4;
        app.settings_page = SettingsPage::Reader;
        let text = render(&mut app, 80, 30);
        assert!(text.contains("Settings · Reader"));
        assert!(text.contains("Reading mode"));
        assert!(text.contains("Zoom"));
        assert!(text.contains("Prefetch behind"));

        app.settings_page = SettingsPage::Library;
        let text = render(&mut app, 80, 30);
        assert!(text.contains("Settings · Library"));
        assert!(text.contains("Library path"));
        assert!(text.contains("Sort order"));
        assert!(text.contains("Default category"));
        assert!(text.contains("Rescan library"));
    }

    #[test]
    fn theme_picker_renders_all_theme_names() {
        let mut app = test_app();
        app.tab = 4;
        app.more_section = MoreSection::Settings;
        app.settings_page = SettingsPage::Appearance;
        app.setting_index = 0;
        app.activate_setting();

        let text = render(&mut app, 80, 30);
        assert!(text.contains("Catppuccin Mocha"));
        assert!(text.contains("Catppuccin Macchiato"));
        assert!(text.contains("Solarized Dark"));
        assert!(text.contains("Gruvbox Light"));
        assert!(text.contains("Tokyo Night"));
        assert!(text.contains("Enter apply"));
        // Every theme row gets the same number of palette swatches.
        assert!(text.matches('■').count() >= THEMES.len() * 7);
    }

    #[test]
    fn accent_picker_renders_choices_and_custom_hex() {
        let mut app = test_app();
        app.tab = 4;
        app.more_section = MoreSection::Settings;
        app.settings_page = SettingsPage::Appearance;
        app.setting_index = 1;
        app.activate_setting();
        for character in "#ff00aa".chars() {
            app.picker_char(character);
        }

        let text = render(&mut app, 80, 30);
        assert!(text.contains("Blue"));
        assert!(text.contains("Mauve"));
        assert!(text.contains("Custom"));
        assert!(text.contains("#ff00aa"));
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
        assert_eq!(selected_cell.style().fg, Some(c_accent()));
        assert_eq!(selected_cell.style().bg, Some(c_base()));

        let inactive = tabs[1].area;
        let inactive_cell = &buffer[(inactive.x + inactive.width / 2, inactive.y)];
        assert_eq!(inactive_cell.style().fg, Some(c_overlay1()));
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
