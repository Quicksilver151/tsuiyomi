use std::collections::HashSet;

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
use tsuiyomi_core::progress_key as remote_progress_key;
use tsuiyomi_core::{
    downloads_dir, BookmarkStore, Chapter, LibrarySort, ProgressStore, ReadingMode, RemoteChapter,
    RemoteManga, RemoteSource, Series, SourceDisplayMode, SourceFilter, SourcePreferenceKind,
    Theme,
};

use crate::{
    app::{
        AppState, BrowseTab, BrowseView, ExtensionRow, FilterRow, LibraryView, MoreFocus, MoreRow,
        MoreSection, PickerKind, PickerState, ReaderMode, SelectionScope, SettingId, SettingsPage,
        SourceFilterRow, SourceRow, MORE_SECTIONS, SETTINGS_PAGES,
    },
    layout::{
        app_layout, grid_columns, grid_margin, grid_row_offset, grid_tile_rect, grid_tile_width,
        grid_visible_rows, library_grid_area, library_layout, more_layout, reader_bar_layout,
        setting_columns, settings_rows, tab_layouts, tile_cover_height, TAB_NAMES, TILE_GAP,
        TILE_TEXT_HEIGHT,
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

    if app.reader.is_some() {
        draw_reader(frame, layout.content, app);
        draw_reader_bar(frame, layout.tabs, app);
    } else {
        match app.tab {
            0 => draw_library(frame, layout.content, app),
            1 => placeholder(
                frame,
                layout.content,
                "Updates",
                "Update tracking is not implemented yet.",
            ),
            2 => draw_history(frame, layout.content, app),
            3 => draw_browse(frame, layout.content, app),
            _ => draw_more(frame, layout.content, app),
        }
        draw_tabs(frame, layout.tabs, app.tab);
    }

    if let Some(commands) = layout.commands {
        draw_commands(frame, commands, app);
    }

    draw_picker(frame, app);
    draw_source_pref_picker(frame, app);
    draw_filter_picker(frame, app);
    draw_quit_confirm(frame, app);
    draw_uninstall_confirm(frame, app);
    draw_library_delete(frame, app);
    draw_duplicate_dialog(frame, app);
    draw_remove_favorite(frame, app);
    draw_text_input(frame, app);
}

fn draw_text_input(frame: &mut Frame, app: &AppState) {
    let Some(input) = app.text_input.as_ref() else {
        return;
    };
    let area = frame.area();
    let width = 64.min(area.width);
    let height = 5.min(area.height);
    if width < 20 || height < 4 {
        return;
    }
    let popup = Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    );
    let block = panel(
        Line::from(Span::styled(
            format!(" {} ", input.title),
            Style::default().fg(c_accent()).add_modifier(Modifier::BOLD),
        )),
        c_accent(),
    );
    let inner = block.inner(popup);
    frame.render_widget(Clear, popup);
    frame.render_widget(&block, popup);
    let lines = vec![
        Line::from(Span::styled(
            format!("{}▏", input.value),
            Style::default().fg(c_text()),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "Enter apply · Esc cancel",
            Style::default().fg(c_overlay0()),
        )),
    ];
    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn draw_quit_confirm(frame: &mut Frame, app: &AppState) {
    if !app.quit_confirm {
        return;
    }
    let area = frame.area();
    let width = 46.min(area.width);
    let height = 5.min(area.height);
    if width < 16 || height < 4 {
        return;
    }
    let popup = Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    );
    let block = panel(
        Line::from(Span::styled(
            " Quit ",
            Style::default().fg(c_accent()).add_modifier(Modifier::BOLD),
        )),
        c_accent(),
    );
    let inner = block.inner(popup);
    frame.render_widget(Clear, popup);
    frame.render_widget(&block, popup);

    let lines = vec![
        Line::from(Span::styled(
            "Quit tsuiyomi?",
            Style::default().fg(c_text()).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "q / Enter / y  exit    n / Esc  cancel",
            Style::default().fg(c_overlay0()),
        )),
    ];
    frame.render_widget(
        Paragraph::new(Text::from(lines)).alignment(Alignment::Center),
        inner,
    );
}

fn draw_uninstall_confirm(frame: &mut Frame, app: &AppState) {
    let Some(items) = &app.uninstall_confirm else {
        return;
    };
    let area = frame.area();
    let width = 54.min(area.width);
    let height = 6.min(area.height);
    if width < 20 || height < 4 {
        return;
    }
    let label = if items.len() == 1 {
        format!("Uninstall {}?", items[0].1)
    } else {
        format!("Uninstall {} extensions?", items.len())
    };
    let popup = Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    );
    let block = panel(
        Line::from(Span::styled(
            " Uninstall ",
            Style::default().fg(c_accent()).add_modifier(Modifier::BOLD),
        )),
        c_accent(),
    );
    let inner = block.inner(popup);
    frame.render_widget(Clear, popup);
    frame.render_widget(&block, popup);

    let lines = vec![
        Line::from(Span::styled(
            label,
            Style::default().fg(c_text()).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "y  uninstall    Esc  cancel",
            Style::default().fg(c_overlay0()),
        )),
    ];
    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true }),
        inner,
    );
}

fn draw_library_delete(frame: &mut Frame, app: &AppState) {
    let Some(dialog) = &app.library_delete else {
        return;
    };
    let area = frame.area();
    let remote = dialog.items.iter().any(|item| item.remote.is_some());
    let width = 66.min(area.width);
    let height = if remote { 9 } else { 8 }.min(area.height);
    if width < 28 || height < 5 {
        return;
    }
    let popup = Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    );
    let block = panel(
        Line::from(Span::styled(
            " Remove ",
            Style::default().fg(c_accent()).add_modifier(Modifier::BOLD),
        )),
        c_accent(),
    );
    let inner = block.inner(popup);
    frame.render_widget(Clear, popup);
    frame.render_widget(&block, popup);

    let checkbox = |checked: bool, label: &str, focused: bool| {
        let marker = if checked { "[x]" } else { "[ ]" };
        let style = if focused {
            Style::default().fg(c_accent()).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(c_text())
        };
        Line::from(Span::styled(format!("{marker} {label}"), style))
    };

    let title = if dialog.items.len() == 1 {
        format!("Remove {}?", dialog.items[0].title)
    } else {
        format!("Remove {} items?", dialog.items.len())
    };
    let mut lines = vec![
        Line::from(Span::styled(
            truncate_text(&title, width.saturating_sub(2)),
            Style::default().fg(c_text()).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        checkbox(
            dialog.remove_from_library,
            "Manga from library",
            dialog.focus == 0,
        ),
    ];
    if remote {
        lines.push(checkbox(
            dialog.delete_downloads,
            "Downloaded chapters",
            dialog.focus == 1,
        ));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Space toggle · ↑↓ select · Enter apply · Esc cancel",
        Style::default().fg(c_overlay0()),
    )));
    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn draw_duplicate_dialog(frame: &mut Frame, app: &AppState) {
    let Some(dialog) = app.duplicate_dialog.as_ref() else {
        return;
    };
    let area = frame.area();
    let width = 72.min(area.width);
    let height = (dialog.duplicates.len() as u16 + 8).min(area.height).max(8);
    if width < 30 || height < 6 {
        return;
    }
    let popup = Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    );
    let block = panel(
        Line::from(Span::styled(
            " Possible duplicate ",
            Style::default().fg(c_accent()).add_modifier(Modifier::BOLD),
        )),
        c_accent(),
    );
    let inner = block.inner(popup);
    frame.render_widget(Clear, popup);
    frame.render_widget(&block, popup);

    let mut lines = vec![
        Line::from(Span::styled(
            format!("{} is already in the library:", dialog.manga.title),
            Style::default().fg(c_text()).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
    ];
    for (index, favorite) in dialog.duplicates.iter().enumerate() {
        let selected = index == dialog.selected;
        let source = source_display_name(app, &favorite.source_id)
            .unwrap_or_else(|| favorite.source_id.clone());
        lines.push(Line::from(vec![
            Span::styled(
                if selected { "› " } else { "  " },
                Style::default().fg(c_accent()),
            ),
            Span::styled(
                favorite.title.clone(),
                if selected {
                    Style::default().fg(c_accent()).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(c_text())
                },
            ),
            Span::styled(format!("  {source}"), Style::default().fg(c_overlay0())),
        ]));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "↑↓ select · a add anyway · Enter open · Esc cancel",
        Style::default().fg(c_overlay0()),
    )));
    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn draw_remove_favorite(frame: &mut Frame, app: &AppState) {
    let Some(manga) = app.remove_favorite.as_ref() else {
        return;
    };
    let area = frame.area();
    let width = 66.min(area.width);
    let height = 6.min(area.height);
    if width < 24 || height < 5 {
        return;
    }
    let popup = Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    );
    let block = panel(
        Line::from(Span::styled(
            " Remove from library ",
            Style::default().fg(c_accent()).add_modifier(Modifier::BOLD),
        )),
        c_accent(),
    );
    let inner = block.inner(popup);
    frame.render_widget(Clear, popup);
    frame.render_widget(&block, popup);
    let lines = vec![
        Line::from(Span::styled(
            truncate_text(&format!("Remove {}?", manga.title), width.saturating_sub(4)),
            Style::default().fg(c_text()).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "y / Enter remove · Esc cancel",
            Style::default().fg(c_overlay0()),
        )),
    ];
    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
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

    let chapter_index = app.reader_chapter_index();
    let chapter_count = app.reader_chapter_count();
    let previous_enabled = chapter_index + 1 < chapter_count;
    let next_enabled = chapter_index > 0;
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

    let mut items: Vec<(&str, &str)> = if app.reader.is_some() {
        if app.reader_mode == ReaderMode::Scroll {
            vec![
                ("q", "quit"),
                ("↑↓", "scroll"),
                ("←→", "page"),
                ("Space", "screen"),
                ("+/-", "zoom"),
                ("f", "full"),
                ("Esc", "back"),
            ]
        } else {
            vec![
                ("q", "quit"),
                ("←→", "page"),
                ("f", "full"),
                ("Esc", "back"),
            ]
        }
    } else if app.tab == 3 {
        match app.browse_view {
            BrowseView::Sources => vec![
                ("q", "quit"),
                ("↑↓", "source"),
                ("↵", "open"),
                ("P", "pin"),
                ("A", "search all"),
                ("o", "filters"),
                ("C", "configure"),
                ("[ ]", "tabs"),
                ("e", "extensions"),
                ("r", "refresh"),
                ("Esc", "back"),
            ],
            BrowseView::SourceFilters => vec![
                ("q", "quit"),
                ("↑↓", "row"),
                ("↵", "toggle"),
                ("r", "refresh"),
                ("Esc", "back"),
            ],
            BrowseView::SourcePrefs => vec![
                ("q", "quit"),
                ("↑↓", "preference"),
                ("↵", "change"),
                ("r", "reload"),
                ("Esc", "back"),
            ],
            BrowseView::Filters => vec![
                ("q", "quit"),
                ("↑↓", "filter"),
                ("↵", "change"),
                ("←→", "adjust"),
                ("r", "reload"),
                ("Esc", "back"),
            ],
            BrowseView::ExtensionDetails => vec![
                ("q", "quit"),
                ("↑↓", "row"),
                ("↵", "toggle"),
                ("C", "prefs"),
                ("r", "reload"),
                ("Esc", "back"),
            ],
            BrowseView::Local => vec![
                ("q", "quit"),
                ("↑↓←→", "move"),
                ("↵", "open"),
                ("s", "search"),
                (
                    "l",
                    if app.browse_local_latest {
                        "all"
                    } else {
                        "latest"
                    },
                ),
                (
                    "v",
                    if app.config.local_source_order == tsuiyomi_core::LocalSourceOrder::TitleDesc {
                        "title ↑"
                    } else {
                        "title ↓"
                    },
                ),
                ("[ ]", "tabs"),
                ("e", "extensions"),
                ("r", "rescan"),
                ("Esc", "back"),
            ],
            BrowseView::Chapters => vec![
                ("q", "quit"),
                ("↑↓", "chapter"),
                ("↵", "read"),
                ("[ ]", "tabs"),
                ("e", "extensions"),
                ("Esc", "back"),
            ],
            BrowseView::RemoteResults => vec![
                ("q", "quit"),
                ("↑↓←→", "move"),
                ("s", "search"),
                ("z", "filters"),
                ("v", "view"),
                ("h", "hide saved"),
                ("l", "latest"),
                ("n", "more"),
                ("F", "library"),
                ("↵", "open"),
                ("r", "refresh"),
                ("[ ]", "tabs"),
                ("e", "extensions"),
                ("Esc", "back"),
            ],
            BrowseView::RemoteDetails => vec![
                ("q", "quit"),
                ("↑↓", "chapter"),
                ("↵", "read"),
                ("F", "library"),
                ("r", "refresh"),
                ("[ ]", "tabs"),
                ("e", "extensions"),
                ("Esc", "back"),
            ],
            BrowseView::GlobalResults => vec![
                ("q", "quit"),
                ("↑↓", "source"),
                ("←→", "result"),
                ("A", "scope"),
                ("F", "library"),
                ("↵", "open"),
                ("Esc", "back"),
            ],
            BrowseView::Extensions => vec![
                ("q", "quit"),
                ("↑↓", "extension"),
                ("i/↵", "install"),
                ("o", "details"),
                ("/", "search"),
                ("P", "pin"),
                ("u", "update"),
                ("U", "update all"),
                ("a", "store"),
                ("t", "stores"),
                ("[ ]", "tabs"),
                ("Esc", "back"),
            ],
            BrowseView::Stores => vec![
                ("q", "quit"),
                ("↑↓", "store"),
                ("a", "add"),
                ("d", "remove"),
                ("t", "extensions"),
                ("[ ]", "tabs"),
                ("Esc", "back"),
            ],
            BrowseView::Migrate => vec![
                ("q", "quit"),
                ("[ ]", "tabs"),
                ("e", "extensions"),
                ("Esc", "back"),
            ],
        }
    } else if app.tab == 4 {
        if app.more_focus == MoreFocus::Sections {
            vec![
                ("q", "quit"),
                ("↑↓", "section"),
                ("↵", "enter"),
                ("Esc", "back"),
            ]
        } else if app.more_section == MoreSection::About {
            vec![
                ("q", "quit"),
                ("H", "host"),
                ("[ ]", "sections"),
                ("Esc", "back"),
            ]
        } else if app.more_section == MoreSection::Settings
            && app.settings_page == SettingsPage::Main
        {
            vec![
                ("q", "quit"),
                ("↑↓", "category"),
                ("↵", "open"),
                ("Esc", "back"),
            ]
        } else {
            vec![
                ("q", "quit"),
                ("↑↓", "setting"),
                ("←→", "change"),
                ("↵", "action"),
                ("Esc", "back"),
            ]
        }
    } else if app.tab == 2 {
        vec![
            ("q", "quit"),
            ("↑↓", "history"),
            ("↵", "resume"),
            ("x/Del", "remove"),
            ("[ ]", "tabs"),
            ("Esc", "back"),
        ]
    } else if app.tab == 0 {
        match app.view {
            LibraryView::Series => vec![
                ("q", "quit"),
                ("↑↓←→", "move"),
                ("↵", "open"),
                ("r", "rescan"),
                ("c", "config"),
                ("R", "covers"),
                ("x/Del", "delete"),
                ("f", "full"),
                ("Esc", "back"),
            ],
            LibraryView::Chapters | LibraryView::RemoteDetails => vec![
                ("q", "quit"),
                ("↑↓", "chapter"),
                ("↵", "read"),
                ("Esc", "back"),
            ],
        }
    } else {
        vec![
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

    if app.has_downloadable_chapter() && !items.iter().any(|(key, _)| *key == "D") {
        let at = items.len().saturating_sub(1);
        items.insert(at, ("D", "download"));
    }
    if app.tab == 3
        && app.browse_view == BrowseView::Extensions
        && app
            .selected_extension()
            .is_some_and(|extension| extension.installed)
        && !items.iter().any(|(key, _)| *key == "x/Del")
    {
        let at = items.len().saturating_sub(1);
        items.insert(at, ("x/Del", "uninstall"));
    }
    if app.has_bookmarkable_chapter() && !items.iter().any(|(key, _)| *key == "b") {
        let at = items.len().saturating_sub(1);
        items.insert(at, ("b", "bookmark"));
    }

    if app.supports_multi_select() && !items.iter().any(|(key, _)| *key == "Space") {
        let at = items.len().saturating_sub(1);
        items.insert(at, ("Space", "select"));
    }

    let mut spans = Vec::new();
    if let Some(selection) = &app.multi_select {
        spans.push(Span::styled(
            format!("{} selected", selection.keys.len()),
            Style::default().fg(c_accent()).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(" · ", Style::default().fg(c_overlay0())));
    }
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
                SelectionScope::LibrarySeries,
            );
            app.library_columns = columns;
            app.library_grid_offset = offset;
        }
        LibraryView::Chapters => {
            draw_breadcrumb(frame, layout.categories, app);
            draw_library_chapters(frame, layout.list, layout.details, app);
        }
        LibraryView::RemoteDetails => {
            draw_breadcrumb(frame, layout.categories, app);
            draw_library_remote_detail(frame, layout.list, layout.details, app);
        }
    }
}

fn draw_library_chapters(
    frame: &mut Frame,
    list_area: Rect,
    details_area: Rect,
    app: &mut AppState,
) {
    let Some(series) = app.selected_series().cloned() else {
        draw_browse_empty(frame, list_area, "Series not found. Rescan the library.");
        return;
    };
    let items = local_chapter_items(
        &series.chapters,
        &app.progress,
        &app.bookmarks,
        app.selected_keys(SelectionScope::LocalChapters),
    );
    draw_story_chapters(
        frame,
        list_area,
        &series.title,
        items,
        series.chapters.len(),
        app.chapter_index,
        &mut app.chapter_list_state,
    );

    let cover_path = series.cover.clone();
    if let Some(path) = &cover_path {
        app.ensure_tile_cover(path);
    }
    let cover = cover_path
        .as_ref()
        .and_then(|path| app.tile_covers.get_mut(path));
    draw_story_info(
        frame,
        details_area,
        &series.title,
        series_lines(&series),
        cover,
    );
}

fn draw_library_remote_detail(
    frame: &mut Frame,
    list_area: Rect,
    details_area: Rect,
    app: &mut AppState,
) {
    let Some(manga) = app.library_remote.manga.clone() else {
        draw_browse_empty(frame, list_area, "No saved manga selected.");
        return;
    };
    let chapters = app.library_remote.chapters.clone();
    let source = app
        .library_remote
        .source_name
        .clone()
        .or_else(|| source_display_name(app, &manga.source_id));
    let items = remote_chapter_items(
        &manga.source_id,
        &manga.url,
        &chapters,
        &app.progress,
        &app.bookmarks,
        &app.library_remote.downloaded,
        app.selected_keys(SelectionScope::RemoteChapters),
    );
    draw_story_chapters(
        frame,
        list_area,
        &manga.title,
        items,
        chapters.len(),
        app.library_remote.chapter_index,
        &mut app.library_remote.list_state,
    );

    let cover_path =
        app.ensure_remote_cover(&manga.source_id, &manga.url, manga.thumbnail_url.as_deref());
    let lines = remote_story_lines(&manga, &chapters, source.as_deref(), "saved to library");
    let cover = cover_path
        .as_ref()
        .and_then(|path| app.tile_covers.get_mut(path));
    draw_story_info(frame, details_area, &manga.title, lines, cover);
}

fn draw_series_grid(
    frame: &mut Frame,
    area: Rect,
    app: &mut AppState,
    indices: &[usize],
    selected: usize,
    current_offset: usize,
    scope: SelectionScope,
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

    for (position, series_index) in indices[start..end].iter().enumerate() {
        let index = start + position;
        let (title, remote, chapter_count, series_path) = {
            let series = &app.library.series[*series_index];
            (
                series.title.clone(),
                series.remote.clone(),
                series.chapters.len(),
                series.path.clone(),
            )
        };
        let selection_key = match &remote {
            Some(remote) => format!("remote:{}\n{}", remote.source_id, remote.url),
            None => format!("local:{}", series_path.display()),
        };
        let multi_selected = app.is_multi_selected(scope, &selection_key);
        // Load the cover immediately before rendering this tile. Loading all covers
        // first could evict early tiles from `tile_covers` before they are drawn on
        // large grids.
        let cover_path = if let Some(remote) = &remote {
            let path = app.ensure_remote_cover(
                &remote.source_id,
                &remote.url,
                remote.thumbnail_url.as_deref(),
            );
            if path.is_none() {
                app.request_series_thumbnail(
                    &remote.source_id,
                    &remote.url,
                    remote.memo.as_deref(),
                );
            }
            path
        } else {
            let path = app.ensure_local_series_cover(*series_index);
            if let Some(path) = &path {
                app.ensure_tile_cover(path);
            }
            path
        };
        let detail = if let Some(remote) = &remote {
            app.remote
                .sources
                .iter()
                .find(|source| source.id == remote.source_id)
                .map(|source| source.display_name())
                .unwrap_or_else(|| "saved".to_owned())
        } else {
            format!("{chapter_count} ch")
        };
        let tile = grid_tile_rect(area, columns, offset, index, tile_height);
        let is_selected = index == selected;
        if is_selected {
            frame.render_widget(
                Block::default().style(Style::default().bg(c_surface0())),
                tile,
            );
        }
        if multi_selected {
            frame.render_widget(
                Paragraph::new("✓")
                    .style(Style::default().fg(c_accent()).add_modifier(Modifier::BOLD)),
                Rect::new(tile.x, tile.y, 1, 1),
            );
        }
        let cover_area = Rect::new(tile.x, tile.y, tile.width, cover_height);
        let mut rendered_cover = false;
        if let Some(path) = &cover_path {
            if let Some(protocol) = app.tile_covers.get_mut(path) {
                render_centered_image(frame, cover_area, protocol);
                rendered_cover = true;
            }
        }
        if !rendered_cover {
            let initial = title
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
        let title = truncate_text(&title, title_area.width);
        frame.render_widget(
            Paragraph::new(title)
                .alignment(Alignment::Center)
                .style(title_style),
            Rect::new(title_area.x, title_area.y, title_area.width, 1),
        );
        frame.render_widget(
            Paragraph::new(detail)
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

fn draw_history(frame: &mut Frame, area: Rect, app: &mut AppState) {
    let entries: Vec<(String, tsuiyomi_core::HistoryEntry)> = app
        .history
        .recent()
        .into_iter()
        .map(|(key, entry)| (key.clone(), entry.clone()))
        .collect();
    let block = panel(
        Line::from(vec![
            Span::styled(
                " History ",
                Style::default().fg(c_accent()).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{} ", entries.len()),
                Style::default().fg(c_overlay0()),
            ),
        ]),
        c_accent(),
    );
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if entries.is_empty() {
        frame.render_widget(
            Paragraph::new("No reading history yet.")
                .alignment(Alignment::Center)
                .style(Style::default().fg(c_overlay1())),
            inner,
        );
        app.history_list_state.select(None);
        return;
    }

    if app.history_selection >= entries.len() {
        app.history_selection = entries.len() - 1;
    }
    app.history_list_state.select(Some(app.history_selection));

    let items: Vec<ListItem> = entries
        .iter()
        .map(|(key, entry)| {
            let marker =
                if app.is_multi_selected(SelectionScope::History, &format!("history:{key}")) {
                    "✓ "
                } else {
                    ""
                };
            ListItem::new(Line::from(vec![
                Span::styled(marker, Style::default().fg(c_accent())),
                Span::styled(
                    entry.series_title.clone(),
                    Style::default().fg(c_text()).add_modifier(Modifier::BOLD),
                ),
                Span::styled(" · ", Style::default().fg(c_overlay0())),
                Span::raw(entry.chapter_title.clone()),
                Span::styled(
                    format!("  {}", entry.source),
                    Style::default().fg(c_overlay1()),
                ),
                Span::styled(
                    format!("  {}/{}", entry.page + 1, entry.pages),
                    Style::default().fg(c_teal()),
                ),
                Span::styled(
                    format!("  {}", history_age(entry.last_read)),
                    Style::default().fg(c_overlay0()),
                ),
            ]))
        })
        .collect();
    let list = List::new(items)
        .style(Style::default().fg(c_text()))
        .highlight_style(
            Style::default()
                .fg(c_accent())
                .bg(c_surface0())
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("› ");
    frame.render_stateful_widget(list, inner, &mut app.history_list_state);
}

fn history_age(last_read: u64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(last_read);
    let seconds = now.saturating_sub(last_read);
    if seconds < 60 {
        "just now".to_owned()
    } else if seconds < 3_600 {
        format!("{}m ago", seconds / 60)
    } else if seconds < 86_400 {
        format!("{}h ago", seconds / 3_600)
    } else {
        format!("{}d ago", seconds / 86_400)
    }
}

fn draw_browse(frame: &mut Frame, area: Rect, app: &mut AppState) {
    match app.browse_view {
        BrowseView::Sources => draw_browse_sources(frame, area, app),
        BrowseView::SourceFilters => draw_source_filters(frame, area, app),
        BrowseView::SourcePrefs => draw_source_prefs(frame, area, app),
        BrowseView::Filters => draw_filters(frame, area, app),
        BrowseView::ExtensionDetails => draw_extension_details(frame, area, app),
        BrowseView::Extensions => draw_extensions(frame, area, app),
        BrowseView::Stores => draw_stores(frame, area, app),
        BrowseView::Migrate => draw_migrate(frame, area, app),
        BrowseView::Local
        | BrowseView::Chapters
        | BrowseView::RemoteResults
        | BrowseView::RemoteDetails
        | BrowseView::GlobalResults => {
            draw_browse_path(frame, Rect::new(area.x, area.y, area.width, 1), app);
            let content = library_grid_area(area);
            match app.browse_view {
                BrowseView::Local => {
                    let indices = app.browse_local_indices();
                    let (columns, offset) = draw_series_grid(
                        frame,
                        content,
                        app,
                        &indices,
                        app.browse_selection,
                        app.browse_grid_offset,
                        SelectionScope::BrowseLocal,
                    );
                    app.browse_columns = columns;
                    app.browse_grid_offset = offset;
                }
                BrowseView::Chapters => draw_browse_chapters(frame, content, app),
                BrowseView::RemoteResults => draw_remote_grid(frame, content, app),
                BrowseView::RemoteDetails => draw_remote_detail(frame, content, app),
                BrowseView::GlobalResults => draw_global_results(frame, content, app),
                _ => {}
            }
            if let Some(status) = &app.remote.status {
                if area.height > 0 {
                    let status_area = Rect::new(area.x, area.y + area.height - 1, area.width, 1);
                    frame.render_widget(
                        Paragraph::new(format!(" {status}")).style(Style::default().fg(
                            if app.remote.error {
                                c_peach()
                            } else {
                                c_green()
                            },
                        )),
                        status_area,
                    );
                }
            }
        }
    }
}

/// The Browse top-level tabs rendered as a panel title.
fn browse_tab_title(app: &AppState) -> Line<'static> {
    let tabs = [
        ("Sources", BrowseTab::Sources),
        ("Extensions", BrowseTab::Extensions),
        ("Migrate", BrowseTab::Migrate),
    ];
    let mut spans = vec![Span::raw(" ")];
    let updates = app.extension_update_count();
    for (index, (label, tab)) in tabs.iter().enumerate() {
        if index > 0 {
            spans.push(Span::styled(" │ ", Style::default().fg(c_overlay0())));
        }
        let style = if app.browse_tab == *tab {
            Style::default().fg(c_accent()).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(c_overlay1())
        };
        spans.push(Span::styled(*label, style));
        if *tab == BrowseTab::Extensions && updates > 0 {
            spans.push(Span::styled(
                format!(" ({updates})"),
                Style::default().fg(c_peach()),
            ));
        }
    }
    spans.push(Span::raw(" "));
    Line::from(spans)
}

/// Panel body plus a reserved bottom status row when there is something to say.
fn browse_body_with_status(frame: &mut Frame, inner: Rect, app: &AppState) -> Rect {
    let Some(status) = browse_status(app) else {
        return inner;
    };
    if inner.height < 2 {
        return inner;
    }
    let status_area = Rect::new(inner.x, inner.y + inner.height - 1, inner.width, 1);
    frame.render_widget(
        Paragraph::new(format!(" {status}")).style(Style::default().fg(if app.remote.error {
            c_peach()
        } else {
            c_green()
        })),
        status_area,
    );
    Rect::new(inner.x, inner.y, inner.width, inner.height - 1)
}

fn browse_status(app: &AppState) -> Option<String> {
    app.remote
        .status
        .clone()
        .or_else(|| app.remote.host_hint.clone())
}

fn draw_global_results(frame: &mut Frame, area: Rect, app: &mut AppState) {
    let groups = app.remote.global_groups.clone();
    if groups.is_empty() {
        let message = if app.remote.global_loading {
            format!(
                "Searching · {}/{} sources",
                app.remote.global_finished, app.remote.global_total
            )
        } else if let Some(failure) = app.remote.global_failures.first() {
            format!(
                "No results · {}: {}",
                failure.source.display_name(),
                failure.message
            )
        } else {
            "No results.".to_owned()
        };
        frame.render_widget(
            Paragraph::new(message)
                .alignment(Alignment::Center)
                .style(Style::default().fg(c_overlay1())),
            Rect::new(area.x, area.y, area.width, 1),
        );
        return;
    }

    let columns = grid_columns(area, app.config.thumbnail_width);
    let tile_width = grid_tile_width(area, columns);
    let cover_height = tile_cover_height(app.config.thumbnail_width, area.height);
    let group_height = cover_height + 3;
    let visible = (area.height / group_height).max(1) as usize;
    let offset = if app.remote.global_group >= visible {
        app.remote.global_group + 1 - visible
    } else {
        0
    };
    let margin = grid_margin(area, columns);
    let stride = tile_width.saturating_add(TILE_GAP);
    let selected_group = app.remote.global_group;
    let selected_item = app.remote.global_item;

    for (group_index, group) in groups.iter().enumerate().skip(offset).take(visible) {
        let y = area.y + (group_index - offset) as u16 * group_height;
        let header = Line::from(vec![
            Span::styled(
                format!(" {}", group.source.display_name()),
                Style::default()
                    .fg(if group_index == selected_group {
                        c_accent()
                    } else {
                        c_text()
                    })
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("  {} results", group.mangas.len()),
                Style::default().fg(c_overlay0()),
            ),
        ]);
        frame.render_widget(Paragraph::new(header), Rect::new(area.x, y, area.width, 1));

        let item = if group_index == selected_group {
            selected_item
        } else {
            0
        };
        let horizontal = item.saturating_sub(columns.saturating_sub(1));
        let cover_y = y + 1;
        for (position, manga) in group
            .mangas
            .iter()
            .enumerate()
            .skip(horizontal)
            .take(columns)
        {
            let x = area.x + margin + (position - horizontal) as u16 * stride;
            let tile = Rect::new(x, cover_y, tile_width, cover_height);
            let is_selected = group_index == selected_group && position == item;
            if is_selected {
                frame.render_widget(
                    Block::default().style(Style::default().bg(c_surface0())),
                    tile,
                );
            }
            if app.is_multi_selected(
                SelectionScope::GlobalResults,
                &format!("remote:{}\n{}", group.source.id, manga.url),
            ) {
                frame.render_widget(
                    Paragraph::new("✓")
                        .style(Style::default().fg(c_accent()).add_modifier(Modifier::BOLD)),
                    Rect::new(tile.x, tile.y, 1, 1),
                );
            }
            let cover_path = app.ensure_remote_cover(
                &group.source.id,
                &manga.url,
                manga.thumbnail_url.as_deref(),
            );
            let mut rendered = false;
            if let Some(path) = &cover_path {
                if let Some(protocol) = app.tile_covers.get_mut(path) {
                    render_centered_image(frame, tile, protocol);
                    rendered = true;
                }
            } else if manga.thumbnail_url.is_none() {
                app.request_series_thumbnail(&group.source.id, &manga.url, manga.memo.as_deref());
            }
            if !rendered {
                let initial = manga
                    .title
                    .chars()
                    .next()
                    .map(|character| character.to_uppercase().collect::<String>())
                    .unwrap_or_default();
                frame.render_widget(
                    Paragraph::new(initial)
                        .alignment(Alignment::Center)
                        .style(Style::default().fg(c_accent()).add_modifier(Modifier::BOLD)),
                    Rect::new(tile.x, tile.y + tile.height / 2, tile.width, 1),
                );
            }
            frame.render_widget(
                Paragraph::new(truncate_text(&manga.title, tile.width))
                    .alignment(Alignment::Center)
                    .style(if is_selected {
                        Style::default().fg(c_accent()).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(c_text())
                    }),
                Rect::new(tile.x, cover_y + cover_height, tile.width, 1),
            );
        }
    }
}

fn browse_host_label(app: &AppState) -> String {
    if app.host_process.is_some() {
        "extension host".to_owned()
    } else if app.host_client.is_some() {
        "external host".to_owned()
    } else {
        "local only".to_owned()
    }
}

fn draw_browse_path(frame: &mut Frame, area: Rect, app: &AppState) {
    let (view, detail, count) = match app.browse_view {
        BrowseView::Sources => (
            "Sources".to_owned(),
            browse_host_label(app),
            format!("{} sources", app.source_order().len()),
        ),
        BrowseView::SourceFilters => (
            "Source filters".to_owned(),
            browse_host_label(app),
            format!(
                "{} sources · {} languages",
                app.source_order().len().saturating_sub(1),
                app.source_languages().len()
            ),
        ),
        BrowseView::SourcePrefs => (
            "Source preferences".to_owned(),
            app.source_prefs
                .as_ref()
                .map(|state| state.source.display_name())
                .unwrap_or_else(|| "Remote source".to_owned()),
            app.source_prefs
                .as_ref()
                .map_or(0, |state| state.preferences.prefs.len())
                .to_string(),
        ),
        BrowseView::ExtensionDetails => (
            "Extension".to_owned(),
            app.extension_details
                .as_ref()
                .map(|state| state.extension.name.clone())
                .unwrap_or_else(|| "Extension".to_owned()),
            app.extension_details
                .as_ref()
                .map_or(0, |state| state.sources.len())
                .to_string(),
        ),
        BrowseView::Filters => (
            "Filters".to_owned(),
            app.remote
                .source
                .as_ref()
                .map(|source| source.display_name())
                .unwrap_or_else(|| "Remote source".to_owned()),
            app.filter_item_count().to_string(),
        ),
        BrowseView::Local => (
            if app.browse_local_latest {
                "Local source · recent".to_owned()
            } else if app.browse_local_query.trim().is_empty() {
                "Local source".to_owned()
            } else {
                format!("Local source · \"{}\"", app.browse_local_query)
            },
            display_path(&app.library.root),
            if app.browse_local_query.trim().is_empty() && !app.browse_local_latest {
                format!("{} series", app.local_series_count())
            } else {
                format!(
                    "{} of {} series",
                    app.browse_local_indices().len(),
                    app.local_series_count()
                )
            },
        ),
        BrowseView::Chapters => (
            app.selected_browse_local_series()
                .map(|series| series.title.clone())
                .unwrap_or_else(|| "Local source".to_owned()),
            display_path(&app.library.root),
            format!("{} chapters", app.browse_local_chapter_count()),
        ),
        BrowseView::RemoteResults | BrowseView::RemoteDetails => (
            app.remote
                .source
                .as_ref()
                .map(|source| source.display_name())
                .unwrap_or_else(|| "Remote source".to_owned()),
            browse_host_label(app),
            match app.browse_view {
                BrowseView::RemoteDetails => format!("{} chapters", app.remote.chapters.len()),
                _ if app.remote.has_next_page => {
                    format!("{} results · more available", app.browse_item_count())
                }
                _ => format!("{} results", app.browse_item_count()),
            },
        ),
        BrowseView::GlobalResults => (
            "Global search".to_owned(),
            format!("\"{}\"", app.remote.global_query),
            format!(
                "{}/{} sources · {} results",
                app.remote.global_finished,
                app.remote.global_total,
                app.remote
                    .global_groups
                    .iter()
                    .map(|group| group.mangas.len())
                    .sum::<usize>()
            ),
        ),
        BrowseView::Extensions => (
            "Extensions".to_owned(),
            browse_host_label(app),
            if app.extension_query.trim().is_empty() {
                format!("{} extensions", app.remote.extensions.len())
            } else {
                format!(
                    "{} of {} extensions",
                    app.filtered_extension_count(),
                    app.remote.extensions.len()
                )
            },
        ),
        BrowseView::Migrate => (
            "Migrate".to_owned(),
            "not implemented".to_owned(),
            String::new(),
        ),
        BrowseView::Stores => (
            "Extension stores".to_owned(),
            browse_host_label(app),
            format!("{} stores", app.remote.repos.len()),
        ),
    };
    let line = Line::from(vec![
        Span::styled(
            format!(" Browse · {view} "),
            Style::default().fg(c_accent()).add_modifier(Modifier::BOLD),
        ),
        Span::styled(detail, Style::default().fg(c_text())),
        Span::styled(format!("  {count}"), Style::default().fg(c_overlay0())),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

fn draw_browse_sources(frame: &mut Frame, area: Rect, app: &mut AppState) {
    let block = panel(browse_tab_title(app), c_accent());
    let inner = block.inner(area);
    frame.render_widget(&block, area);
    let body = browse_body_with_status(frame, inner, app);
    if body.height == 0 {
        return;
    }
    draw_browse_path(frame, Rect::new(body.x, body.y, body.width, 1), app);
    let list_area = Rect::new(
        body.x,
        body.y + 1,
        body.width,
        body.height.saturating_sub(1),
    );

    let rows = app.source_rows();
    let mut items = Vec::with_capacity(rows.len());
    for row in &rows {
        match row {
            SourceRow::Header(label) => items.push(ListItem::new(Line::from(Span::styled(
                format!(" {label}"),
                Style::default()
                    .fg(c_overlay0())
                    .add_modifier(Modifier::BOLD),
            )))),
            SourceRow::Local => {
                let marker = if app.is_multi_selected(SelectionScope::Sources, "source:local") {
                    "✓ "
                } else {
                    ""
                };
                items.push(ListItem::new(Line::from(vec![
                    Span::styled(marker, Style::default().fg(c_accent())),
                    Span::styled("Local source", Style::default().fg(c_text())),
                    Span::styled(
                        format!("  {} series", app.local_series_count()),
                        Style::default().fg(c_overlay0()),
                    ),
                ])));
            }
            SourceRow::Remote(index) => {
                let source = &app.remote.sources[*index];
                let mut spans = Vec::new();
                if app.is_multi_selected(SelectionScope::Sources, &format!("source:{}", source.id))
                {
                    spans.push(Span::styled("✓ ", Style::default().fg(c_accent())));
                }
                if app.is_source_pinned(source) {
                    spans.push(Span::styled("★ ", Style::default().fg(c_accent())));
                }
                spans.push(Span::styled(
                    source.display_name(),
                    Style::default().fg(c_text()),
                ));
                spans.push(Span::styled(
                    format!("  {} · {}", source.lang, source.name),
                    Style::default().fg(c_overlay0()),
                ));
                if source.supports_latest {
                    spans.push(Span::styled("  latest", Style::default().fg(c_blue())));
                }
                items.push(ListItem::new(Line::from(spans)));
            }
        }
    }

    app.browse_list_state.select(app.selected_source_row());
    let list = List::new(items)
        .highlight_style(
            Style::default()
                .fg(c_accent())
                .bg(c_surface0())
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("› ");
    frame.render_stateful_widget(list, list_area, &mut app.browse_list_state);
}

fn draw_source_filters(frame: &mut Frame, area: Rect, app: &mut AppState) {
    let block = panel(browse_tab_title(app), c_accent());
    let inner = block.inner(area);
    frame.render_widget(&block, area);
    let body = browse_body_with_status(frame, inner, app);
    if body.height == 0 {
        return;
    }
    draw_browse_path(frame, Rect::new(body.x, body.y, body.width, 1), app);
    let list_area = Rect::new(
        body.x,
        body.y + 1,
        body.width,
        body.height.saturating_sub(1),
    );

    if app.remote.sources.is_empty() {
        frame.render_widget(
            Paragraph::new("No sources loaded. Press r to retry.")
                .alignment(Alignment::Center)
                .style(Style::default().fg(c_overlay1())),
            Rect::new(list_area.x, list_area.y, list_area.width, 1),
        );
        app.source_filter_list_state.select(None);
        return;
    }

    let rows = app.source_filter_rows();
    let items: Vec<ListItem> = rows
        .iter()
        .map(|row| match row {
            SourceFilterRow::Header(label) => ListItem::new(Line::from(Span::styled(
                format!(" {label}"),
                Style::default()
                    .fg(c_overlay0())
                    .add_modifier(Modifier::BOLD),
            ))),
            SourceFilterRow::Language {
                lang,
                enabled,
                source_count,
            } => {
                let label = if lang.is_empty() {
                    "Other".to_owned()
                } else {
                    lang.to_uppercase()
                };
                ListItem::new(Line::from(vec![
                    Span::styled(
                        if *enabled { "[x] " } else { "[ ] " },
                        Style::default().fg(if *enabled { c_green() } else { c_overlay0() }),
                    ),
                    Span::styled(label, Style::default().fg(c_text())),
                    Span::styled(
                        format!("  {source_count} sources"),
                        Style::default().fg(c_overlay0()),
                    ),
                ]))
            }
            SourceFilterRow::Source { index, enabled } => {
                let source = &app.remote.sources[*index];
                let mut spans = vec![
                    Span::styled(
                        if *enabled { "[x] " } else { "[ ] " },
                        Style::default().fg(if *enabled { c_green() } else { c_overlay0() }),
                    ),
                    Span::styled(
                        source.display_name(),
                        Style::default().fg(if *enabled { c_text() } else { c_overlay1() }),
                    ),
                ];
                if app.is_source_pinned(source) {
                    spans.insert(0, Span::styled("★ ", Style::default().fg(c_accent())));
                }
                spans.push(Span::styled(
                    format!("  {}", source.pkg.as_deref().unwrap_or("")),
                    Style::default().fg(c_overlay0()),
                ));
                ListItem::new(Line::from(spans))
            }
        })
        .collect();

    app.source_filter_list_state
        .select(app.selected_source_filter_row());
    let list = List::new(items)
        .highlight_style(
            Style::default()
                .fg(c_accent())
                .bg(c_surface0())
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("› ");
    frame.render_stateful_widget(list, list_area, &mut app.source_filter_list_state);
}

fn draw_source_prefs(frame: &mut Frame, area: Rect, app: &mut AppState) {
    let block = panel(browse_tab_title(app), c_accent());
    let inner = block.inner(area);
    frame.render_widget(&block, area);
    let body = browse_body_with_status(frame, inner, app);
    if body.height == 0 {
        return;
    }
    draw_browse_path(frame, Rect::new(body.x, body.y, body.width, 1), app);
    let list_area = Rect::new(
        body.x,
        body.y + 1,
        body.width,
        body.height.saturating_sub(1),
    );

    let Some(state) = app.source_prefs.as_ref() else {
        frame.render_widget(
            Paragraph::new("No source preferences loaded.")
                .alignment(Alignment::Center)
                .style(Style::default().fg(c_overlay1())),
            Rect::new(list_area.x, list_area.y, list_area.width, 1),
        );
        return;
    };
    if state.preferences.prefs.is_empty() {
        frame.render_widget(
            Paragraph::new("This source has no configurable preferences.")
                .alignment(Alignment::Center)
                .style(Style::default().fg(c_overlay1())),
            Rect::new(list_area.x, list_area.y, list_area.width, 1),
        );
        return;
    }

    let items: Vec<ListItem> = state
        .preferences
        .prefs
        .iter()
        .map(|preference| {
            let (marker, marker_style) = match preference.kind {
                SourcePreferenceKind::Switch => (
                    if preference.switch_value() {
                        "[x] "
                    } else {
                        "[ ] "
                    },
                    Style::default().fg(if preference.switch_value() {
                        c_green()
                    } else {
                        c_overlay0()
                    }),
                ),
                SourcePreferenceKind::Text => ("TXT ", Style::default().fg(c_blue())),
                SourcePreferenceKind::List => ("LIST", Style::default().fg(c_teal())),
                SourcePreferenceKind::Multi => ("MULTI", Style::default().fg(c_mauve())),
                SourcePreferenceKind::Unknown => ("? ", Style::default().fg(c_overlay0())),
            };
            ListItem::new(Line::from(vec![
                Span::styled(marker, marker_style),
                Span::styled(" ", Style::default()),
                Span::styled(
                    preference.title.clone(),
                    Style::default().fg(c_text()).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("  {}", preference.display_value()),
                    Style::default().fg(c_accent()),
                ),
            ]))
        })
        .collect();

    let mut list_state = state.list_state;
    list_state.select((!state.preferences.prefs.is_empty()).then_some(app.browse_selection));
    let list = List::new(items)
        .highlight_style(
            Style::default()
                .fg(c_accent())
                .bg(c_surface0())
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("› ");
    frame.render_stateful_widget(list, list_area, &mut list_state);
    if let Some(state) = app.source_prefs.as_mut() {
        state.list_state = list_state;
    }
}

fn draw_source_pref_picker(frame: &mut Frame, app: &AppState) {
    let Some(picker) = app.source_pref_picker.as_ref() else {
        return;
    };
    let area = frame.area();
    let width = 56.min(area.width).max(20);
    let height = (picker.values.len() as u16 + 2).min(area.height);
    if width < 16 || height < 3 {
        return;
    }
    let popup = Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    );
    let block = panel(
        Line::from(Span::styled(
            format!(" {} ", picker.title),
            Style::default().fg(c_accent()).add_modifier(Modifier::BOLD),
        )),
        c_accent(),
    );
    let inner = block.inner(popup);
    frame.render_widget(Clear, popup);
    frame.render_widget(&block, popup);
    let items: Vec<ListItem> = picker
        .values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let label = picker.labels.get(index).unwrap_or(value);
            ListItem::new(Line::from(vec![
                Span::styled(
                    if index == picker.selected {
                        "› "
                    } else {
                        "  "
                    },
                    Style::default().fg(c_accent()),
                ),
                Span::styled(
                    label.clone(),
                    if index == picker.selected {
                        Style::default().fg(c_accent()).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(c_text())
                    },
                ),
            ]))
        })
        .collect();
    let mut state = ListState::default();
    state.select(Some(picker.selected));
    frame.render_stateful_widget(List::new(items), inner, &mut state);
}

fn draw_filters(frame: &mut Frame, area: Rect, app: &mut AppState) {
    let block = panel(browse_tab_title(app), c_accent());
    let inner = block.inner(area);
    frame.render_widget(&block, area);
    let body = browse_body_with_status(frame, inner, app);
    if body.height == 0 {
        return;
    }
    draw_browse_path(frame, Rect::new(body.x, body.y, body.width, 1), app);
    let list_area = Rect::new(
        body.x,
        body.y + 1,
        body.width,
        body.height.saturating_sub(1),
    );

    if app.remote.filters.is_empty() {
        frame.render_widget(
            Paragraph::new("This source has no filters.")
                .alignment(Alignment::Center)
                .style(Style::default().fg(c_overlay1())),
            Rect::new(list_area.x, list_area.y, list_area.width, 1),
        );
        return;
    }

    let rows = app.filter_rows();
    let items: Vec<ListItem> = rows
        .iter()
        .map(|row| match row {
            FilterRow::Filter { path, depth } => {
                let Some(filter) = app.filter_at_path(path) else {
                    return ListItem::new("");
                };
                let indent = "  ".repeat(*depth);
                match filter {
                    SourceFilter::Header { name } => ListItem::new(Line::from(Span::styled(
                        format!("{indent}{name}"),
                        Style::default().fg(c_accent()).add_modifier(Modifier::BOLD),
                    ))),
                    SourceFilter::Separator { .. } => ListItem::new(Line::from(Span::styled(
                        "─",
                        Style::default().fg(c_surface1()),
                    ))),
                    SourceFilter::Checkbox { name, checked } => ListItem::new(Line::from(vec![
                        Span::styled(
                            if *checked { "[x] " } else { "[ ] " },
                            Style::default().fg(if *checked { c_green() } else { c_overlay0() }),
                        ),
                        Span::styled(format!("{indent}{name}"), Style::default().fg(c_text())),
                    ])),
                    SourceFilter::Tristate { name, .. } => ListItem::new(Line::from(vec![
                        Span::styled("◐ ", Style::default().fg(c_mauve())),
                        Span::styled(
                            format!("{indent}{name}  {}", filter.display_value()),
                            Style::default().fg(c_text()),
                        ),
                    ])),
                    SourceFilter::Text { name, .. } => ListItem::new(Line::from(vec![
                        Span::styled("TXT ", Style::default().fg(c_blue())),
                        Span::styled(
                            format!("{indent}{name}  {}", filter.display_value()),
                            Style::default().fg(c_text()),
                        ),
                    ])),
                    SourceFilter::Select { name, .. } => ListItem::new(Line::from(vec![
                        Span::styled("LIST ", Style::default().fg(c_teal())),
                        Span::styled(
                            format!("{indent}{name}  {}", filter.display_value()),
                            Style::default().fg(c_text()),
                        ),
                    ])),
                    SourceFilter::Sort { name, .. } => ListItem::new(Line::from(vec![
                        Span::styled("SORT ", Style::default().fg(c_peach())),
                        Span::styled(
                            format!("{indent}{name}  {}", filter.display_value()),
                            Style::default().fg(c_text()),
                        ),
                    ])),
                    SourceFilter::Group { name, .. } => ListItem::new(Line::from(vec![
                        Span::styled("▾ ", Style::default().fg(c_overlay0())),
                        Span::styled(
                            format!("{indent}{name}"),
                            Style::default().fg(c_text()).add_modifier(Modifier::BOLD),
                        ),
                    ])),
                    SourceFilter::Unknown { name } => ListItem::new(Line::from(vec![
                        Span::styled("? ", Style::default().fg(c_overlay0())),
                        Span::styled(
                            format!("{indent}{name} (unsupported)"),
                            Style::default().fg(c_overlay1()),
                        ),
                    ])),
                }
            }
            FilterRow::Reset => ListItem::new(Line::from(Span::styled(
                " Reset filters",
                Style::default().fg(c_yellow()),
            ))),
            FilterRow::Apply => ListItem::new(Line::from(Span::styled(
                " Apply filters",
                Style::default().fg(c_accent()).add_modifier(Modifier::BOLD),
            ))),
        })
        .collect();

    app.remote
        .filter_list_state
        .select(app.selected_filter_row());
    let list = List::new(items)
        .highlight_style(
            Style::default()
                .fg(c_accent())
                .bg(c_surface0())
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("› ");
    frame.render_stateful_widget(list, list_area, &mut app.remote.filter_list_state);
}

fn draw_filter_picker(frame: &mut Frame, app: &AppState) {
    let Some(picker) = app.filter_picker.as_ref() else {
        return;
    };
    let area = frame.area();
    let width = 56.min(area.width).max(20);
    let height = (picker.values.len() as u16 + 2).min(area.height);
    if width < 16 || height < 3 {
        return;
    }
    let popup = Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    );
    let block = panel(
        Line::from(Span::styled(
            format!(" {} ", picker.title),
            Style::default().fg(c_accent()).add_modifier(Modifier::BOLD),
        )),
        c_accent(),
    );
    let inner = block.inner(popup);
    frame.render_widget(Clear, popup);
    frame.render_widget(&block, popup);
    let items: Vec<ListItem> = picker
        .values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            ListItem::new(Line::from(vec![
                Span::styled(
                    if index == picker.selected {
                        "› "
                    } else {
                        "  "
                    },
                    Style::default().fg(c_accent()),
                ),
                Span::styled(
                    value.clone(),
                    if index == picker.selected {
                        Style::default().fg(c_accent()).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(c_text())
                    },
                ),
            ]))
        })
        .collect();
    let mut state = ListState::default();
    state.select(Some(picker.selected));
    frame.render_stateful_widget(List::new(items), inner, &mut state);
}

fn draw_extension_details(frame: &mut Frame, area: Rect, app: &mut AppState) {
    let block = panel(browse_tab_title(app), c_accent());
    let inner = block.inner(area);
    frame.render_widget(&block, area);
    let body = browse_body_with_status(frame, inner, app);
    if body.height == 0 {
        return;
    }
    draw_browse_path(frame, Rect::new(body.x, body.y, body.width, 1), app);

    let Some(state) = app.extension_details.as_ref() else {
        frame.render_widget(
            Paragraph::new("No extension selected.")
                .alignment(Alignment::Center)
                .style(Style::default().fg(c_overlay1())),
            Rect::new(body.x, body.y + 2, body.width, 1),
        );
        return;
    };
    let extension = &state.extension;
    let info = vec![
        Line::from(Span::styled(
            extension.name.clone(),
            Style::default().fg(c_accent()).add_modifier(Modifier::BOLD),
        )),
        field("Package", &extension.pkg, c_blue()),
        field(
            "Version",
            &match &extension.available_version {
                Some(available) if extension.update_available() => {
                    format!("{} → {available}", extension.version)
                }
                _ => extension.version.clone(),
            },
            c_green(),
        ),
        field(
            "Store",
            if extension.repo_url.is_empty() {
                "installed only"
            } else {
                &extension.repo_url
            },
            c_overlay1(),
        ),
        field(
            "Sources",
            &format!(
                "{} · {} enabled",
                state.sources.len(),
                state
                    .sources
                    .iter()
                    .filter(|source| app.source_enabled(source))
                    .count()
            ),
            c_teal(),
        ),
        Line::from(""),
    ];
    frame.render_widget(
        Paragraph::new(Text::from(info)),
        Rect::new(body.x, body.y + 1, body.width, 6),
    );

    let list_area = Rect::new(
        body.x,
        body.y + 7,
        body.width,
        body.height.saturating_sub(7),
    );
    let rows = app.extension_detail_rows();
    let items: Vec<ListItem> =
        rows.iter()
            .map(|row| match row {
                crate::app::ExtensionDetailRow::Source(index) => {
                    let Some(source) = state.sources.get(*index) else {
                        return ListItem::new("");
                    };
                    let enabled = app.source_enabled(source);
                    ListItem::new(Line::from(vec![
                        Span::styled(
                            if enabled { "[x] " } else { "[ ] " },
                            Style::default().fg(if enabled { c_green() } else { c_overlay0() }),
                        ),
                        Span::styled(
                            source.display_name(),
                            Style::default().fg(c_text()).add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            format!("  {}", source.pkg.as_deref().unwrap_or("")),
                            Style::default().fg(c_overlay0()),
                        ),
                    ]))
                }
                crate::app::ExtensionDetailRow::EnableAll => ListItem::new(Line::from(
                    Span::styled(" Enable all sources", Style::default().fg(c_green())),
                )),
                crate::app::ExtensionDetailRow::DisableAll => ListItem::new(Line::from(
                    Span::styled(" Disable all sources", Style::default().fg(c_yellow())),
                )),
                crate::app::ExtensionDetailRow::Uninstall => ListItem::new(Line::from(
                    Span::styled(" Uninstall extension", Style::default().fg(c_peach())),
                )),
            })
            .collect();
    if let Some(state) = app.extension_details.as_mut() {
        state
            .list_state
            .select((!rows.is_empty()).then_some(app.browse_selection));
    }
    let list = List::new(items)
        .highlight_style(
            Style::default()
                .fg(c_accent())
                .bg(c_surface0())
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("› ");
    frame.render_stateful_widget(
        list,
        list_area,
        &mut app
            .extension_details
            .as_mut()
            .expect("extension details")
            .list_state,
    );
}

fn draw_remote_grid(frame: &mut Frame, area: Rect, app: &mut AppState) {
    let results = app.visible_remote_results();
    if app.config.source_display_mode == SourceDisplayMode::List {
        draw_remote_result_list(frame, area, app, &results);
        return;
    }
    let columns = grid_columns(area, app.config.thumbnail_width);
    if results.is_empty() {
        let message = if !app.remote.results.is_empty() && app.config.browse_hide_saved {
            "All results are saved. Press h to show them."
        } else {
            "No results."
        };
        frame.render_widget(
            Paragraph::new(message)
                .alignment(Alignment::Center)
                .style(Style::default().fg(c_overlay1())),
            Rect::new(area.x, area.y, area.width, 1),
        );
        app.browse_columns = columns;
        return;
    }
    let cover_height = tile_cover_height(app.config.thumbnail_width, area.height);
    let compact = app.config.source_display_mode == SourceDisplayMode::Compact;
    let tile_height = cover_height + if compact { 1 } else { TILE_TEXT_HEIGHT };
    let selected = app.browse_selection.min(results.len() - 1);
    let visible_rows = grid_visible_rows(area, tile_height);
    let offset = grid_row_offset(
        area,
        columns,
        selected,
        app.browse_grid_offset,
        results.len(),
        tile_height,
    );
    let start = offset.saturating_mul(columns);
    let end = (offset.saturating_add(visible_rows))
        .saturating_mul(columns)
        .min(results.len());

    let source_id = app
        .remote
        .source
        .as_ref()
        .map(|source| source.id.clone())
        .unwrap_or_default();

    for (position, result) in results[start..end].iter().enumerate() {
        let index = start + position;
        let tile = grid_tile_rect(area, columns, offset, index, tile_height);
        let is_selected = index == selected;
        if is_selected {
            frame.render_widget(
                Block::default().style(Style::default().bg(c_surface0())),
                tile,
            );
        }
        if app.is_multi_selected(
            SelectionScope::RemoteResults,
            &format!("remote:{}\n{}", source_id, result.url),
        ) {
            frame.render_widget(
                Paragraph::new("✓")
                    .style(Style::default().fg(c_accent()).add_modifier(Modifier::BOLD)),
                Rect::new(tile.x, tile.y, 1, 1),
            );
        }
        if app.is_favorite(&source_id, &result.url) {
            frame.render_widget(
                Paragraph::new("✓")
                    .style(Style::default().fg(c_green()).add_modifier(Modifier::BOLD)),
                Rect::new(tile.x + tile.width.saturating_sub(1), tile.y, 1, 1),
            );
        }
        let cover_area = Rect::new(tile.x, tile.y, tile.width, cover_height);
        let cover_path =
            app.ensure_remote_cover(&source_id, &result.url, result.thumbnail_url.as_deref());
        let mut rendered = false;
        if let Some(path) = &cover_path {
            if let Some(protocol) = app.tile_covers.get_mut(path) {
                render_centered_image(frame, cover_area, protocol);
                rendered = true;
            }
        } else if result.thumbnail_url.is_none() {
            app.request_series_thumbnail(&source_id, &result.url, result.memo.as_deref());
        }
        if !rendered {
            let initial = result
                .title
                .chars()
                .next()
                .map(|character| character.to_uppercase().collect::<String>())
                .unwrap_or_default();
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
        let title_style = if is_selected {
            Style::default().fg(c_accent()).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(c_text())
        };
        frame.render_widget(
            Paragraph::new(truncate_text(&result.title, tile.width))
                .alignment(Alignment::Center)
                .style(title_style),
            Rect::new(tile.x, tile.y + cover_height, tile.width, 1),
        );
        if !compact {
            frame.render_widget(
                Paragraph::new(result.status.replace('_', " ").to_lowercase())
                    .alignment(Alignment::Center)
                    .style(Style::default().fg(c_overlay0())),
                Rect::new(tile.x, tile.y + cover_height + 1, tile.width, 1),
            );
        }
    }

    app.browse_columns = columns;
    app.browse_grid_offset = offset;
}

fn draw_remote_result_list(
    frame: &mut Frame,
    area: Rect,
    app: &mut AppState,
    results: &[RemoteManga],
) {
    if results.is_empty() {
        let message = if !app.remote.results.is_empty() && app.config.browse_hide_saved {
            "All results are saved. Press h to show them."
        } else {
            "No results."
        };
        frame.render_widget(
            Paragraph::new(message)
                .alignment(Alignment::Center)
                .style(Style::default().fg(c_overlay1())),
            Rect::new(area.x, area.y, area.width, 1),
        );
        app.remote.remote_list_state.select(None);
        return;
    }
    let source_id = app
        .remote
        .source
        .as_ref()
        .map(|source| source.id.clone())
        .unwrap_or_default();
    let selected = app.browse_selection.min(results.len() - 1);
    let items: Vec<ListItem> = results
        .iter()
        .map(|manga| {
            let saved = app.is_favorite(&source_id, &manga.url);
            let mut meta = Vec::new();
            if let Some(author) = manga.author.as_deref() {
                if !author.trim().is_empty() {
                    meta.push(author.to_owned());
                }
            }
            if !manga.status.trim().is_empty() && manga.status != "unknown" {
                meta.push(manga.status.replace('_', " "));
            }
            let meta = if meta.is_empty() {
                String::new()
            } else {
                format!("  {}", meta.join(" · "))
            };
            Line::from(vec![
                Span::styled(
                    if saved { "✓ " } else { "  " },
                    Style::default().fg(c_green()),
                ),
                Span::styled(
                    manga.title.clone(),
                    Style::default().fg(c_text()).add_modifier(Modifier::BOLD),
                ),
                Span::styled(meta, Style::default().fg(c_overlay0())),
            ])
            .into()
        })
        .collect();
    app.remote.remote_list_state.select(Some(selected));
    let list = List::new(items)
        .highlight_style(
            Style::default()
                .fg(c_accent())
                .bg(c_surface0())
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("› ");
    frame.render_stateful_widget(list, area, &mut app.remote.remote_list_state);
}

fn draw_remote_detail(frame: &mut Frame, area: Rect, app: &mut AppState) {
    let layout = library_layout(area);
    let Some(manga) = app.remote.manga.clone() else {
        draw_browse_empty(frame, layout.list, "No manga selected.");
        return;
    };
    let chapters = app.remote.chapters.clone();
    let source = source_display_name(app, &manga.source_id);
    let items = remote_chapter_items(
        &manga.source_id,
        &manga.url,
        &chapters,
        &app.progress,
        &app.bookmarks,
        &app.remote.downloaded,
        app.selected_keys(SelectionScope::RemoteChapters),
    );
    draw_story_chapters(
        frame,
        layout.list,
        &manga.title,
        items,
        chapters.len(),
        app.browse_selection,
        &mut app.browse_list_state,
    );

    let cover_path =
        app.ensure_remote_cover(&manga.source_id, &manga.url, manga.thumbnail_url.as_deref());
    if cover_path.is_none() && manga.thumbnail_url.is_none() {
        app.request_series_thumbnail(&manga.source_id, &manga.url, manga.memo.as_deref());
    }
    let note = if app.is_favorite(&manga.source_id, &manga.url) {
        "saved · F removes"
    } else {
        "not saved · F adds"
    };
    let lines = remote_story_lines(&manga, &chapters, source.as_deref(), note);
    let cover = cover_path
        .as_ref()
        .and_then(|path| app.tile_covers.get_mut(path));
    draw_story_info(frame, layout.details, &manga.title, lines, cover);
}

/// Right-hand metadata shared by Browse and Library remote story screens.
fn remote_story_lines(
    manga: &RemoteManga,
    chapters: &[RemoteChapter],
    source: Option<&str>,
    note: &str,
) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from(Span::styled(
            manga.title.clone(),
            Style::default().fg(c_accent()).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
    ];
    if let Some(author) = &manga.author {
        lines.push(field("Author", author, c_peach()));
    }
    if let Some(artist) = &manga.artist {
        lines.push(field("Artist", artist, c_mauve()));
    }
    if let Some(source) = source {
        lines.push(field("Source", source, c_blue()));
    }
    lines.push(field("Status", &manga.status, c_green()));
    if !manga.genre.is_empty() {
        lines.push(field("Tags", &manga.genre.join(", "), c_teal()));
    }
    lines.push(field("Chapters", &chapters.len().to_string(), c_green()));
    if let Some(latest) = chapters.first() {
        lines.push(field("Latest", &latest.name, c_blue()));
    }
    lines.push(field("Library", note, c_yellow()));
    lines.push(Line::from(""));

    let description = manga.description.as_deref().unwrap_or("").trim();
    if description.is_empty() {
        lines.push(Line::from(Span::styled(
            "No description.",
            Style::default().fg(c_overlay1()),
        )));
    } else {
        lines.push(Line::from(description.to_owned()));
    }
    lines
}

fn source_display_name(app: &AppState, source_id: &str) -> Option<String> {
    app.remote
        .sources
        .iter()
        .find(|source| source.id == source_id)
        .map(RemoteSource::display_name)
}

fn draw_extensions(frame: &mut Frame, area: Rect, app: &mut AppState) {
    let block = panel(browse_tab_title(app), c_accent());
    let inner = block.inner(area);
    frame.render_widget(&block, area);
    let body = browse_body_with_status(frame, inner, app);
    if body.height < 2 {
        return;
    }
    draw_extension_search(frame, Rect::new(body.x, body.y, body.width, 1), app);
    draw_browse_path(frame, Rect::new(body.x, body.y + 1, body.width, 1), app);
    let list_area = Rect::new(
        body.x,
        body.y + 2,
        body.width,
        body.height.saturating_sub(2),
    );

    let rows = app.extension_rows();
    if app.filtered_extension_count() == 0 {
        let message = if app.remote.extensions.is_empty() {
            "No extensions. Press a to add a store URL."
        } else {
            "No extensions match the search."
        };
        frame.render_widget(
            Paragraph::new(message)
                .alignment(Alignment::Center)
                .style(Style::default().fg(c_overlay1())),
            Rect::new(list_area.x, list_area.y, list_area.width, 1),
        );
        app.browse_list_state.select(None);
        return;
    }

    let items: Vec<ListItem> = rows
        .iter()
        .map(|row| match row {
            ExtensionRow::Header(label) => ListItem::new(Line::from(Span::styled(
                format!(" {label}"),
                Style::default()
                    .fg(c_overlay0())
                    .add_modifier(Modifier::BOLD),
            ))),
            ExtensionRow::Extension(index) => {
                let extension = &app.remote.extensions[*index];
                let status = if extension.update_available() {
                    "update"
                } else if extension.installed {
                    "installed"
                } else {
                    "available"
                };
                let nsfw = if extension.nsfw { " · 18+" } else { "" };
                let mut spans = Vec::new();
                if app.is_multi_selected(
                    SelectionScope::Extensions,
                    &format!("ext:{}", extension.pkg),
                ) {
                    spans.push(Span::styled("✓ ", Style::default().fg(c_accent())));
                }
                if app.is_extension_pinned(extension) {
                    spans.push(Span::styled("★ ", Style::default().fg(c_accent())));
                }
                spans.push(Span::raw(extension.name.clone()));
                spans.push(Span::styled(
                    format!(
                        "  {} · {}{nsfw} · {} src",
                        extension.lang, extension.version, extension.source_count
                    ),
                    Style::default().fg(c_overlay0()),
                ));
                if let Some(available) = &extension.available_version {
                    if extension.update_available() {
                        spans.push(Span::styled(
                            format!(" → {available}"),
                            Style::default().fg(c_peach()),
                        ));
                    }
                }
                spans.push(Span::styled(
                    format!("  {status}"),
                    Style::default().fg(if extension.update_available() {
                        c_peach()
                    } else if extension.installed {
                        c_green()
                    } else {
                        c_overlay1()
                    }),
                ));
                ListItem::new(Line::from(spans))
            }
        })
        .collect();

    app.browse_list_state.select(app.selected_extension_row());
    let list = List::new(items)
        .highlight_style(
            Style::default()
                .fg(c_accent())
                .bg(c_surface0())
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("› ");
    frame.render_stateful_widget(list, list_area, &mut app.browse_list_state);
}

fn draw_extension_search(frame: &mut Frame, area: Rect, app: &AppState) {
    let (text, style) = if app.extension_search {
        (
            format!(" Search: {}▏", app.extension_query),
            Style::default().fg(c_accent()).add_modifier(Modifier::BOLD),
        )
    } else if app.extension_query.trim().is_empty() {
        (
            " / search extensions".to_owned(),
            Style::default().fg(c_overlay0()),
        )
    } else {
        (
            format!(" Search: {}  (/ to edit, Esc clears)", app.extension_query),
            Style::default().fg(c_text()),
        )
    };
    frame.render_widget(Paragraph::new(text).style(style), area);
}

fn draw_stores(frame: &mut Frame, area: Rect, app: &mut AppState) {
    let block = panel(browse_tab_title(app), c_accent());
    let inner = block.inner(area);
    frame.render_widget(&block, area);
    let body = browse_body_with_status(frame, inner, app);
    if body.height == 0 {
        return;
    }
    draw_browse_path(frame, Rect::new(body.x, body.y, body.width, 1), app);
    let list_area = Rect::new(
        body.x,
        body.y + 1,
        body.width,
        body.height.saturating_sub(1),
    );

    let stores = app.remote.repos.clone();
    if stores.is_empty() {
        frame.render_widget(
            Paragraph::new("No extension stores. Press a to add one.")
                .alignment(Alignment::Center)
                .style(Style::default().fg(c_overlay1())),
            Rect::new(list_area.x, list_area.y, list_area.width, 1),
        );
        app.browse_list_state.select(None);
        return;
    }
    let items: Vec<ListItem> = stores
        .iter()
        .map(|store| {
            let marker = if app.is_multi_selected(SelectionScope::Stores, &format!("store:{store}"))
            {
                "✓ "
            } else {
                ""
            };
            ListItem::new(Line::from(vec![
                Span::styled(marker, Style::default().fg(c_accent())),
                Span::styled(store.clone(), Style::default().fg(c_text())),
            ]))
        })
        .collect();
    app.browse_list_state
        .select(Some(app.browse_selection.min(stores.len() - 1)));
    let list = List::new(items)
        .highlight_style(
            Style::default()
                .fg(c_accent())
                .bg(c_surface0())
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("› ");
    frame.render_stateful_widget(list, list_area, &mut app.browse_list_state);
}

fn draw_migrate(frame: &mut Frame, area: Rect, app: &AppState) {
    let block = panel(browse_tab_title(app), c_accent());
    let inner = block.inner(area);
    frame.render_widget(&block, area);
    let body = browse_body_with_status(frame, inner, app);
    if body.height == 0 {
        return;
    }
    frame.render_widget(
        Paragraph::new("Migrating between sources is not implemented yet.")
            .alignment(Alignment::Center)
            .style(Style::default().fg(c_overlay1())),
        Rect::new(body.x, body.y + body.height / 2, body.width, 1),
    );
}

fn draw_browse_chapters(frame: &mut Frame, area: Rect, app: &mut AppState) {
    let layout = library_layout(area);
    let Some(series) = app.selected_browse_local_series().cloned() else {
        draw_browse_empty(frame, layout.list, "Series not found. Rescan the library.");
        return;
    };
    let items = local_chapter_items(
        &series.chapters,
        &app.progress,
        &app.bookmarks,
        app.selected_keys(SelectionScope::LocalChapters),
    );
    draw_story_chapters(
        frame,
        layout.list,
        &series.title,
        items,
        series.chapters.len(),
        app.browse_local_chapter,
        &mut app.browse_local_list_state,
    );
    let cover_path = series.cover.clone();
    if let Some(path) = &cover_path {
        app.ensure_tile_cover(path);
    }
    let cover = cover_path
        .as_ref()
        .and_then(|path| app.tile_covers.get_mut(path));
    draw_story_info(
        frame,
        layout.details,
        &series.title,
        series_lines(&series),
        cover,
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
            if let Some(scroll) = reader.scroll.as_mut() {
                let area = chunks[1];
                let offset = scroll.offset;
                // Rendering a fresh kitty block copies a multi-megabyte transmission
                // string into the frame buffer. Show one new block per frame so a
                // scroll step cannot stall input for seconds.
                let mut budget = 1usize;
                let mut deferred = false;
                for index in 0..scroll.layout.len() {
                    let page = scroll.layout[index];
                    let top = i64::from(page.start) - i64::from(offset);
                    if top >= i64::from(area.height) {
                        break;
                    }
                    if top + i64::from(page.rows) <= 0 {
                        continue;
                    }
                    if !scroll.protocols.get(index).is_some_and(Option::is_some) {
                        continue;
                    }
                    if scroll.fresh.get(index).copied().unwrap_or(false) {
                        if budget == 0 {
                            deferred = true;
                            continue;
                        }
                        budget -= 1;
                        if let Some(fresh) = scroll.fresh.get_mut(index) {
                            *fresh = false;
                        }
                    }

                    let x = area.width.saturating_sub(page.cell_width) / 2;
                    if let Some(protocol) = scroll.protocols.get(index).and_then(Option::as_ref) {
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
                }
                scroll.deferred_transmits = deferred;
            } else if let Some(protocol) = reader.protocol.as_mut() {
                frame.render_stateful_widget(
                    StatefulImage::new().resize(Resize::Fit(None)),
                    chunks[1],
                    protocol,
                );
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
    let (title, chapters) = if app.view == LibraryView::RemoteDetails {
        (
            app.library_remote
                .manga
                .as_ref()
                .map(|manga| manga.title.as_str())
                .unwrap_or("Saved manga"),
            app.library_remote.chapters.len(),
        )
    } else {
        (
            app.selected_series()
                .map(|series| series.title.as_str())
                .unwrap_or("Series"),
            app.chapter_count(),
        )
    };
    let line = Line::from(vec![
        Span::raw(" "),
        Span::styled("Library", Style::default().fg(c_overlay1())),
        Span::styled(" › ", Style::default().fg(c_overlay0())),
        Span::styled(
            title.to_owned(),
            Style::default().fg(c_accent()).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("  {chapters} chapters"),
            Style::default().fg(c_overlay0()),
        ),
        Span::styled("   ↵ open · Esc back", Style::default().fg(c_overlay0())),
    ]);
    let mut line = line;
    if let Some(status) = &app.status {
        line.spans.push(Span::styled(
            format!("   {status}"),
            Style::default().fg(c_peach()),
        ));
    }
    frame.render_widget(Paragraph::new(line), area);
}

fn draw_story_chapters(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    items: Vec<ListItem<'static>>,
    count: usize,
    selected: usize,
    list_state: &mut ListState,
) {
    if count == 0 {
        list_state.select(None);
    } else {
        list_state.select(Some(selected.min(count - 1)));
    }

    let list = List::new(items)
        .block(panel(
            Line::from(vec![
                Span::styled(
                    format!(" {title} · Chapters "),
                    Style::default().fg(c_accent()).add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!("{count} "), Style::default().fg(c_overlay0())),
            ]),
            c_accent(),
        ))
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

fn chapter_marker_spans(bookmarked: bool, downloaded: Option<bool>) -> Vec<Span<'static>> {
    let bookmark = if bookmarked {
        Span::styled("★ ", Style::default().fg(c_yellow()))
    } else {
        Span::styled("☆ ", Style::default().fg(c_overlay0()))
    };
    let download = match downloaded {
        Some(true) => Span::styled("⬇ ", Style::default().fg(c_green())),
        Some(false) => Span::styled("↓ ", Style::default().fg(c_overlay0())),
        None => Span::styled("· ", Style::default().fg(c_overlay0())),
    };
    vec![bookmark, download]
}

fn local_chapter_items(
    chapters: &[Chapter],
    progress: &ProgressStore,
    bookmarks: &BookmarkStore,
    selected: Option<&HashSet<String>>,
) -> Vec<ListItem<'static>> {
    chapters
        .iter()
        .map(|chapter| {
            let mut spans = chapter_marker_spans(bookmarks.is_bookmarked(&chapter.path), None);
            if selected
                .is_some_and(|keys| keys.contains(&format!("chapter:{}", chapter.path.display())))
            {
                spans.push(Span::styled("✓ ", Style::default().fg(c_accent())));
            }
            spans.push(Span::raw(chapter.title.clone()));
            spans.push(Span::styled(
                format!("  {}", chapter_number(chapter.number)),
                Style::default().fg(c_overlay0()),
            ));
            if let Some(entry) = progress.entry(&chapter.path) {
                spans.push(Span::styled(
                    format!("  {}/{}", entry.page + 1, entry.pages),
                    Style::default().fg(c_teal()),
                ));
            }
            ListItem::new(Line::from(spans))
        })
        .collect()
}

fn remote_chapter_items(
    source_id: &str,
    manga_url: &str,
    chapters: &[RemoteChapter],
    progress: &ProgressStore,
    bookmarks: &BookmarkStore,
    downloaded: &[bool],
    selected: Option<&HashSet<String>>,
) -> Vec<ListItem<'static>> {
    chapters
        .iter()
        .enumerate()
        .map(|(index, chapter)| {
            let key = remote_progress_key(source_id, manga_url, &chapter.url);
            let mut spans = chapter_marker_spans(
                bookmarks.is_bookmarked(&key),
                Some(downloaded.get(index).copied().unwrap_or(false)),
            );
            let selection_key = format!("rchap:{source_id}\n{manga_url}\n{}", chapter.url);
            if selected.is_some_and(|keys| keys.contains(&selection_key)) {
                spans.push(Span::styled("✓ ", Style::default().fg(c_accent())));
            }
            spans.push(Span::raw(chapter.name.clone()));
            if let Some(scanlator) = chapter
                .scanlator
                .as_deref()
                .filter(|value| !value.is_empty())
            {
                spans.push(Span::styled(
                    format!("  {scanlator}"),
                    Style::default().fg(c_overlay1()),
                ));
            }
            if let Some(entry) = progress.entry(&key) {
                spans.push(Span::styled(
                    format!("  {}/{}", entry.page + 1, entry.pages),
                    Style::default().fg(c_teal()),
                ));
            }
            ListItem::new(Line::from(spans))
        })
        .collect()
}

/// Right-hand pane shared by every story screen: cover thumbnail above the metadata.
fn draw_story_info(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    lines: Vec<Line<'static>>,
    cover: Option<&mut StatefulProtocol>,
) {
    let block = panel(
        Line::from(Span::styled(
            " Details ",
            Style::default().fg(c_blue()).add_modifier(Modifier::BOLD),
        )),
        c_surface1(),
    );
    let inner = block.inner(area);
    frame.render_widget(&block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let cover_height = (u32::from(inner.height) * 2 / 5).clamp(6, 16) as u16;
    let cover_height = cover_height.min(inner.height.saturating_sub(3));
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(cover_height), Constraint::Min(0)])
        .split(inner);

    let mut rendered = false;
    if let Some(protocol) = cover {
        render_centered_image(frame, chunks[0], protocol);
        rendered = true;
    }
    if !rendered && chunks[0].height > 0 {
        let initial = title
            .chars()
            .next()
            .map(|character| character.to_uppercase().collect::<String>())
            .unwrap_or_default();
        frame.render_widget(
            Paragraph::new(initial)
                .alignment(Alignment::Center)
                .style(Style::default().fg(c_accent()).add_modifier(Modifier::BOLD)),
            Rect::new(
                chunks[0].x,
                chunks[0].y + chunks[0].height / 2,
                chunks[0].width,
                1,
            ),
        );
    }

    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .style(Style::default().fg(c_subtext1()))
            .wrap(Wrap { trim: true }),
        chunks[1],
    );
}

fn chapter_number(number: Option<f32>) -> String {
    match number {
        Some(number) if number.fract() == 0.0 => format!("{}", number as i64),
        Some(number) => format!("{number}"),
        None => "—".to_owned(),
    }
}

fn in_category(series: &Series, category: Option<&str>) -> bool {
    category.is_none_or(|category| series.categories.iter().any(|item| item == category))
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
    lines.push(field("Source", "Local library", c_blue()));
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
        MoreSection::Downloads => draw_more_downloads(frame, layout.content, app),
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
        SettingId::HostUrl => (
            "Extension host".to_owned(),
            app.config
                .host_url
                .clone()
                .unwrap_or_else(|| "not set".to_owned()),
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

fn draw_more_downloads(frame: &mut Frame, area: Rect, app: &AppState) {
    let root = downloads_dir(&app.config)
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "(unavailable)".to_owned());
    let lines = vec![
        Line::from(Span::styled(
            "Chapter downloads",
            Style::default().fg(c_accent()).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from("Select a remote chapter and press D, or press D in the reader."),
        Line::from("Downloaded chapters are read from disk, even when the source is offline."),
        Line::from(""),
        field("Folder", &root, c_blue()),
        field(
            "Layout",
            "<source>/<series>/<chapter>/<page>.<ext>",
            c_teal(),
        ),
    ];
    let block = panel(
        Line::from(Span::styled(
            " Downloads ",
            Style::default().fg(c_accent()).add_modifier(Modifier::BOLD),
        )),
        c_accent(),
    );
    let inner = block.inner(area);
    frame.render_widget(block, area);
    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .style(Style::default().fg(c_subtext1()))
            .wrap(Wrap { trim: true }),
        inner,
    );
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
    let mut lines = vec![
        field("Version", env!("CARGO_PKG_VERSION"), c_text()),
        field("Graphics", app.protocol_name(), c_blue()),
        field("Reader", mode, c_blue()),
        field("Zoom", &format!("{}%", app.zoom), c_green()),
        field("Host", &app.host_summary(), c_teal()),
    ];
    if let Some(path) = app.host_data_path() {
        lines.push(field("Data dir", &display_path(&path), c_overlay1()));
    }
    if let Some(path) = app.host_log_path() {
        lines.push(field("Host log", &display_path(&path), c_overlay1()));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "H starts or stops the extension host.",
        Style::default().fg(c_overlay1()),
    )));
    lines.push(Line::from(Span::styled(
        "Tsuiyomi is an independent Mihon-compatible TUI.",
        Style::default().fg(c_overlay1()),
    )));
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
    use tsuiyomi_core::{
        Chapter, ChapterKind, ExtensionInfo, Library, Page, PageSource, RemoteChapter, RemoteManga,
        RemoteSource, Series, SourcePreference, SourcePreferenceKind, SourcePreferences,
    };

    use crate::app::{ReaderState, SourcePrefsState};

    use super::*;

    fn test_app() -> AppState {
        let series = Series {
            path: PathBuf::from("/tmp/library/Test Series"),
            remote: None,
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
        assert!(text.contains("Details"));
        assert!(text.contains("Local library"));
        assert!(text.contains("Esc back"));
    }

    #[test]
    fn reader_view_renders_status_and_error() {
        let mut app = test_app();
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
        assert_eq!(app.tab, 3);
        assert_eq!(app.browse_view, BrowseView::Chapters);
        assert!(text.contains("Chapters"));
        assert!(text.contains("Chapter 1"));
        assert!(text.contains("Details"));
        assert!(text.contains("Test Series"));
    }

    #[test]
    fn extension_details_screen_renders_sources_and_actions() {
        let mut app = test_app();
        app.tab = 3;
        app.browse_view = BrowseView::ExtensionDetails;
        app.extension_details = Some(crate::app::ExtensionDetailsState {
            extension: ExtensionInfo {
                name: "MangaDex".to_owned(),
                pkg: "pkg.mangadex".to_owned(),
                lang: "en".to_owned(),
                version: "1.0".to_owned(),
                apk: "x.apk".to_owned(),
                installed: true,
                nsfw: false,
                source_count: 1,
                sources: Vec::new(),
                repo_url: "https://example.org/index.min.json".to_owned(),
                available_version: None,
            },
            sources: vec![RemoteSource {
                id: "1".to_owned(),
                name: "MangaDex".to_owned(),
                lang: "en".to_owned(),
                pkg: Some("pkg.mangadex".to_owned()),
                supports_latest: true,
            }],
            list_state: ListState::default(),
        });

        let text = render(&mut app, 100, 30);
        assert!(text.contains("MangaDex"));
        assert!(text.contains("Package"));
        assert!(text.contains("pkg.mangadex"));
        assert!(text.contains("Enable all sources"));
        assert!(text.contains("Disable all sources"));
        assert!(text.contains("Uninstall extension"));
    }

    #[test]
    fn duplicate_and_remove_dialogs_render() {
        let mut app = test_app();
        app.duplicate_dialog = Some(crate::app::DuplicateDialog {
            manga: RemoteManga {
                source_id: "1".to_owned(),
                url: "/manga/alpha".to_owned(),
                title: "Alpha".to_owned(),
                author: None,
                artist: None,
                description: None,
                genre: Vec::new(),
                status: "unknown".to_owned(),
                thumbnail_url: None,
                memo: None,
            },
            duplicates: vec![tsuiyomi_core::RemoteFavorite {
                source_id: "2".to_owned(),
                url: "/manga/alpha2".to_owned(),
                title: "Alpha".to_owned(),
                thumbnail_url: None,
                last_read_url: None,
                last_read_name: None,
                categories: Vec::new(),
            }],
            selected: 0,
        });
        let text = render(&mut app, 100, 30);
        assert!(text.contains("Possible duplicate"));
        assert!(text.contains("add anyway"));

        app.duplicate_dialog = None;
        app.remove_favorite = Some(RemoteManga {
            source_id: "1".to_owned(),
            url: "/manga/alpha".to_owned(),
            title: "Alpha".to_owned(),
            author: None,
            artist: None,
            description: None,
            genre: Vec::new(),
            status: "unknown".to_owned(),
            thumbnail_url: None,
            memo: None,
        });
        let text = render(&mut app, 100, 30);
        assert!(text.contains("Remove from library"));
        assert!(text.contains("Remove Alpha?"));
    }

    #[test]
    fn remote_results_list_mode_marks_saved_manga() {
        let mut app = test_app();
        app.tab = 3;
        app.browse_view = BrowseView::RemoteResults;
        app.config.source_display_mode = SourceDisplayMode::List;
        app.remote.source = Some(RemoteSource {
            id: "1".to_owned(),
            name: "Demo".to_owned(),
            lang: "en".to_owned(),
            pkg: Some("pkg.demo".to_owned()),
            supports_latest: true,
        });
        app.remote.results = vec![
            RemoteManga {
                source_id: "1".to_owned(),
                url: "/manga/Alpha".to_owned(),
                title: "Alpha".to_owned(),
                author: Some("Author".to_owned()),
                artist: None,
                description: None,
                genre: Vec::new(),
                status: "ongoing".to_owned(),
                thumbnail_url: None,
                memo: None,
            },
            RemoteManga {
                source_id: "1".to_owned(),
                url: "/manga/Beta".to_owned(),
                title: "Beta".to_owned(),
                author: None,
                artist: None,
                description: None,
                genre: Vec::new(),
                status: "completed".to_owned(),
                thumbnail_url: None,
                memo: None,
            },
        ];
        app.remote.favorites = vec![tsuiyomi_core::RemoteFavorite {
            source_id: "1".to_owned(),
            url: "/manga/Alpha".to_owned(),
            title: "Alpha".to_owned(),
            thumbnail_url: None,
            last_read_url: None,
            last_read_name: None,
            categories: Vec::new(),
        }];

        let text = render(&mut app, 100, 30);
        assert!(text.contains("Alpha"));
        assert!(text.contains("Beta"));
        assert!(text.contains("✓"));
        assert!(text.contains("Author"));
    }

    #[test]
    fn filter_screen_renders_filter_controls() {
        let mut app = test_app();
        app.tab = 3;
        app.browse_view = BrowseView::Filters;
        app.remote.filters = vec![
            SourceFilter::Header {
                name: "Tags".to_owned(),
            },
            SourceFilter::Checkbox {
                name: "Completed".to_owned(),
                checked: true,
            },
            SourceFilter::Sort {
                name: "Sort".to_owned(),
                values: vec!["Name".to_owned(), "Date".to_owned()],
                index: Some(0),
                ascending: Some(false),
            },
        ];
        app.remote.filters_loaded = true;

        let text = render(&mut app, 100, 30);
        assert!(text.contains("Filters"));
        assert!(text.contains("Tags"));
        assert!(text.contains("[x]"));
        assert!(text.contains("Completed"));
        assert!(text.contains("Name ↓"));
        assert!(text.contains("Reset filters"));
        assert!(text.contains("Apply filters"));
    }

    #[test]
    fn source_preferences_screen_renders_values() {
        let mut app = test_app();
        app.tab = 3;
        app.browse_view = BrowseView::SourcePrefs;
        app.source_prefs = Some(SourcePrefsState {
            source: RemoteSource {
                id: "1".to_owned(),
                name: "Demo".to_owned(),
                lang: "en".to_owned(),
                pkg: Some("pkg.demo".to_owned()),
                supports_latest: true,
            },
            preferences: SourcePreferences {
                source_id: "1".to_owned(),
                name: "Demo".to_owned(),
                configurable: true,
                prefs: vec![
                    SourcePreference {
                        key: Some("nsfw".to_owned()),
                        title: "Show NSFW".to_owned(),
                        summary: None,
                        kind: SourcePreferenceKind::Switch,
                        value: Some("true".to_owned()),
                        values: Vec::new(),
                        labels: Vec::new(),
                    },
                    SourcePreference {
                        key: Some("lang".to_owned()),
                        title: "Language".to_owned(),
                        summary: None,
                        kind: SourcePreferenceKind::List,
                        value: Some("en".to_owned()),
                        values: vec!["en".to_owned(), "ja".to_owned()],
                        labels: vec!["English".to_owned(), "Japanese".to_owned()],
                    },
                ],
                error: None,
            },
            list_state: ListState::default(),
        });

        let text = render(&mut app, 100, 30);
        assert!(text.contains("Source preferences"));
        assert!(text.contains("Show NSFW"));
        assert!(text.contains("[x]"));
        assert!(text.contains("Language"));
        assert!(text.contains("English"));
    }

    #[test]
    fn source_filter_screen_renders_languages_and_sources() {
        let mut app = test_app();
        app.tab = 3;
        app.remote.sources = vec![
            RemoteSource {
                id: "1".to_owned(),
                name: "Alpha".to_owned(),
                lang: "en".to_owned(),
                pkg: Some("pkg.a".to_owned()),
                supports_latest: true,
            },
            RemoteSource {
                id: "2".to_owned(),
                name: "Beta".to_owned(),
                lang: "ja".to_owned(),
                pkg: Some("pkg.b".to_owned()),
                supports_latest: false,
            },
        ];
        app.browse_view = BrowseView::SourceFilters;

        let text = render(&mut app, 100, 30);
        assert!(text.contains("Source filters"));
        assert!(text.contains("[x] EN"));
        assert!(text.contains("[x] JA"));
        assert!(text.contains("Alpha (EN)"));
        assert!(text.contains("Beta (JA)"));
        assert!(text.contains("toggle"));
    }

    #[test]
    fn browse_renders_remote_sources_results_and_extensions() {
        let mut app = test_app();
        app.tab = 3;
        app.remote.sources = vec![RemoteSource {
            id: "7".to_owned(),
            name: "MangaDex".to_owned(),
            lang: "en".to_owned(),
            pkg: None,
            supports_latest: true,
        }];
        let text = render(&mut app, 80, 30);
        assert!(text.contains("MangaDex (EN)"));

        app.browse_view = BrowseView::RemoteResults;
        app.remote.results = vec![RemoteManga {
            source_id: "7".to_owned(),
            url: "/manga/3".to_owned(),
            title: "Berserk".to_owned(),
            author: Some("Miura".to_owned()),
            artist: None,
            description: Some("Dark fantasy".to_owned()),
            genre: vec!["Action".to_owned()],
            status: "ongoing".to_owned(),
            thumbnail_url: None,
            memo: None,
        }];
        let text = render(&mut app, 80, 30);
        assert!(text.contains("Berserk"));

        app.browse_view = BrowseView::RemoteDetails;
        app.browse_selection = 0;
        app.remote.manga = app.remote.results.first().cloned();
        app.remote.chapters = vec![RemoteChapter {
            url: "/ch/1".to_owned(),
            name: "Chapter 1".to_owned(),
            chapter_number: 1.0,
            scanlator: None,
            upload_date: 0,
            memo: None,
        }];
        let text = render(&mut app, 80, 30);
        assert!(text.contains("Chapters"));
        assert!(text.contains("Chapter 1"));

        app.browse_view = BrowseView::Extensions;
        app.remote.extensions = vec![ExtensionInfo {
            name: "MangaDex".to_owned(),
            pkg: "eu.kanade.tachiyomi.extension.en.mangadex".to_owned(),
            lang: "en".to_owned(),
            version: "1.5.0".to_owned(),
            apk: "x.apk".to_owned(),
            installed: true,
            nsfw: false,
            source_count: 1,
            sources: Vec::new(),
            repo_url: "https://example.org/index.min.json".to_owned(),
            available_version: Some("1.6.0".to_owned()),
        }];
        let text = render(&mut app, 80, 30);
        assert!(text.contains("update"));
        assert!(text.contains("→ 1.6.0"));
        assert!(text.contains("Extensions (1)"));
    }

    #[test]
    fn text_input_modal_renders() {
        let mut app = test_app();
        app.open_text_input(
            crate::app::TextAction::AddExtensionStore,
            "Extension store URL",
            "https://example.org/index.min.json",
        );
        let text = render(&mut app, 80, 30);
        assert!(text.contains("Extension store URL"));
        assert!(text.contains("https://example.org/index.min.json"));
    }

    #[test]
    fn library_story_screen_renders_the_cover_and_info() {
        let root = tempfile::tempdir().unwrap();
        let cover = root.path().join("cover.jpg");
        image::RgbImage::from_pixel(20, 30, image::Rgb([120, 80, 160]))
            .save(&cover)
            .unwrap();

        let mut app = test_app();
        app.set_image_picker(ratatui_image::picker::Picker::halfblocks());
        app.library.series[0].cover = Some(cover.clone());
        app.view = LibraryView::Chapters;

        let text = render(&mut app, 100, 30);
        assert!(app.tile_covers.contains_key(&cover));
        assert!(text.contains("Test Series"));
        assert!(text.contains("Author"));
        // Chapter list on the left, story details on the right.
        assert!(text.contains("Chapters"));
        assert!(text.contains("Details"));
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
    fn history_tab_renders_entries_and_empty_state() {
        let mut app = test_app();
        app.tab = 2;

        let text = render(&mut app, 100, 30);
        assert!(text.contains("No reading history yet."));

        app.history.record(
            "chapter:1".to_owned(),
            tsuiyomi_core::HistoryEntry {
                series_title: "Test Series".to_owned(),
                chapter_title: "Chapter 1".to_owned(),
                source: "Local".to_owned(),
                cover: None,
                local: None,
                remote: None,
                page: 2,
                pages: 10,
                last_read: 1,
            },
        );
        let text = render(&mut app, 100, 30);
        assert!(text.contains("Test Series"));
        assert!(text.contains("Chapter 1"));
        assert!(text.contains("3/10"));
    }

    #[test]
    fn library_delete_dialog_renders_the_options() {
        let mut app = test_app();
        app.library_delete = Some(crate::app::LibraryDeleteDialog {
            items: vec![crate::app::LibraryDeleteItem {
                path: PathBuf::from("/tmp/library/Test Series"),
                title: "Test Series".to_owned(),
                remote: Some(tsuiyomi_core::RemoteSeries {
                    source_id: "7".to_owned(),
                    url: "/manga/test".to_owned(),
                    thumbnail_url: None,
                    memo: None,
                }),
            }],
            remove_from_library: true,
            delete_downloads: false,
            focus: 0,
        });

        let text = render(&mut app, 100, 30);

        assert!(text.contains("Remove Test Series?"));
        assert!(text.contains("[x] Manga from library"));
        assert!(text.contains("[ ] Downloaded chapters"));
    }

    #[test]
    fn uninstall_confirmation_renders_the_extension_name() {
        let mut app = test_app();
        app.uninstall_confirm = Some(vec![("pkg.asura".to_owned(), "Asura Scans".to_owned())]);

        let text = render(&mut app, 100, 30);

        assert!(text.contains("Uninstall Asura Scans?"));
        assert!(text.contains("y  uninstall"));
    }

    #[test]
    fn quit_confirmation_renders_as_a_centered_panel() {
        let mut app = test_app();
        app.quit_confirm = true;

        let text = render(&mut app, 80, 24);
        assert!(text.contains("Quit tsuiyomi?"));
        assert!(text.contains("exit"));
        assert!(text.contains("cancel"));
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
