use std::{
    collections::{BTreeMap, HashSet},
    path::PathBuf,
};

use chrono::{DateTime, Local};
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
    date_key_for_unix, downloads_dir, parse_chapter_number, read_state, remote_cover_path,
    today_key, BookmarkStore, Chapter, ChapterDisplay, ChapterSettings, HistoryEntry, LibrarySort,
    ProgressStore, ReadState, ReadingMode, RemoteChapter, RemoteManga, RemoteSource, Series,
    SourceDisplayMode, SourceFilter, SourcePreferenceKind, Theme, UpdateEntry,
};

use crate::{
    app::{
        AppState, BrowseTab, BrowseView, ChapterSettingRow, DownloadState, ExtensionRow, FilterRow,
        HistorySection, LibraryView, MoreFocus, MoreRow, MoreSection, PickerKind, PickerState,
        ReaderMode, SelectionScope, SettingId, SettingsPage, SourceFilterRow, SourceRow,
        MORE_SECTIONS, SETTINGS_PAGES,
    },
    layout::{
        app_layout_with_commands, grid_columns, grid_margin, grid_row_offset, grid_tile_rect,
        grid_tile_width, grid_visible_rows, library_grid_area, library_layout, more_layout,
        reader_bar_layout, setting_columns, settings_rows, tab_layouts, tile_cover_height,
        TAB_NAMES, TILE_GAP, TILE_TEXT_HEIGHT,
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

    let layout = app_layout_with_commands(area, app.fullscreen, commands_height(app, area));

    if let Some(header) = layout.header {
        draw_header(frame, header);
    }

    if app.reader.is_some() {
        draw_reader(frame, layout.content, app);
        draw_reader_bar(frame, layout.tabs, app);
    } else {
        match app.tab {
            0 => draw_library(frame, layout.content, app),
            1 => draw_updates(frame, layout.content, app),
            2 => draw_history(frame, layout.content, app),
            3 => draw_browse(frame, layout.content, app),
            _ => draw_more(frame, layout.content, app),
        }
        draw_tabs(frame, layout.tabs, app.tab, app.updates_count());
    }

    if let Some(commands) = layout.commands {
        draw_commands(frame, commands, app);
    }

    draw_cover_overlay(frame, app);
    draw_picker(frame, app);
    draw_source_pref_picker(frame, app);
    draw_filter_picker(frame, app);
    draw_quit_confirm(frame, app);
    draw_uninstall_confirm(frame, app);
    draw_library_delete(frame, app);
    draw_duplicate_dialog(frame, app);
    draw_remove_favorite(frame, app);
    draw_history_delete(frame, app);
    draw_history_clear_confirm(frame, app);
    draw_text_input(frame, app);
    draw_chapter_settings(frame, app);
    draw_download_menu(frame, app);
    draw_download_delete_confirm(frame, app);
    draw_category_picker(frame, app);
    draw_save_categories(frame, app);
    draw_tag_picker(frame, app);
    draw_migration_sources(frame, app);
    draw_migration_search(frame, app);
    draw_clear_downloads_confirm(frame, app);
    draw_downloads_delete_confirm(frame, app);
}

fn draw_cover_overlay(frame: &mut Frame, app: &mut AppState) {
    if !app.cover_overlay {
        return;
    }
    let Some(path) = app.current_story_cover() else {
        return;
    };
    if let Some(protocol) = app.tile_covers.get_mut(&path) {
        let area = frame.area();
        let popup = Rect::new(
            area.x + 4.min(area.width / 4),
            area.y + 2.min(area.height / 4),
            area.width.saturating_sub(8.min(area.width / 2)),
            area.height.saturating_sub(4.min(area.height / 2)),
        );
        frame.render_widget(Clear, popup);
        render_centered_image(frame, popup, protocol);
    }
}

fn draw_download_menu(frame: &mut Frame, app: &AppState) {
    let Some(menu) = app.download_menu.as_ref() else {
        return;
    };
    let rows = app.download_menu_rows();
    if rows.is_empty() {
        return;
    }
    let area = frame.area();
    let width = 58.min(area.width).max(20);
    let height = (rows.len() as u16 + 2).min(area.height);
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
            " Downloads ",
            Style::default().fg(c_accent()).add_modifier(Modifier::BOLD),
        )),
        c_accent(),
    );
    let inner = block.inner(popup);
    frame.render_widget(Clear, popup);
    frame.render_widget(&block, popup);
    let items: Vec<ListItem> = rows
        .iter()
        .enumerate()
        .map(|(index, (label, _))| {
            let selected = index == menu.selected;
            ListItem::new(Line::from(vec![
                Span::styled(
                    if selected { "› " } else { "  " },
                    Style::default().fg(c_accent()),
                ),
                Span::styled(
                    label.clone(),
                    if selected {
                        Style::default().fg(c_accent()).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(c_text())
                    },
                ),
            ]))
        })
        .collect();
    let mut state = ListState::default();
    state.select(Some(menu.selected.min(rows.len().saturating_sub(1))));
    frame.render_stateful_widget(List::new(items), inner, &mut state);
}

fn draw_download_delete_confirm(frame: &mut Frame, app: &AppState) {
    if !app.download_delete_confirm {
        return;
    }
    let area = frame.area();
    let width = 54.min(area.width).max(20);
    let height = 6.min(area.height);
    let popup = Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    );
    let block = panel(
        Line::from(Span::styled(
            " Delete downloads ",
            Style::default().fg(c_peach()).add_modifier(Modifier::BOLD),
        )),
        c_peach(),
    );
    let inner = block.inner(popup);
    frame.render_widget(Clear, popup);
    frame.render_widget(&block, popup);
    frame.render_widget(
        Paragraph::new(Text::from(vec![
            Line::from("Delete the selected downloaded chapters from disk?"),
            Line::from(""),
            Line::from(Span::styled(
                "Enter / y delete · Esc cancel",
                Style::default().fg(c_overlay1()),
            )),
        ])),
        inner,
    );
}

fn draw_clear_downloads_confirm(frame: &mut Frame, app: &AppState) {
    if !app.storage_clear_downloads_confirm {
        return;
    }
    let area = frame.area();
    let width = 62.min(area.width).max(24);
    let height = 7.min(area.height);
    let popup = Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    );
    let block = panel(
        Line::from(Span::styled(
            " Clear downloads ",
            Style::default().fg(c_peach()).add_modifier(Modifier::BOLD),
        )),
        c_peach(),
    );
    let inner = block.inner(popup);
    frame.render_widget(Clear, popup);
    frame.render_widget(&block, popup);
    frame.render_widget(
        Paragraph::new(Text::from(vec![
            Line::from("Delete every downloaded chapter from disk?"),
            Line::from("Local library chapters are not affected."),
            Line::from(""),
            Line::from(Span::styled(
                "Enter / y delete · Esc cancel",
                Style::default().fg(c_overlay1()),
            )),
        ])),
        inner,
    );
}

fn draw_migration_search(frame: &mut Frame, app: &AppState) {
    let Some(search) = app.migration_search.as_ref() else {
        return;
    };
    let area = frame.area();
    let height = (search.results.len() as u16 + 2).min(area.height).max(4);
    let width = 76.min(area.width).max(24);
    if width < 20 || height < 3 {
        return;
    }
    let popup = Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    );
    let block = panel(
        Line::from(vec![
            Span::styled(
                " Manual migration target ",
                Style::default().fg(c_accent()).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "Enter select · Esc cancel ",
                Style::default().fg(c_overlay0()),
            ),
        ]),
        c_accent(),
    );
    let inner = block.inner(popup);
    frame.render_widget(Clear, popup);
    frame.render_widget(&block, popup);
    let items: Vec<ListItem> = search
        .results
        .iter()
        .enumerate()
        .map(|(index, result)| {
            let selected = index == search.selected;
            ListItem::new(Line::from(vec![
                Span::styled(
                    if selected { "› " } else { "  " },
                    Style::default().fg(c_accent()),
                ),
                Span::styled(
                    result.manga.title.clone(),
                    if selected {
                        Style::default().fg(c_accent()).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(c_text())
                    },
                ),
                Span::styled(
                    format!("  {:>3.0}%", result.score * 100.0),
                    Style::default().fg(c_teal()),
                ),
                Span::styled(
                    format!("  {}", result.source.display_name()),
                    Style::default().fg(c_overlay0()),
                ),
            ]))
        })
        .collect();
    let mut state = ListState::default();
    state.select(Some(
        search.selected.min(search.results.len().saturating_sub(1)),
    ));
    frame.render_stateful_widget(List::new(items), inner, &mut state);
}

fn draw_migration_sources(frame: &mut Frame, app: &AppState) {
    let Some(picker) = app.migration_source_picker.as_ref() else {
        return;
    };
    let sources = app.migration_source_list();
    let area = frame.area();
    let height = (sources.len() as u16 + 3).min(area.height).max(4);
    let width = 68.min(area.width).max(20);
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
        Line::from(vec![
            Span::styled(
                " Migration sources ",
                Style::default().fg(c_accent()).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "Space toggle · a all · n none · +/- order · Enter apply · Esc cancel ",
                Style::default().fg(c_overlay0()),
            ),
        ]),
        c_accent(),
    );
    let inner = block.inner(popup);
    frame.render_widget(Clear, popup);
    frame.render_widget(&block, popup);
    let items: Vec<ListItem> = sources
        .iter()
        .enumerate()
        .map(|(index, source)| {
            let selected = index == picker.selected;
            let checked = app.migration_source_selected(source);
            ListItem::new(Line::from(vec![
                Span::styled(
                    if selected { "› " } else { "  " },
                    Style::default().fg(c_accent()),
                ),
                Span::styled(
                    format!(
                        "{} {}",
                        if checked { "[x]" } else { "[ ]" },
                        source.display_name()
                    ),
                    if selected {
                        Style::default().fg(c_accent()).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(c_text())
                    },
                ),
            ]))
        })
        .collect();
    let mut state = ListState::default();
    state.select(Some(picker.selected.min(sources.len().saturating_sub(1))));
    frame.render_stateful_widget(List::new(items), inner, &mut state);
}

fn draw_tag_picker(frame: &mut Frame, app: &AppState) {
    let Some(picker) = app.tag_picker.as_ref() else {
        return;
    };
    let area = frame.area();
    let height = (picker.tags.len() as u16 + 2).min(area.height).max(4);
    let width = 56.min(area.width).max(20);
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
        Line::from(vec![
            Span::styled(
                " Tags ",
                Style::default().fg(c_accent()).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "Enter search · Esc cancel ",
                Style::default().fg(c_overlay0()),
            ),
        ]),
        c_accent(),
    );
    let inner = block.inner(popup);
    frame.render_widget(Clear, popup);
    frame.render_widget(&block, popup);
    let items: Vec<ListItem> = picker
        .tags
        .iter()
        .enumerate()
        .map(|(index, tag)| {
            let selected = index == picker.selected;
            ListItem::new(Line::from(vec![
                Span::styled(
                    if selected { "› " } else { "  " },
                    Style::default().fg(c_accent()),
                ),
                Span::styled(
                    tag.clone(),
                    if selected {
                        Style::default().fg(c_accent()).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(c_text())
                    },
                ),
            ]))
        })
        .collect();
    let mut state = ListState::default();
    state.select(Some(
        picker.selected.min(picker.tags.len().saturating_sub(1)),
    ));
    frame.render_stateful_widget(List::new(items), inner, &mut state);
}

fn draw_save_categories(frame: &mut Frame, app: &AppState) {
    let Some(dialog) = app.save_categories.as_ref() else {
        return;
    };
    let area = frame.area();
    let height = (app.library.categories.len() as u16 + 3)
        .min(area.height)
        .max(4);
    let width = 58.min(area.width).max(20);
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
        Line::from(vec![
            Span::styled(
                " Save to library ",
                Style::default().fg(c_accent()).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "Space toggle · Enter save · Esc cancel ",
                Style::default().fg(c_overlay0()),
            ),
        ]),
        c_accent(),
    );
    let inner = block.inner(popup);
    frame.render_widget(Clear, popup);
    frame.render_widget(&block, popup);
    let title = Line::from(Span::styled(
        truncate_text(&dialog.manga.title, inner.width),
        Style::default().fg(c_text()).add_modifier(Modifier::BOLD),
    ));
    let rows: Vec<ListItem> = app
        .library
        .categories
        .iter()
        .enumerate()
        .map(|(index, category)| {
            let selected = index == dialog.selected;
            let checked = dialog.chosen.contains(category);
            ListItem::new(Line::from(vec![
                Span::styled(
                    if selected { "› " } else { "  " },
                    Style::default().fg(c_accent()),
                ),
                Span::styled(
                    format!("{} {category}", if checked { "[x]" } else { "[ ]" }),
                    if selected {
                        Style::default().fg(c_accent()).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(c_text())
                    },
                ),
            ]))
        })
        .collect();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(inner);
    frame.render_widget(Paragraph::new(title), chunks[0]);
    let mut state = ListState::default();
    state.select(Some(
        dialog
            .selected
            .min(app.library.categories.len().saturating_sub(1)),
    ));
    frame.render_stateful_widget(List::new(rows), chunks[1], &mut state);
}

fn draw_category_picker(frame: &mut Frame, app: &AppState) {
    let Some(picker) = app.category_picker.as_ref() else {
        return;
    };
    let area = frame.area();
    let height = (app.library.categories.len() as u16 + 3)
        .min(area.height)
        .max(4);
    let width = 52.min(area.width).max(20);
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
        Line::from(vec![
            Span::styled(
                " Categories ",
                Style::default().fg(c_accent()).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "Space toggle · Enter apply · Esc cancel ",
                Style::default().fg(c_overlay0()),
            ),
        ]),
        c_accent(),
    );
    let inner = block.inner(popup);
    frame.render_widget(Clear, popup);
    frame.render_widget(&block, popup);
    let items: Vec<ListItem> = app
        .library
        .categories
        .iter()
        .enumerate()
        .map(|(index, category)| {
            let selected = index == picker.selected;
            let checked = picker.chosen.contains(category);
            ListItem::new(Line::from(vec![
                Span::styled(
                    if selected { "› " } else { "  " },
                    Style::default().fg(c_accent()),
                ),
                Span::styled(
                    format!("{} {category}", if checked { "[x]" } else { "[ ]" }),
                    if selected {
                        Style::default().fg(c_accent()).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(c_text())
                    },
                ),
            ]))
        })
        .collect();
    let mut state = ListState::default();
    state.select(Some(
        picker
            .selected
            .min(app.library.categories.len().saturating_sub(1)),
    ));
    frame.render_stateful_widget(List::new(items), inner, &mut state);
}

fn draw_chapter_settings(frame: &mut Frame, app: &AppState) {
    let Some(modal) = app.chapter_settings_modal.as_ref() else {
        return;
    };
    let settings = app.chapter_settings();
    let scanlators = app.chapter_scanlators();
    let rows = app.chapter_setting_rows();
    if rows.is_empty() {
        return;
    }
    let area = frame.area();
    let width = 66.min(area.width).max(24);
    let height = (rows.len() as u16 + 4).min(area.height);
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
        Line::from(vec![
            Span::styled(
                " Chapter settings ",
                Style::default().fg(c_accent()).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "←→ change · Space toggle · Esc close ",
                Style::default().fg(c_overlay0()),
            ),
        ]),
        c_accent(),
    );
    let inner = block.inner(popup);
    frame.render_widget(Clear, popup);
    frame.render_widget(&block, popup);
    let items: Vec<ListItem> = rows
        .iter()
        .enumerate()
        .map(|(index, row)| {
            let selected = index == modal.selected;
            let (label, value) = match row {
                ChapterSettingRow::Downloaded => (
                    "Downloaded".to_owned(),
                    settings.downloaded.label("downloaded"),
                ),
                ChapterSettingRow::Unread => ("Unread".to_owned(), settings.unread.label("unread")),
                ChapterSettingRow::Bookmarked => (
                    "Bookmarked".to_owned(),
                    settings.bookmarked.label("bookmarked"),
                ),
                ChapterSettingRow::Sort => ("Sort".to_owned(), settings.sort.label().to_owned()),
                ChapterSettingRow::Direction => (
                    "Direction".to_owned(),
                    if settings.descending {
                        "descending"
                    } else {
                        "ascending"
                    }
                    .to_owned(),
                ),
                ChapterSettingRow::Display => {
                    ("Display".to_owned(), settings.display.label().to_owned())
                }
                ChapterSettingRow::DownloadNewChapters => (
                    "Download new chapters".to_owned(),
                    if settings.download_new_chapters {
                        "on"
                    } else {
                        "off"
                    }
                    .to_owned(),
                ),
                ChapterSettingRow::Reset => ("Reset to defaults".to_owned(), String::new()),
                ChapterSettingRow::Scanlator(index) => {
                    let name = scanlators.get(*index).cloned().unwrap_or_default();
                    let excluded = settings.excluded_scanlators.contains(&name);
                    (
                        format!("{} {name}", if excluded { "[ ]" } else { "[x]" }),
                        if excluded { "hidden" } else { "shown" }.to_owned(),
                    )
                }
            };
            ListItem::new(Line::from(vec![
                Span::styled(
                    if selected { "› " } else { "  " },
                    Style::default().fg(c_accent()),
                ),
                Span::styled(
                    label,
                    if selected {
                        Style::default().fg(c_accent()).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(c_text())
                    },
                ),
                Span::styled(format!("  {value}"), Style::default().fg(c_overlay1())),
            ]))
        })
        .collect();
    let mut state = ListState::default();
    state.select(Some(modal.selected.min(rows.len().saturating_sub(1))));
    frame.render_stateful_widget(List::new(items), inner, &mut state);
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
        "↑↓ select · a add anyway · Enter open · M migrate · Esc cancel",
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

fn draw_history_delete(frame: &mut Frame, app: &AppState) {
    let Some(dialog) = app.history_delete.as_ref() else {
        return;
    };
    let area = frame.area();
    let width = 64.min(area.width);
    let height = 8.min(area.height);
    if width < 28 || height < 6 {
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
            " Remove history ",
            Style::default().fg(c_accent()).add_modifier(Modifier::BOLD),
        )),
        c_accent(),
    );
    let inner = block.inner(popup);
    frame.render_widget(Clear, popup);
    frame.render_widget(&block, popup);
    let title = if dialog.keys.len() == 1 {
        "Remove this history entry?".to_owned()
    } else {
        format!("Remove {} history entries?", dialog.keys.len())
    };
    let checkbox = format!(
        "{}  Also remove all history for the selected manga",
        if dialog.all_series { "[x]" } else { "[ ]" }
    );
    let lines = vec![
        Line::from(Span::styled(
            truncate_text(&title, width.saturating_sub(4)),
            Style::default().fg(c_text()).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            checkbox,
            Style::default().fg(if dialog.all_series {
                c_yellow()
            } else {
                c_text()
            }),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "Space toggle · Enter remove · Esc cancel",
            Style::default().fg(c_overlay0()),
        )),
    ];
    frame.render_widget(
        Paragraph::new(Text::from(lines)).wrap(Wrap { trim: true }),
        inner,
    );
}

fn draw_history_clear_confirm(frame: &mut Frame, app: &AppState) {
    if !app.history_clear_confirm {
        return;
    }
    let area = frame.area();
    let width = 66.min(area.width);
    let height = 7.min(area.height);
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
            " Clear all history ",
            Style::default().fg(c_accent()).add_modifier(Modifier::BOLD),
        )),
        c_accent(),
    );
    let inner = block.inner(popup);
    frame.render_widget(Clear, popup);
    frame.render_widget(&block, popup);
    let lines = vec![
        Line::from(Span::styled(
            "Remove all reading history?",
            Style::default().fg(c_text()).add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            "Every chapter will be marked unread. Bookmarks and downloads stay.",
            Style::default().fg(c_peach()),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "Enter / y confirm · Esc cancel",
            Style::default().fg(c_overlay0()),
        )),
    ];
    frame.render_widget(
        Paragraph::new(Text::from(lines)).wrap(Wrap { trim: true }),
        inner,
    );
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

fn draw_tabs(frame: &mut Frame, area: Rect, active: usize, updates: usize) {
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

        let label = if tab.index == 1 && updates > 0 {
            format!("Updates ({updates})")
        } else {
            TAB_NAMES[tab.index].to_owned()
        };
        let text = center_label(&label, tab.area.width);
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

fn command_items(app: &AppState) -> Vec<(&'static str, &'static str)> {
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
        } else if app.reader_vertical_paging() {
            vec![
                ("q", "quit"),
                ("↑↓", "page"),
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
                ("g", "site"),
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
                ("g", "site"),
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
                ("n", "continue"),
                ("f", "settings"),
                ("m/u", "read"),
                ("a/I", "select"),
                ("b", "bookmark"),
                ("c", "categories"),
                ("N", "notes"),
                ("Y", "copy tags"),
                ("E", "metadata"),
                ("S", "share cover"),
                ("C", "copy URL"),
                ("o", "open source"),
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
                ("M", "migrate"),
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
                ("n", "continue"),
                ("f", "settings"),
                ("m/u", "read"),
                ("a/I", "select"),
                ("F", "library"),
                ("M", "migrate"),
                ("c", "categories"),
                ("N", "notes"),
                ("Y", "copy tags"),
                ("S", "share cover"),
                ("C/o", "copy/open"),
                ("w/W", "cover"),
                ("t", "tags"),
                ("r", "refresh"),
                ("Esc", "back"),
            ],
            BrowseView::GlobalResults => vec![
                ("q", "quit"),
                ("↑↓", "source"),
                ("←→", "result"),
                ("A", "scope"),
                ("h", "results only"),
                ("F", "library"),
                ("M", "migrate"),
                ("↵", "open"),
                ("o", "source"),
                ("r", "retry"),
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
                ("c", "cancel"),
                ("a", "store"),
                ("t", "stores"),
                ("[ ]", "tabs"),
                ("Esc", "back"),
            ],
            BrowseView::Stores => vec![
                ("q", "quit"),
                ("↑↓", "store"),
                ("a", "add"),
                ("n", "name"),
                ("d", "remove"),
                ("C", "copy URL"),
                ("o", "open"),
                ("r", "refresh"),
                ("t", "extensions"),
                ("[ ]", "tabs"),
                ("Esc", "back"),
            ],
            BrowseView::Migrate => match app.migrate.screen {
                crate::app::MigrateScreen::Sources => vec![
                    ("q", "quit"),
                    ("↑↓", "source"),
                    ("↵", "open"),
                    ("[ ]", "tabs"),
                    ("Esc", "back"),
                ],
                crate::app::MigrateScreen::Favorites => vec![
                    ("q", "quit"),
                    ("↑↓", "manga"),
                    ("Space", "select"),
                    ("p", "chapters"),
                    ("t", "sources"),
                    ("M", "match"),
                    ("Esc", "back"),
                ],
                crate::app::MigrateScreen::Matches if app.migrate.applying => {
                    vec![("q", "quit"), ("Esc", "cancel")]
                }
                crate::app::MigrateScreen::Matches => vec![
                    ("q", "quit"),
                    ("↑↓", "match"),
                    ("s", "skip"),
                    ("↵/o", "override"),
                    ("h", "hide"),
                    ("u", "updates"),
                    ("p", "chapters"),
                    ("t", "sources"),
                    ("M", "migrate"),
                    ("C", "copy"),
                    ("Esc", "back"),
                ],
            },
        }
    } else if app.tab == 4 {
        if app.more_focus == MoreFocus::Sections {
            vec![
                ("q", "quit"),
                ("↑↓", "section"),
                ("↵", "enter"),
                ("Esc", "back"),
            ]
        } else if app.more_section == MoreSection::Downloads {
            vec![
                ("q", "quit"),
                ("↑↓", "series"),
                ("r", "refresh"),
                ("x/Del", "delete"),
                ("[ ]", "sections"),
                ("Esc", "back"),
            ]
        } else if app.more_section == MoreSection::Storage {
            vec![
                ("q", "quit"),
                ("r", "refresh"),
                ("c", "clear covers"),
                ("d", "clear downloads"),
                ("[ ]", "sections"),
                ("Esc", "back"),
            ]
        } else if app.more_section == MoreSection::Categories {
            vec![
                ("q", "quit"),
                ("↑↓", "category"),
                ("a", "add"),
                ("↵", "rename"),
                ("x/Del", "remove"),
                ("[ ]", "sections"),
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
    } else if app.tab == 1 {
        vec![
            ("q", "quit"),
            ("↑↓", "update"),
            ("↵", "read"),
            ("x/Del", "dismiss"),
            ("X", "clear"),
            ("r", "rescan"),
            ("[ ]", "tabs"),
            ("Esc", "library"),
        ]
    } else if app.tab == 2 {
        if app.history_search {
            vec![
                ("q", "quit"),
                ("type", "search"),
                ("↵", "done"),
                ("Esc", "clear"),
            ]
        } else {
            match app.history_section {
                HistorySection::History => vec![
                    ("q", "quit"),
                    ("↑↓", "history"),
                    ("↵", "resume"),
                    ("o", "details"),
                    ("F", "library"),
                    ("x/Del", "remove"),
                    ("X", "clear all"),
                    ("/", "search"),
                    ("[ ]", "tabs"),
                    ("Esc", "back"),
                ],
                HistorySection::Stats => {
                    vec![
                        ("q", "quit"),
                        ("↑↓", "scroll"),
                        ("[ ]", "tabs"),
                        ("Esc", "back"),
                    ]
                }
            }
        }
    } else if app.tab == 0 {
        match app.view {
            LibraryView::Series => vec![
                ("q", "quit"),
                ("↑↓←→", "move"),
                ("↵", "open"),
                ("/", "search"),
                ("i", "import"),
                ("r", "rescan"),
                ("U", "update saved"),
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
                ("n", "continue"),
                ("f", "settings"),
                ("m/u", "read"),
                ("a/I", "select"),
                ("b", "bookmark"),
                ("Y", "tags"),
                ("E", "metadata"),
                ("S", "cover"),
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
        items.insert(at, ("D", "downloads"));
    }
    if app.story_screen_active()
        && app.has_downloadable_chapter()
        && !items.iter().any(|(key, _)| *key == "x/Del")
    {
        let at = items.len().saturating_sub(1);
        items.insert(at, ("x/Del", "delete downloads"));
    }
    if app.tab == 3
        && app.browse_view == BrowseView::Extensions
        && (app
            .selected_extension()
            .is_some_and(|extension| extension.installed)
            || app.selected_not_loaded().is_some())
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
    items
}

fn command_item_width(key: &str, label: &str) -> usize {
    key.chars().count()
        + if label.is_empty() {
            0
        } else {
            1 + label.chars().count()
        }
}

fn command_prefix(app: &AppState) -> String {
    app.multi_select
        .as_ref()
        .map(|selection| format!("{} selected", selection.keys.len()))
        .unwrap_or_default()
}

fn command_line_count(app: &AppState, items: &[(&'static str, &'static str)], width: u16) -> usize {
    let width = usize::from(width);
    if width == 0 {
        return 1;
    }
    let mut lines = 1usize;
    let mut used = command_prefix(app).chars().count();
    for (key, label) in items {
        let item = command_item_width(key, label);
        let extra = if used == 0 { item } else { 3 + item };
        if used > 0 && used + extra > width {
            lines += 1;
            used = item;
        } else {
            used += extra;
        }
    }
    lines
}

/// Height of the commands box for the current screen and terminal width.
pub(crate) fn commands_height(app: &AppState, area: Rect) -> u16 {
    if app.fullscreen {
        return 3;
    }
    let items = command_items(app);
    let lines = command_line_count(app, &items, area.width.saturating_sub(2));
    let maximum = area.height.saturating_sub(6).max(3);
    (lines as u16 + 2).clamp(3, maximum)
}

fn command_lines(
    app: &AppState,
    items: &[(&'static str, &'static str)],
    width: u16,
) -> Vec<Line<'static>> {
    let width = usize::from(width);
    let prefix = command_prefix(app);
    let mut lines = Vec::new();
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut used = 0usize;
    if !prefix.is_empty() {
        used = prefix.chars().count();
        spans.push(Span::styled(
            prefix.clone(),
            Style::default().fg(c_accent()).add_modifier(Modifier::BOLD),
        ));
    }
    for (key, label) in items {
        let item = command_item_width(key, label);
        if used > 0 && used + 3 + item > width {
            lines.push(Line::from(std::mem::take(&mut spans)));
            used = 0;
        }
        if used > 0 {
            spans.push(Span::styled(" · ", Style::default().fg(c_overlay0())));
            used += 3;
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
        used += item;
    }
    if !spans.is_empty() || lines.is_empty() {
        lines.push(Line::from(spans));
    }
    lines
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

    let items = command_items(app);

    let lines = command_lines(app, &items, inner.width);
    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn draw_library(frame: &mut Frame, area: Rect, app: &mut AppState) {
    let layout = library_layout(area);

    match app.view {
        LibraryView::Series => {
            draw_categories(frame, layout.categories, app);
            let mut grid_area = library_grid_area(area);
            if app.library_filter_active() && grid_area.height > 1 {
                draw_library_search(
                    frame,
                    Rect::new(grid_area.x, grid_area.y, grid_area.width, 1),
                    app,
                );
                grid_area = Rect::new(
                    grid_area.x,
                    grid_area.y + 1,
                    grid_area.width,
                    grid_area.height - 1,
                );
            }
            let indices = app.visible_indices();
            let (columns, offset) = draw_series_grid(
                frame,
                grid_area,
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

fn draw_library_search(frame: &mut Frame, area: Rect, app: &AppState) {
    let style = if app.library_search {
        Style::default().fg(c_accent()).add_modifier(Modifier::BOLD)
    } else if app.library_query.trim().is_empty() {
        Style::default().fg(c_overlay0())
    } else {
        Style::default().fg(c_text())
    };
    let text = if app.library_search {
        format!(" Search library: {}▏", app.library_query)
    } else {
        format!(
            " Search library: {}  (/ edits, Esc clears)",
            app.library_query
        )
    };
    frame.render_widget(Paragraph::new(text).style(style), area);
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
    let settings = app.chapter_settings();
    let indices = app.story_chapter_indices();
    let row_state = ChapterRowState {
        settings: &settings,
        progress: &app.progress,
        bookmarks: &app.bookmarks,
        selected: app.selected_keys(SelectionScope::LocalChapters),
        download_states: &app.download_states,
    };
    let items = local_chapter_items(&series.chapters, &indices, &row_state);
    let position = app.story_chapter_position();
    let summary = story_chapter_summary(
        &settings,
        indices.len(),
        app.story_chapter_total(),
        app.story_missing_chapters(),
        app.download_queue.label(),
    );
    draw_story_chapters(
        frame,
        list_area,
        &series.title,
        items,
        position,
        &summary,
        &mut app.chapter_list_state,
    );

    let cover_path = series.cover.clone();
    if let Some(path) = &cover_path {
        app.ensure_tile_cover(path);
    }
    let mut lines = series_lines(&series);
    append_story_notes(&mut lines, app);
    let cover = cover_path
        .as_ref()
        .and_then(|path| app.tile_covers.get_mut(path));
    draw_story_info(frame, details_area, &series.title, lines, cover);
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
    let settings = app.chapter_settings();
    let indices = app.story_chapter_indices();
    let row_state = ChapterRowState {
        settings: &settings,
        progress: &app.progress,
        bookmarks: &app.bookmarks,
        selected: app.selected_keys(SelectionScope::RemoteChapters),
        download_states: &app.download_states,
    };
    let items = remote_chapter_items(
        &manga,
        &chapters,
        &app.library_remote.downloaded,
        &indices,
        &row_state,
    );
    let position = app.story_chapter_position();
    let summary = story_chapter_summary(
        &settings,
        indices.len(),
        app.story_chapter_total(),
        app.story_missing_chapters(),
        app.download_queue.label(),
    );
    draw_story_chapters(
        frame,
        list_area,
        &manga.title,
        items,
        position,
        &summary,
        &mut app.library_remote.list_state,
    );

    let cover_path =
        app.ensure_remote_cover(&manga.source_id, &manga.url, manga.thumbnail_url.as_deref());
    let mut lines = remote_story_lines(
        &manga,
        &chapters,
        source.as_deref(),
        "saved to library",
        app.story_updated_at,
        app.next_story_update_at(),
    );
    append_story_notes(&mut lines, app);
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
        let message = if app.library_query.trim().is_empty() {
            "No series found."
        } else {
            "No series match the library search."
        };
        frame.render_widget(
            Paragraph::new(message)
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

fn draw_updates(frame: &mut Frame, area: Rect, app: &mut AppState) {
    let rows: Vec<UpdateEntry> = app.update_rows().into_iter().cloned().collect();
    let title = if rows.is_empty() {
        " Updates ".to_owned()
    } else {
        format!(" Updates · {} ", rows.len())
    };
    let block = panel(
        Line::from(Span::styled(
            title,
            Style::default().fg(c_accent()).add_modifier(Modifier::BOLD),
        )),
        c_surface1(),
    );
    let inner = block.inner(area);
    frame.render_widget(&block, area);

    if rows.is_empty() {
        frame.render_widget(
            Paragraph::new(Text::from(vec![
                Line::from(Span::styled(
                    "No new chapters yet.",
                    Style::default().fg(c_subtext1()),
                )),
                Line::from(Span::styled(
                    "New local chapters appear here after a rescan.",
                    Style::default().fg(c_overlay0()),
                )),
            ]))
            .alignment(Alignment::Center),
            Rect::new(inner.x, inner.y + inner.height / 2, inner.width, 2),
        );
        app.updates_selection = 0;
        return;
    }

    app.updates_selection = app.updates_selection.min(rows.len() - 1);
    let row_height = 3u16;
    let visible = (inner.height / row_height).max(1) as usize;
    let mut scroll = 0usize;
    if app.updates_selection >= visible {
        scroll = app.updates_selection + 1 - visible;
    }

    let mut y = inner.y;
    let mut previous_day: Option<String> = None;
    for (index, entry) in rows.iter().enumerate().skip(scroll) {
        if y + 3 > inner.y + inner.height {
            break;
        }
        let day = date_key_for_unix(entry.found_at);
        if previous_day.as_deref() != Some(day.as_str()) {
            if y + 4 > inner.y + inner.height {
                break;
            }
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    format!(" {}", history_date_label(entry.found_at)),
                    Style::default()
                        .fg(c_overlay0())
                        .add_modifier(Modifier::BOLD),
                ))),
                Rect::new(inner.x, y, inner.width, 1),
            );
            y += 1;
            previous_day = Some(day);
        }

        let row = Rect::new(inner.x, y, inner.width, row_height);
        let selected = index == app.updates_selection;
        if selected {
            frame.render_widget(
                Block::default().style(Style::default().bg(c_surface0())),
                row,
            );
        }

        let cover_width = if row.width >= 40 { 4 } else { 0 };
        if cover_width > 0 {
            let cover_area = Rect::new(row.x, row.y, cover_width, 3);
            let cover = app
                .library
                .series
                .iter()
                .position(|series| series.path.to_string_lossy() == entry.series_key)
                .and_then(|index| app.ensure_local_series_cover(index));
            let cover = cover.and_then(|path| {
                app.ensure_tile_cover(&path);
                app.tile_covers.contains_key(&path).then_some(path)
            });
            if let Some(path) = cover {
                if let Some(protocol) = app.tile_covers.get_mut(&path) {
                    render_centered_image(frame, cover_area, protocol);
                }
            } else {
                let initial = entry
                    .series_title
                    .chars()
                    .next()
                    .map(|character| character.to_uppercase().collect::<String>())
                    .unwrap_or_default();
                frame.render_widget(
                    Paragraph::new(initial)
                        .alignment(Alignment::Center)
                        .style(Style::default().fg(c_accent()).add_modifier(Modifier::BOLD)),
                    Rect::new(cover_area.x, cover_area.y + 1, cover_area.width, 1),
                );
            }
        }

        let text_x = row.x + cover_width + if cover_width > 0 { 1 } else { 0 };
        let text_width = row.width.saturating_sub(text_x - row.x);
        let title_style = if selected {
            Style::default().fg(c_accent()).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(c_text()).add_modifier(Modifier::BOLD)
        };
        let number = entry
            .chapter_number
            .filter(|number| *number >= 0.0)
            .or_else(|| parse_chapter_number(&entry.chapter_title));
        let detail = if let Some(number) = number {
            format!(
                "Ch. {} · {}",
                chapter_number(Some(number)),
                entry.chapter_title
            )
        } else {
            entry.chapter_title.clone()
        };
        let lines = vec![
            Line::from(Span::styled(
                truncate_text(&entry.series_title, text_width.saturating_sub(2)),
                title_style,
            )),
            Line::from(Span::styled(detail, Style::default().fg(c_subtext1()))),
            Line::from(Span::styled(
                format!("Local · {}", history_time(entry.found_at)),
                Style::default().fg(c_overlay0()),
            )),
        ];
        frame.render_widget(
            Paragraph::new(Text::from(lines)),
            Rect::new(text_x, row.y, text_width, 3),
        );
        y += row_height;
    }
}

fn draw_history(frame: &mut Frame, area: Rect, app: &mut AppState) {
    let block = panel(history_tab_title(app), c_accent());
    let inner = block.inner(area);
    frame.render_widget(&block, area);
    let body = if let Some(status) = &app.status {
        if inner.height < 2 {
            inner
        } else {
            let status_area = Rect::new(inner.x, inner.y + inner.height - 1, inner.width, 1);
            frame.render_widget(
                Paragraph::new(format!(" {status}")).style(Style::default().fg(c_green())),
                status_area,
            );
            Rect::new(inner.x, inner.y, inner.width, inner.height - 1)
        }
    } else {
        inner
    };
    match app.history_section {
        HistorySection::History => draw_history_recent(frame, body, app),
        HistorySection::Stats => draw_history_stats(frame, body, app),
    }
}

fn history_tab_title(app: &AppState) -> Line<'static> {
    let tabs = [
        ("History", HistorySection::History),
        ("Stats", HistorySection::Stats),
    ];
    let mut spans = vec![Span::raw(" ")];
    for (index, (label, section)) in tabs.iter().enumerate() {
        if index > 0 {
            spans.push(Span::styled(" │ ", Style::default().fg(c_overlay0())));
        }
        let selected = app.history_section == *section;
        spans.push(Span::styled(
            *label,
            Style::default()
                .fg(if selected { c_accent() } else { c_overlay1() })
                .add_modifier(if selected {
                    Modifier::BOLD
                } else {
                    Modifier::empty()
                }),
        ));
        if *section == HistorySection::History {
            spans.push(Span::styled(
                format!(" {}", app.history_count()),
                Style::default().fg(c_overlay0()),
            ));
        }
    }
    spans.push(Span::raw(" "));
    Line::from(spans)
}

fn draw_history_recent(frame: &mut Frame, area: Rect, app: &mut AppState) {
    if area.height == 0 {
        return;
    }
    let search_area = Rect::new(area.x, area.y, area.width, 1);
    let search_style = if app.history_search {
        Style::default().fg(c_accent()).add_modifier(Modifier::BOLD)
    } else if app.history_query.trim().is_empty() {
        Style::default().fg(c_overlay0())
    } else {
        Style::default().fg(c_text())
    };
    let search_text = if app.history_search {
        format!(" Search: {}▏", app.history_query)
    } else if app.history_query.trim().is_empty() {
        " / search history".to_owned()
    } else {
        format!(" Search: {}  (/ edits, Esc clears)", app.history_query)
    };
    frame.render_widget(Paragraph::new(search_text).style(search_style), search_area);
    let list_area = Rect::new(
        area.x,
        area.y + 1,
        area.width,
        area.height.saturating_sub(1),
    );

    let entries = app.history_entries();
    if entries.is_empty() {
        let message = if app.history_query.trim().is_empty() {
            "No reading history yet."
        } else {
            "No history matches the search."
        };
        frame.render_widget(
            Paragraph::new(message)
                .alignment(Alignment::Center)
                .style(Style::default().fg(c_overlay1())),
            Rect::new(list_area.x, list_area.y, list_area.width, 1),
        );
        app.history_list_state.select(None);
        return;
    }

    app.history_selection = app.history_selection.min(entries.len() - 1);
    let row_height = 4u16;
    let visible = (list_area.height / row_height).max(1) as usize;
    if app.history_selection < app.history_scroll {
        app.history_scroll = app.history_selection;
    } else if app.history_selection >= app.history_scroll + visible {
        app.history_scroll = app.history_selection + 1 - visible;
    }

    let mut y = list_area.y;
    let mut previous_day: Option<String> = None;
    for (index, (key, entry)) in entries.iter().enumerate().skip(app.history_scroll) {
        if y + 3 > list_area.y + list_area.height {
            break;
        }
        let day = date_key_for_unix(entry.last_read);
        if previous_day.as_deref() != Some(day.as_str()) {
            if y + 4 > list_area.y + list_area.height {
                break;
            }
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    format!(" {}", history_date_label(entry.last_read)),
                    Style::default()
                        .fg(c_overlay0())
                        .add_modifier(Modifier::BOLD),
                ))),
                Rect::new(list_area.x, y, list_area.width, 1),
            );
            y += 1;
            previous_day = Some(day);
        }
        let row = Rect::new(list_area.x, y, list_area.width, 3);
        let selected = index == app.history_selection;
        if selected {
            frame.render_widget(
                Block::default().style(Style::default().bg(c_surface0())),
                row,
            );
        }
        let selected_marker =
            app.is_multi_selected(SelectionScope::History, &format!("history:{key}"));
        let favorite = entry
            .remote
            .as_ref()
            .is_some_and(|remote| app.is_favorite(&remote.manga.source_id, &remote.manga.url));
        let marker = match (selected_marker, favorite) {
            (true, true) => "✓ ★ ",
            (true, false) => "✓ ",
            (false, true) => "★ ",
            (false, false) => "",
        };
        let cover_width = if row.width >= 40 { 4 } else { 0 };
        if cover_width > 0 {
            let cover_area = Rect::new(row.x, row.y, cover_width, 3);
            let cover = history_cover_path(app, entry);
            let mut rendered = false;
            if let Some(path) = &cover {
                if let Some(protocol) = app.tile_covers.get_mut(path) {
                    render_centered_image(frame, cover_area, protocol);
                    rendered = true;
                }
            }
            if !rendered {
                let initial = entry
                    .series_title
                    .chars()
                    .next()
                    .map(|character| character.to_uppercase().collect::<String>())
                    .unwrap_or_default();
                frame.render_widget(
                    Paragraph::new(initial)
                        .alignment(Alignment::Center)
                        .style(Style::default().fg(c_accent()).add_modifier(Modifier::BOLD)),
                    Rect::new(cover_area.x, cover_area.y + 1, cover_area.width, 1),
                );
            }
        }
        let text_x = row.x + cover_width + if cover_width > 0 { 1 } else { 0 };
        let text_width = row.width.saturating_sub(text_x - row.x);
        let title_style = if selected {
            Style::default().fg(c_accent()).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(c_text()).add_modifier(Modifier::BOLD)
        };
        let chapter = if entry.chapter_title.is_empty() {
            "Unknown chapter".to_owned()
        } else {
            entry.chapter_title.clone()
        };
        let number = entry
            .remote
            .as_ref()
            .map(|remote| remote.chapter.chapter_number)
            .filter(|number| *number >= 0.0)
            .or_else(|| parse_chapter_number(&chapter));
        let mut detail = if let Some(number) = number {
            format!(
                "Ch. {} · {chapter} · {}",
                chapter_number(Some(number)),
                history_time(entry.last_read)
            )
        } else {
            format!("{chapter} · {}", history_time(entry.last_read))
        };
        if entry.pages > 0 {
            detail.push_str(&format!(" · {}/{}", entry.page + 1, entry.pages));
        }
        let source = if entry.read_duration_secs > 0 {
            format!(
                "{} · {} read",
                entry.source,
                format_duration(entry.read_duration_secs)
            )
        } else {
            entry.source.clone()
        };
        let lines = vec![
            Line::from(vec![
                Span::styled(marker, Style::default().fg(c_accent())),
                Span::styled(
                    truncate_text(&entry.series_title, text_width.saturating_sub(2)),
                    title_style,
                ),
            ]),
            Line::from(Span::styled(detail, Style::default().fg(c_subtext1()))),
            Line::from(Span::styled(source, Style::default().fg(c_overlay0()))),
        ];
        frame.render_widget(
            Paragraph::new(Text::from(lines)),
            Rect::new(text_x, row.y, text_width, 3),
        );
        y += row_height;
    }

    app.history_list_state.select(Some(app.history_selection));
}

fn history_cover_path(app: &mut AppState, entry: &HistoryEntry) -> Option<PathBuf> {
    if let Some(remote) = &entry.remote {
        let path = remote_cover_path(&remote.manga.source_id, &remote.manga.url)?;
        if path.exists() {
            app.ensure_tile_cover(&path);
            if app.tile_covers.contains_key(&path) {
                return Some(path);
            }
        } else {
            app.request_series_thumbnail(
                &remote.manga.source_id,
                &remote.manga.url,
                remote.manga.memo.as_deref(),
            );
        }
        return None;
    }
    if let Some(cover) = entry.cover.clone().filter(|path| path.exists()) {
        app.ensure_tile_cover(&cover);
        if app.tile_covers.contains_key(&cover) {
            return Some(cover);
        }
    }
    let local = entry.local.as_ref()?;
    let index = app
        .library
        .series
        .iter()
        .position(|series| series.path == local.series_path)?;
    let cover = app.ensure_local_series_cover(index)?;
    app.ensure_tile_cover(&cover);
    app.tile_covers.contains_key(&cover).then_some(cover)
}

fn history_date_label(unix_seconds: u64) -> String {
    let Some(date) = DateTime::from_timestamp(unix_seconds as i64, 0)
        .map(|utc| utc.with_timezone(&Local).date_naive())
    else {
        return "Unknown date".to_owned();
    };
    let today = Local::now().date_naive();
    if date == today {
        "Today".to_owned()
    } else if date == today - chrono::Duration::days(1) {
        "Yesterday".to_owned()
    } else {
        date.format("%a, %b %-d").to_string()
    }
}

fn history_time(unix_seconds: u64) -> String {
    DateTime::from_timestamp(unix_seconds as i64, 0)
        .map(|utc| utc.with_timezone(&Local).format("%H:%M").to_string())
        .unwrap_or_else(|| "--:--".to_owned())
}

fn draw_history_stats(frame: &mut Frame, area: Rect, app: &mut AppState) {
    let stats = app.reading_stats.clone();
    let mut lines = vec![
        Line::from(vec![
            Span::styled(" All time   ", Style::default().fg(c_overlay1())),
            Span::styled(
                format_duration(stats.total_seconds),
                Style::default().fg(c_accent()).add_modifier(Modifier::BOLD),
            ),
            Span::styled("    Today ", Style::default().fg(c_overlay1())),
            Span::styled(
                format_duration(stats.day_seconds(&today_key())),
                Style::default().fg(c_green()),
            ),
        ]),
        Line::from(vec![
            Span::styled(" Last 7d    ", Style::default().fg(c_overlay1())),
            Span::styled(
                format_duration(stats.range_seconds(7)),
                Style::default().fg(c_teal()),
            ),
            Span::styled("    Last 30d ", Style::default().fg(c_overlay1())),
            Span::styled(
                format_duration(stats.range_seconds(30)),
                Style::default().fg(c_blue()),
            ),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            " Last 14 days",
            Style::default()
                .fg(c_overlay0())
                .add_modifier(Modifier::BOLD),
        )),
    ];

    let mut max_day = 1u64;
    let today = Local::now().date_naive();
    let days: Vec<_> = (0..14)
        .rev()
        .map(|offset| {
            let date = today - chrono::Duration::days(offset);
            let key = date.format("%Y-%m-%d").to_string();
            let seconds = stats.day_seconds(&key);
            max_day = max_day.max(seconds);
            (date, seconds)
        })
        .collect();
    for (date, seconds) in days {
        let width = if max_day == 0 {
            0
        } else {
            ((seconds as f64 / max_day as f64) * 22.0).round() as usize
        };
        lines.push(Line::from(vec![
            Span::styled(
                format!(" {} ", date.format("%b %-d")),
                Style::default().fg(c_overlay0()),
            ),
            Span::styled("█".repeat(width), Style::default().fg(c_accent())),
            Span::styled(
                format!(" {}", format_duration(seconds)),
                Style::default().fg(c_subtext1()),
            ),
        ]));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        " Top series",
        Style::default()
            .fg(c_overlay0())
            .add_modifier(Modifier::BOLD),
    )));
    let top = stats.top_series(6);
    if top.is_empty() {
        lines.push(Line::from(Span::styled(
            " No reading time recorded yet.",
            Style::default().fg(c_overlay1()),
        )));
    } else {
        for (index, (_, series)) in top.iter().enumerate() {
            lines.push(Line::from(vec![
                Span::styled(
                    format!(" {:>2}. ", index + 1),
                    Style::default().fg(c_overlay0()),
                ),
                Span::styled(
                    truncate_text(&series.title, area.width.saturating_sub(24)),
                    Style::default().fg(c_text()),
                ),
                Span::styled(
                    format!("  {}", format_duration(series.seconds)),
                    Style::default().fg(c_teal()),
                ),
            ]));
        }
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        format!(
            " Series {} · chapters {} · streak {} days · active days {}",
            stats.series_seconds.len(),
            app.history.len(),
            stats.reading_streak(),
            stats.active_days()
        ),
        Style::default().fg(c_overlay1()),
    )));
    let max_scroll = lines.len().saturating_sub(area.height as usize);
    app.history_stats_scroll = app.history_stats_scroll.min(max_scroll);
    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .scroll((app.history_stats_scroll.min(u16::MAX as usize) as u16, 0)),
        area,
    );
}

fn format_bytes(bytes: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = KIB * 1024;
    const GIB: u64 = MIB * 1024;
    if bytes >= GIB {
        format!("{:.1} GiB", bytes as f64 / GIB as f64)
    } else if bytes >= MIB {
        format!("{:.1} MiB", bytes as f64 / MIB as f64)
    } else if bytes >= KIB {
        format!("{:.1} KiB", bytes as f64 / KIB as f64)
    } else {
        format!("{bytes} B")
    }
}

fn format_duration(seconds: u64) -> String {
    if seconds == 0 {
        "0m".to_owned()
    } else if seconds < 60 {
        format!("{seconds}s")
    } else if seconds < 3600 {
        format!("{}m", seconds / 60)
    } else {
        let hours = seconds / 3600;
        let minutes = (seconds % 3600) / 60;
        if minutes == 0 {
            format!("{hours}h")
        } else {
            format!("{hours}h {minutes:02}m")
        }
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
            "No results. Press r to retry.".to_owned()
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
            if let Some(error) = &group.error {
                Span::styled(format!("  error · {error}"), Style::default().fg(c_peach()))
            } else if group.loading {
                Span::styled("  searching…", Style::default().fg(c_yellow()))
            } else {
                Span::styled(
                    format!("  {} results", group.mangas.len()),
                    Style::default().fg(c_overlay0()),
                )
            },
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
        BrowseView::Migrate => {
            let (label, detail) = match app.migrate.screen {
                crate::app::MigrateScreen::Sources => {
                    ("Migrate · sources", "saved manga by source".to_owned())
                }
                crate::app::MigrateScreen::Favorites => (
                    "Migrate · favorites",
                    app.migrate
                        .source_id
                        .as_deref()
                        .and_then(|id| app.remote.sources.iter().find(|source| source.id == id))
                        .map(RemoteSource::display_name)
                        .unwrap_or_else(|| "source".to_owned()),
                ),
                crate::app::MigrateScreen::Matches => {
                    ("Migrate · matches", "M migrate · C copy".to_owned())
                }
            };
            (
                label.to_owned(),
                detail,
                app.migrate_item_count().to_string(),
            )
        }
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
    let store = if extension.repo_url.is_empty() {
        if extension.obsolete {
            "installed only · orphaned".to_owned()
        } else {
            "installed only · private".to_owned()
        }
    } else if extension.obsolete {
        format!("{} · orphaned", app.store_label(&extension.repo_url))
    } else {
        app.store_label(&extension.repo_url)
    };
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
            &store,
            if extension.obsolete {
                c_peach()
            } else {
                c_overlay1()
            },
        ),
        field(
            "Warning",
            if extension.nsfw { "NSFW" } else { "safe" },
            if extension.nsfw { c_peach() } else { c_green() },
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
        Rect::new(body.x, body.y + 1, body.width, 7),
    );

    let list_area = Rect::new(
        body.x,
        body.y + 8,
        body.width,
        body.height.saturating_sub(8),
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
                crate::app::ExtensionDetailRow::Incognito => {
                    let enabled = app.extension_details.as_ref().is_some_and(|state| {
                        app.config
                            .incognito_extensions
                            .contains(&state.extension.pkg)
                    });
                    ListItem::new(Line::from(vec![
                        Span::styled(
                            if enabled { "[x] " } else { "[ ] " },
                            Style::default().fg(if enabled { c_green() } else { c_overlay0() }),
                        ),
                        Span::styled("Incognito mode", Style::default().fg(c_text())),
                    ]))
                }
                crate::app::ExtensionDetailRow::ClearCookies => ListItem::new(Line::from(
                    Span::styled(" Clear cookies", Style::default().fg(c_blue())),
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

fn remote_results_empty_message(app: &AppState) -> String {
    if !app.remote.results.is_empty() && app.config.browse_hide_saved {
        return "All results are saved. Press h to show them.".to_owned();
    }
    if app.remote.loading {
        return "Loading results…".to_owned();
    }
    if !app.remote.query.trim().is_empty() {
        return format!(
            "No results for \"{}\". Press s to search or r to retry.",
            app.remote.query
        );
    }
    "No results. Press r to retry or g for the source home page.".to_owned()
}

fn draw_remote_grid(frame: &mut Frame, area: Rect, app: &mut AppState) {
    let results = app.visible_remote_results();
    if app.config.source_display_mode == SourceDisplayMode::List {
        draw_remote_result_list(frame, area, app, &results);
        return;
    }
    let columns = grid_columns(area, app.config.thumbnail_width);
    if results.is_empty() {
        let message = remote_results_empty_message(app);
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
        let saved = app.is_favorite(&source_id, &result.url);
        if saved {
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
        let mut title_style = if is_selected {
            Style::default().fg(c_accent()).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(c_text())
        };
        if saved && !is_selected {
            title_style = title_style.add_modifier(Modifier::DIM);
        }
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
                    .style(Style::default().fg(c_overlay0()).add_modifier(if saved {
                        Modifier::DIM
                    } else {
                        Modifier::empty()
                    })),
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
        let message = remote_results_empty_message(app);
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
    let settings = app.chapter_settings();
    let indices = app.story_chapter_indices();
    let row_state = ChapterRowState {
        settings: &settings,
        progress: &app.progress,
        bookmarks: &app.bookmarks,
        selected: app.selected_keys(SelectionScope::RemoteChapters),
        download_states: &app.download_states,
    };
    let items = remote_chapter_items(
        &manga,
        &chapters,
        &app.remote.downloaded,
        &indices,
        &row_state,
    );
    let position = app.story_chapter_position();
    let summary = story_chapter_summary(
        &settings,
        indices.len(),
        app.story_chapter_total(),
        app.story_missing_chapters(),
        app.download_queue.label(),
    );
    draw_story_chapters(
        frame,
        layout.list,
        &manga.title,
        items,
        position,
        &summary,
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
    let mut lines = remote_story_lines(
        &manga,
        &chapters,
        source.as_deref(),
        note,
        app.story_updated_at,
        app.next_story_update_at(),
    );
    append_story_notes(&mut lines, app);
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
    updated_at: Option<u64>,
    next_update: Option<u64>,
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
    } else {
        lines.push(field("Source", "unavailable", c_peach()));
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
    if let Some(updated_at) = updated_at {
        lines.push(field("Updated", &relative_age(updated_at), c_overlay1()));
    }
    if let Some(next_update) = next_update {
        lines.push(field("Next check", &relative_until(next_update), c_teal()));
    }
    if source.is_none() {
        lines.push(Line::from(Span::styled(
            "The source extension is not loaded (host stopped or extension missing).",
            Style::default().fg(c_peach()),
        )));
    }
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

fn relative_until(unix_seconds: u64) -> String {
    let seconds = unix_seconds as i64 - Local::now().timestamp();
    if seconds <= 0 {
        return "due now".to_owned();
    }
    let seconds = seconds as u64;
    if seconds < 60 {
        "in <1m".to_owned()
    } else if seconds < 3_600 {
        format!("in {}m", seconds / 60)
    } else if seconds < 86_400 {
        format!("in {}h", seconds / 3_600)
    } else {
        format!("in {}d", seconds / 86_400)
    }
}

fn relative_age(unix_seconds: u64) -> String {
    let seconds = Local::now()
        .timestamp()
        .saturating_sub(unix_seconds as i64)
        .max(0) as u64;
    match seconds {
        0..=59 => "just now".to_owned(),
        60..=3_599 => format!("{}m ago", seconds / 60),
        3_600..=86_399 => format!("{}h ago", seconds / 3_600),
        _ => format!("{}d ago", seconds / 86_400),
    }
}

fn append_story_notes(lines: &mut Vec<Line<'static>>, app: &AppState) {
    let Some(key) = app.current_story_key() else {
        return;
    };
    let Some(note) = app
        .config
        .story_notes
        .get(&key)
        .filter(|note| !note.is_empty())
    else {
        return;
    };
    lines.push(field("Notes", note, c_yellow()));
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
            ExtensionRow::NotLoaded(index) => {
                let extension = &app.remote.not_loaded[*index];
                ListItem::new(Line::from(vec![
                    Span::styled("! ", Style::default().fg(c_peach())),
                    Span::styled(
                        extension.name.clone(),
                        Style::default().fg(c_text()).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!("  {} · {}", extension.reason, extension.file),
                        Style::default().fg(c_overlay0()),
                    ),
                ]))
            }
            ExtensionRow::Extension(index) => {
                let extension = &app.remote.extensions[*index];
                let job = app.remote.extension_jobs.get(&extension.pkg);
                let private = extension.installed && extension.repo_url.is_empty();
                let mut status = if let Some(job) = job {
                    job.label()
                } else if extension.update_available() {
                    "update".to_owned()
                } else if extension.installed {
                    "installed".to_owned()
                } else {
                    "available".to_owned()
                };
                if extension.obsolete {
                    status.push_str(" · orphaned");
                } else if private {
                    status.push_str(" · private");
                } else if !extension.repo_url.is_empty() {
                    status.push_str(&format!(" · {}", app.store_label(&extension.repo_url)));
                }
                let status_style = if job.is_some_and(|job| job.state == "error") {
                    Style::default().fg(c_peach())
                } else if job.is_some() {
                    Style::default().fg(c_yellow())
                } else if extension.update_available() {
                    Style::default().fg(c_peach())
                } else if extension.installed {
                    Style::default().fg(c_green())
                } else {
                    Style::default().fg(c_overlay1())
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
                spans.push(Span::styled(format!("  {status}"), status_style));
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
            let count = app
                .remote
                .extensions
                .iter()
                .filter(|extension| &extension.repo_url == store)
                .count();
            let installed = app
                .remote
                .extensions
                .iter()
                .filter(|extension| &extension.repo_url == store && extension.installed)
                .count();
            let label = app.store_label(store);
            ListItem::new(vec![
                Line::from(vec![
                    Span::styled(marker, Style::default().fg(c_accent())),
                    Span::styled(
                        label,
                        Style::default().fg(c_text()).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!("  {count} extensions · {installed} installed"),
                        Style::default().fg(c_overlay0()),
                    ),
                ]),
                Line::from(Span::styled(
                    format!("  {store}"),
                    Style::default().fg(c_overlay0()),
                )),
            ])
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

fn draw_migrate(frame: &mut Frame, area: Rect, app: &mut AppState) {
    let block = panel(browse_tab_title(app), c_accent());
    let inner = block.inner(area);
    frame.render_widget(&block, area);
    let body = browse_body_with_status(frame, inner, app);
    if body.height < 2 {
        return;
    }
    draw_browse_path(frame, Rect::new(body.x, body.y, body.width, 1), app);
    let list_area = Rect::new(
        body.x,
        body.y + 1,
        body.width,
        body.height.saturating_sub(1),
    );

    let rows = match app.migrate.screen {
        crate::app::MigrateScreen::Sources => {
            let sources = app.migrate_sources();
            if sources.is_empty() {
                frame.render_widget(
                    Paragraph::new("No saved source manga to migrate.")
                        .alignment(Alignment::Center)
                        .style(Style::default().fg(c_overlay1())),
                    Rect::new(list_area.x, list_area.y, list_area.width, 1),
                );
                app.migrate.list_state.select(None);
                return;
            }
            sources
                .into_iter()
                .map(|(source, count)| {
                    ListItem::new(Line::from(vec![
                        Span::styled(source.display_name(), Style::default().fg(c_text())),
                        Span::styled(
                            format!("  {count} saved"),
                            Style::default().fg(c_overlay0()),
                        ),
                    ]))
                })
                .collect::<Vec<_>>()
        }
        crate::app::MigrateScreen::Favorites => {
            if app.migrate.favorites.is_empty() {
                frame.render_widget(
                    Paragraph::new("No saved manga in this source.")
                        .alignment(Alignment::Center)
                        .style(Style::default().fg(c_overlay1())),
                    Rect::new(list_area.x, list_area.y, list_area.width, 1),
                );
                app.migrate.list_state.select(None);
                return;
            }
            app.migrate
                .favorites
                .iter()
                .map(|favorite| {
                    let marker = if app.is_multi_selected(
                        SelectionScope::Migrate,
                        &format!("migrate:{}\n{}", favorite.source_id, favorite.url),
                    ) {
                        "✓ "
                    } else {
                        "  "
                    };
                    let categories = if favorite.categories.is_empty() {
                        "Default".to_owned()
                    } else {
                        favorite.categories.join(", ")
                    };
                    ListItem::new(Line::from(vec![
                        Span::styled(marker, Style::default().fg(c_accent())),
                        Span::styled(
                            favorite.title.clone(),
                            Style::default().fg(c_text()).add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(format!("  {categories}"), Style::default().fg(c_overlay0())),
                    ]))
                })
                .collect::<Vec<_>>()
        }
        crate::app::MigrateScreen::Matches => {
            let visible = app.migration_visible_indices();
            if visible.is_empty() {
                let message = if app.migrate.matches.is_empty() {
                    "No migration matches yet."
                } else {
                    "All matches are hidden. Press h to show them."
                };
                frame.render_widget(
                    Paragraph::new(message)
                        .alignment(Alignment::Center)
                        .style(Style::default().fg(c_overlay1())),
                    Rect::new(list_area.x, list_area.y, list_area.width, 1),
                );
                app.migrate.list_state.select(None);
                return;
            }
            visible
                .iter()
                .map(|index| {
                    let matched = &app.migrate.matches[*index];
                    let target = if let Some(target) = &matched.target {
                        let source = matched
                            .target_source
                            .as_ref()
                            .map(RemoteSource::display_name)
                            .unwrap_or_default();
                        format!(
                            "{} · {source} · {:.0}%",
                            target.title,
                            matched.score * 100.0
                        )
                    } else {
                        matched
                            .error
                            .clone()
                            .unwrap_or_else(|| "No match".to_owned())
                    };
                    ListItem::new(Line::from(vec![
                        Span::styled("→ ", Style::default().fg(c_accent())),
                        Span::styled(matched.old.title.clone(), Style::default().fg(c_text())),
                        Span::styled("  ", Style::default()),
                        Span::styled(
                            target,
                            Style::default().fg(if matched.target.is_some() {
                                c_green()
                            } else {
                                c_peach()
                            }),
                        ),
                    ]))
                })
                .collect::<Vec<_>>()
        }
    };

    app.migrate
        .list_state
        .select((!rows.is_empty()).then_some(app.browse_selection));
    let list = List::new(rows)
        .highlight_style(
            Style::default()
                .fg(c_accent())
                .bg(c_surface0())
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("› ");
    frame.render_stateful_widget(list, list_area, &mut app.migrate.list_state);
}

fn draw_browse_chapters(frame: &mut Frame, area: Rect, app: &mut AppState) {
    let layout = library_layout(area);
    let Some(series) = app.selected_browse_local_series().cloned() else {
        draw_browse_empty(frame, layout.list, "Series not found. Rescan the library.");
        return;
    };
    let settings = app.chapter_settings();
    let indices = app.story_chapter_indices();
    let row_state = ChapterRowState {
        settings: &settings,
        progress: &app.progress,
        bookmarks: &app.bookmarks,
        selected: app.selected_keys(SelectionScope::LocalChapters),
        download_states: &app.download_states,
    };
    let items = local_chapter_items(&series.chapters, &indices, &row_state);
    let position = app.story_chapter_position();
    let summary = story_chapter_summary(
        &settings,
        indices.len(),
        app.story_chapter_total(),
        app.story_missing_chapters(),
        app.download_queue.label(),
    );
    draw_story_chapters(
        frame,
        layout.list,
        &series.title,
        items,
        position,
        &summary,
        &mut app.browse_local_list_state,
    );
    let cover_path = series.cover.clone();
    if let Some(path) = &cover_path {
        app.ensure_tile_cover(path);
    }
    let mut lines = series_lines(&series);
    append_story_notes(&mut lines, app);
    let cover = cover_path
        .as_ref()
        .and_then(|path| app.tile_covers.get_mut(path));
    draw_story_info(frame, layout.details, &series.title, lines, cover);
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
                let resize = match app.config.reader_fit {
                    tsuiyomi_core::ReaderFit::Fit => Resize::Fit(None),
                    tsuiyomi_core::ReaderFit::Crop => Resize::Crop(None),
                    tsuiyomi_core::ReaderFit::Scale => Resize::Scale(None),
                };
                frame.render_stateful_widget(
                    StatefulImage::new().resize(resize),
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
    selected: usize,
    summary: &str,
    list_state: &mut ListState,
) {
    let count = items.len();
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
                Span::styled(format!("· {summary} "), Style::default().fg(c_overlay1())),
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

fn story_chapter_summary(
    settings: &ChapterSettings,
    visible: usize,
    total: usize,
    missing: usize,
    queue: Option<String>,
) -> String {
    let mut parts = Vec::new();
    if visible != total {
        parts.push(format!("{visible} of {total} shown"));
    }
    if missing > 0 {
        parts.push(format!("{missing} missing"));
    }
    parts.push(settings.summary());
    if let Some(queue) = queue {
        parts.push(queue);
    }
    parts.join(" · ")
}

fn chapter_marker_spans(
    bookmarked: bool,
    downloaded: Option<bool>,
    state: Option<DownloadState>,
) -> Vec<Span<'static>> {
    let bookmark = if bookmarked {
        Span::styled("★ ", Style::default().fg(c_yellow()))
    } else {
        Span::styled("☆ ", Style::default().fg(c_overlay0()))
    };
    let download = match state {
        Some(DownloadState::Queued) => Span::styled("… ", Style::default().fg(c_blue())),
        Some(DownloadState::Downloading) => Span::styled("⟳ ", Style::default().fg(c_yellow())),
        Some(DownloadState::Error) => Span::styled("! ", Style::default().fg(c_peach())),
        Some(DownloadState::Cancelled) => Span::styled("× ", Style::default().fg(c_overlay0())),
        Some(DownloadState::Done) | None => match downloaded {
            Some(true) => Span::styled("⬇ ", Style::default().fg(c_green())),
            Some(false) => Span::styled("↓ ", Style::default().fg(c_overlay0())),
            None => Span::styled("· ", Style::default().fg(c_overlay0())),
        },
    };
    vec![bookmark, download]
}

fn read_marker(read: ReadState) -> Span<'static> {
    match read {
        ReadState::Unread => Span::styled("● ", Style::default().fg(c_accent())),
        ReadState::InProgress => Span::styled("◐ ", Style::default().fg(c_teal())),
        ReadState::Read => Span::styled("○ ", Style::default().fg(c_overlay0())),
    }
}

fn chapter_title_style(read: ReadState) -> Style {
    match read {
        ReadState::Read => Style::default().fg(c_overlay1()),
        ReadState::InProgress | ReadState::Unread => Style::default().fg(c_text()),
    }
}

fn chapter_label(
    settings: &ChapterSettings,
    title: &str,
    number: Option<f32>,
) -> (String, Option<String>) {
    match settings.display {
        ChapterDisplay::Title => (title.to_owned(), Some(chapter_number(number))),
        ChapterDisplay::Number => match number {
            Some(number) => (format!("Chapter {}", chapter_number(Some(number))), None),
            None => (title.to_owned(), None),
        },
    }
}

struct ChapterRowState<'a> {
    settings: &'a ChapterSettings,
    progress: &'a ProgressStore,
    bookmarks: &'a BookmarkStore,
    selected: Option<&'a HashSet<String>>,
    download_states: &'a BTreeMap<String, DownloadState>,
}

fn local_chapter_items(
    chapters: &[Chapter],
    indices: &[usize],
    state: &ChapterRowState<'_>,
) -> Vec<ListItem<'static>> {
    let ChapterRowState {
        settings,
        progress,
        bookmarks,
        selected,
        download_states: _,
    } = *state;
    indices
        .iter()
        .filter_map(|index| chapters.get(*index))
        .map(|chapter| {
            let entry = progress.entry(&chapter.path);
            let read = read_state(entry.map(|entry| (entry.page, entry.pages)));
            let mut spans =
                chapter_marker_spans(bookmarks.is_bookmarked(&chapter.path), None, None);
            spans.push(read_marker(read));
            if selected
                .is_some_and(|keys| keys.contains(&format!("chapter:{}", chapter.path.display())))
            {
                spans.push(Span::styled("✓ ", Style::default().fg(c_accent())));
            }
            let (label, suffix) = chapter_label(settings, &chapter.title, chapter.number);
            spans.push(Span::styled(label, chapter_title_style(read)));
            if let Some(suffix) = suffix {
                spans.push(Span::styled(
                    format!("  {suffix}"),
                    Style::default().fg(c_overlay0()),
                ));
            }
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
            if let Some(date) = chapter.date {
                spans.push(Span::styled(
                    format!("  {:04}-{:02}-{:02}", date.year, date.month, date.day),
                    Style::default().fg(c_overlay0()),
                ));
            }
            if let Some(entry) = entry.filter(|_| read == ReadState::InProgress) {
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
    manga: &RemoteManga,
    chapters: &[RemoteChapter],
    downloaded: &[bool],
    indices: &[usize],
    state: &ChapterRowState<'_>,
) -> Vec<ListItem<'static>> {
    let ChapterRowState {
        settings,
        progress,
        bookmarks,
        selected,
        download_states,
    } = *state;
    let source_id = &manga.source_id;
    let manga_url = &manga.url;
    indices
        .iter()
        .filter_map(|index| chapters.get(*index).map(|chapter| (*index, chapter)))
        .map(|(index, chapter)| {
            let key = remote_progress_key(source_id, manga_url, &chapter.url);
            let entry = progress.entry(&key);
            let read = read_state(entry.map(|entry| (entry.page, entry.pages)));
            let mut spans = chapter_marker_spans(
                bookmarks.is_bookmarked(&key),
                Some(downloaded.get(index).copied().unwrap_or(false)),
                download_states
                    .get(&key.to_string_lossy().into_owned())
                    .copied(),
            );
            spans.push(read_marker(read));
            let selection_key = format!("rchap:{source_id}\n{manga_url}\n{}", chapter.url);
            if selected.is_some_and(|keys| keys.contains(&selection_key)) {
                spans.push(Span::styled("✓ ", Style::default().fg(c_accent())));
            }
            let number = (chapter.chapter_number >= 0.0).then_some(chapter.chapter_number);
            let (label, suffix) = chapter_label(settings, &chapter.name, number);
            spans.push(Span::styled(label, chapter_title_style(read)));
            if let Some(suffix) = suffix {
                spans.push(Span::styled(
                    format!("  {suffix}"),
                    Style::default().fg(c_overlay0()),
                ));
            }
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
            if chapter.upload_date > 0 {
                let seconds = if chapter.upload_date > 10_000_000_000 {
                    chapter.upload_date / 1000
                } else {
                    chapter.upload_date
                };
                if let Some(date) = DateTime::from_timestamp(seconds, 0) {
                    spans.push(Span::styled(
                        format!("  {}", date.format("%Y-%m-%d")),
                        Style::default().fg(c_overlay0()),
                    ));
                }
            }
            if let Some(entry) = entry.filter(|_| read == ReadState::InProgress) {
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
        SettingId::ReaderFit => (
            "Page fit".to_owned(),
            match app.config.reader_fit {
                tsuiyomi_core::ReaderFit::Fit => "Fit",
                tsuiyomi_core::ReaderFit::Crop => "Fill / crop",
                tsuiyomi_core::ReaderFit::Scale => "Stretch",
            }
            .to_owned(),
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
        SettingId::UpdateInterval => (
            "Update interval".to_owned(),
            if app.config.update_interval_hours == 0 {
                "Off".to_owned()
            } else {
                format!("{}h", app.config.update_interval_hours)
            },
        ),
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
        SettingId::ImportBackup => ("Import Mihon backup".to_owned(), "Enter path".to_owned()),
        SettingId::ExportBackup => ("Export Mihon backup".to_owned(), "Enter path".to_owned()),
        SettingId::DownloadsPath => (
            "Downloads folder".to_owned(),
            downloads_dir(&app.config)
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "not set".to_owned()),
        ),
        SettingId::FlareSolverr => (
            "FlareSolverr URL".to_owned(),
            app.config
                .flaresolverr_url
                .clone()
                .unwrap_or_else(|| "off".to_owned()),
        ),
        SettingId::ShowNsfw => (
            "Show NSFW extensions".to_owned(),
            if app.config.show_nsfw_extensions {
                "Yes"
            } else {
                "No"
            }
            .to_owned(),
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
        LibrarySort::ChapterCountDesc => "Most chapters",
        LibrarySort::LatestChapterDesc => "Latest chapter",
        LibrarySort::UnreadDesc => "Most unread",
        LibrarySort::LastReadDesc => "Recently read",
        LibrarySort::DateAddedDesc => "Recently added",
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
    let block = panel(
        Line::from(Span::styled(
            " Downloads ",
            Style::default().fg(c_accent()).add_modifier(Modifier::BOLD),
        )),
        more_content_border(app),
    );
    let inner = block.inner(area);
    frame.render_widget(&block, area);
    let mut lines = vec![field("Folder", &root, c_blue())];
    if let Some(label) = app.download_queue.label() {
        lines.push(field("Queue", &label, c_yellow()));
    }
    if let Some((_, title, _)) = &app.download_queue.active {
        lines.push(field("Active", title, c_teal()));
    }
    for label in app.download_queue.pending_labels().iter().take(3) {
        lines.push(Line::from(Span::styled(
            format!("  waiting · {label}"),
            Style::default().fg(c_overlay1()),
        )));
    }
    lines.push(Line::from(""));
    if app.downloaded_series.is_empty() {
        lines.push(Line::from(Span::styled(
            if app.downloads_loaded {
                "No downloaded chapters."
            } else {
                "Press Enter to load downloaded series."
            },
            Style::default().fg(c_overlay1()),
        )));
    } else {
        for (index, series) in app.downloaded_series.iter().enumerate() {
            let selected = index == app.downloads_selection;
            lines.push(Line::from(vec![
                Span::styled(
                    if selected { "› " } else { "  " },
                    Style::default().fg(c_accent()),
                ),
                Span::styled(
                    format!("{} · {}", series.source, series.series),
                    if selected {
                        Style::default().fg(c_accent()).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(c_text())
                    },
                ),
                Span::styled(
                    format!(
                        "  {} chapters · {}",
                        series.chapters,
                        format_bytes(series.bytes)
                    ),
                    Style::default().fg(c_overlay0()),
                ),
            ]));
        }
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "r refresh · x/Del delete selected series · Esc back",
            Style::default().fg(c_overlay0()),
        )));
    }
    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .style(Style::default().fg(c_subtext1()))
            .wrap(Wrap { trim: true }),
        inner,
    );
}

fn draw_downloads_delete_confirm(frame: &mut Frame, app: &AppState) {
    if !app.downloads_delete_confirm {
        return;
    }
    let Some(series) = app.downloaded_series.get(app.downloads_selection) else {
        return;
    };
    let area = frame.area();
    let width = 66.min(area.width).max(24);
    let height = 7.min(area.height);
    let popup = Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    );
    let block = panel(
        Line::from(Span::styled(
            " Delete downloads ",
            Style::default().fg(c_peach()).add_modifier(Modifier::BOLD),
        )),
        c_peach(),
    );
    let inner = block.inner(popup);
    frame.render_widget(Clear, popup);
    frame.render_widget(&block, popup);
    frame.render_widget(
        Paragraph::new(Text::from(vec![
            Line::from(format!("Delete downloaded chapters for {}?", series.series)),
            Line::from(Span::styled(
                format!("{} · {} chapters", series.source, series.chapters),
                Style::default().fg(c_overlay1()),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Enter / y delete · Esc cancel",
                Style::default().fg(c_overlay1()),
            )),
        ])),
        inner,
    );
}

fn draw_more_categories(frame: &mut Frame, area: Rect, app: &AppState) {
    let mut lines = Vec::new();
    for (index, category) in app.library.categories.iter().enumerate() {
        let count = app
            .library
            .series
            .iter()
            .filter(|series| series.categories.iter().any(|name| name == category))
            .count();
        let selected = index == app.category_manage_index;
        lines.push(Line::from(vec![
            Span::styled(
                if selected { "› " } else { "  " },
                Style::default().fg(c_accent()),
            ),
            Span::styled(
                category.clone(),
                if selected {
                    Style::default().fg(c_accent()).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(c_text())
                },
            ),
            Span::styled(
                format!("  {count} series"),
                Style::default().fg(c_overlay0()),
            ),
        ]));
    }
    if lines.is_empty() {
        lines.push(Line::from("No categories found."));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "a add · Enter rename · x/Del remove · Esc back",
        Style::default().fg(c_overlay0()),
    )));

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
    let mut read = 0usize;
    let mut in_progress = 0usize;
    let mut unread = 0usize;
    for series in &app.library.series {
        for chapter in &series.chapters {
            match read_state(
                app.progress
                    .entry(&chapter.path)
                    .map(|entry| (entry.page, entry.pages)),
            ) {
                ReadState::Read => read += 1,
                ReadState::InProgress => in_progress += 1,
                ReadState::Unread => unread += 1,
            }
        }
    }
    let stats = &app.reading_stats;
    let lines = vec![
        Line::from(Span::styled(
            " Library",
            Style::default()
                .fg(c_overlay0())
                .add_modifier(Modifier::BOLD),
        )),
        stat_field("Series", &app.library.series.len().to_string(), c_green()),
        stat_field("Chapters", &chapters.to_string(), c_green()),
        stat_field(
            "Categories",
            &app.library.categories.len().to_string(),
            c_green(),
        ),
        stat_field(
            "Scan errors",
            &app.library.errors.len().to_string(),
            c_peach(),
        ),
        Line::from(""),
        Line::from(Span::styled(
            " Chapters",
            Style::default()
                .fg(c_overlay0())
                .add_modifier(Modifier::BOLD),
        )),
        stat_field("Read", &read.to_string(), c_green()),
        stat_field("In progress", &in_progress.to_string(), c_blue()),
        stat_field("Unread", &unread.to_string(), c_yellow()),
        Line::from(""),
        Line::from(Span::styled(
            " Reading time",
            Style::default()
                .fg(c_overlay0())
                .add_modifier(Modifier::BOLD),
        )),
        stat_field(
            "All time",
            &format_duration(stats.total_seconds),
            c_accent(),
        ),
        stat_field(
            "Today",
            &format_duration(stats.day_seconds(&today_key())),
            c_green(),
        ),
        stat_field(
            "Last 7d",
            &format_duration(stats.range_seconds(7)),
            c_teal(),
        ),
        stat_field(
            "Last 30d",
            &format_duration(stats.range_seconds(30)),
            c_blue(),
        ),
        stat_field(
            "Series read",
            &stats.series_seconds.len().to_string(),
            c_mauve(),
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
        .style(Style::default().fg(c_subtext1()))
        .wrap(Wrap { trim: true });
    frame.render_widget(paragraph, area);
}

fn stat_field(label: &str, value: &str, value_color: Color) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{label:<12}"), Style::default().fg(c_overlay1())),
        Span::styled(value.to_owned(), Style::default().fg(value_color)),
    ])
}

fn draw_more_storage(frame: &mut Frame, area: Rect, app: &AppState) {
    let mut lines = vec![
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
    ];
    if let Some(storage) = &app.storage_snapshot {
        lines.push(Line::from(""));
        lines.push(field("Series", &storage.series.to_string(), c_green()));
        lines.push(field(
            "Installed extensions",
            &storage.extensions.to_string(),
            c_green(),
        ));
        lines.push(field(
            "Library size",
            &format_bytes(storage.library_bytes),
            c_blue(),
        ));
        lines.push(field(
            "Downloads",
            &format_bytes(storage.downloads_bytes),
            c_blue(),
        ));
        lines.push(field(
            "Host data",
            &format_bytes(storage.host_bytes),
            c_blue(),
        ));
    } else {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Press r to calculate storage usage.",
            Style::default().fg(c_overlay1()),
        )));
    }
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
    fn global_search_renders_failed_source_rows() {
        let mut app = test_app();
        app.tab = 3;
        app.browse_view = BrowseView::GlobalResults;
        app.remote.global_query = "alpha".to_owned();
        app.remote.global_total = 1;
        app.remote.global_finished = 1;
        app.remote.global_groups = vec![crate::app::GlobalGroup {
            source: RemoteSource {
                id: "1".to_owned(),
                name: "Demo".to_owned(),
                lang: "en".to_owned(),
                pkg: Some("pkg.demo".to_owned()),
                supports_latest: true,
            },
            mangas: Vec::new(),
            error: Some("timeout".to_owned()),
            loading: false,
        }];

        let text = render(&mut app, 100, 30);
        assert!(text.contains("Demo (EN)"));
        assert!(text.contains("error"));
        assert!(text.contains("timeout"));
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
                obsolete: false,
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
        assert!(text.contains("Incognito mode"));
        assert!(text.contains("Clear cookies"));
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
            obsolete: false,
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
    fn commands_box_grows_when_items_wrap() {
        let app = test_app();
        let wide = commands_height(&app, Rect::new(0, 0, 180, 40));
        let narrow = commands_height(&app, Rect::new(0, 0, 54, 40));
        assert_eq!(wide, 3);
        assert!(narrow > wide);
        let items = command_items(&app);
        let lines = command_line_count(&app, &items, 52);
        assert_eq!(narrow, lines as u16 + 2);
    }

    #[test]
    fn selected_tab_uses_lavender_text_without_background() {
        let mut app = test_app();
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();

        let area = Rect::new(0, 0, 80, 24);
        let layout = app_layout_with_commands(area, false, commands_height(&app, area));
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
    fn more_statistics_show_read_state_and_reading_time() {
        let mut app = test_app();
        let chapter = PathBuf::from("/tmp/library/Test Series/Chapter 1");
        app.progress.update(&chapter, 0, 1);
        app.reading_stats.total_seconds = 3661;
        app.reading_stats.daily_seconds.insert(today_key(), 60);
        app.tab = 4;
        app.more_section = MoreSection::Statistics;

        let text = render(&mut app, 80, 30);
        assert!(text.contains("Statistics"));
        assert!(text.contains("Read"));
        assert!(text.contains("In progress"));
        assert!(text.contains("All time"));
        assert!(text.contains("1h 01m"));
        assert!(text.contains("Categories"));
    }

    #[test]
    fn relative_until_labels_cover_due_and_future_windows() {
        let now = Local::now().timestamp().max(0) as u64;
        assert_eq!(relative_until(now.saturating_sub(5)), "due now");
        assert_eq!(relative_until(now + 120), "in 2m");
        assert_eq!(relative_until(now + 7_200), "in 2h");
        assert_eq!(relative_until(now + 172_800), "in 2d");
    }

    #[test]
    fn relative_age_labels_cover_minutes_hours_and_days() {
        let now = Local::now().timestamp().max(0) as u64;
        assert_eq!(relative_age(now.saturating_sub(30)), "just now");
        assert_eq!(relative_age(now.saturating_sub(120)), "2m ago");
        assert_eq!(relative_age(now.saturating_sub(7_200)), "2h ago");
        assert_eq!(relative_age(now.saturating_sub(172_800)), "2d ago");
    }

    #[test]
    fn remote_story_warns_when_the_source_is_not_loaded() {
        let manga = RemoteManga {
            source_id: "42".to_owned(),
            url: "/manga/test".to_owned(),
            title: "Test".to_owned(),
            author: None,
            artist: None,
            description: None,
            genre: Vec::new(),
            status: "unknown".to_owned(),
            thumbnail_url: None,
            memo: None,
        };
        let lines = remote_story_lines(&manga, &[], None, "Not saved", None, None);
        let text: String = lines
            .iter()
            .flat_map(|line| line.spans.iter())
            .map(|span| span.content.as_ref())
            .collect();
        assert!(text.contains("unavailable"));
        assert!(text.contains("source extension is not loaded"));

        let lines = remote_story_lines(&manga, &[], Some("Source (EN)"), "Saved", Some(0), Some(0));
        let text: String = lines
            .iter()
            .flat_map(|line| line.spans.iter())
            .map(|span| span.content.as_ref())
            .collect();
        assert!(!text.contains("unavailable"));
    }

    #[test]
    fn updates_tab_lists_new_chapters() {
        let mut app = test_app();
        app.updates
            .observe(&app.library, 100)
            .unwrap_or_else(|error| panic!("{error}"));
        app.library.series[0].chapters.push(Chapter {
            path: PathBuf::from("/tmp/library/Test Series/Chapter 2"),
            title: "Chapter 2".to_owned(),
            number: Some(2.0),
            kind: ChapterKind::Directory,
            scanlator: None,
            date: None,
        });
        app.updates
            .observe(&app.library, 200)
            .unwrap_or_else(|error| panic!("{error}"));
        app.tab = 1;

        let text = render(&mut app, 80, 24);
        assert!(text.contains("Test Series"));
        assert!(text.contains("Chapter 2"));
        assert!(text.contains("Updates (1)"));
        assert!(!text.contains("No new chapters yet"));
    }

    #[test]
    fn history_stats_tab_renders_totals_and_top_series() {
        let mut app = test_app();
        app.tab = 2;
        app.history_section = HistorySection::Stats;
        let mut entry = HistoryEntry::local(
            "Test Series".to_owned(),
            "Chapter 1".to_owned(),
            PathBuf::from("/library/Test Series"),
            PathBuf::from("/library/Test Series/Chapter 1"),
            None,
            0,
            10,
        );
        entry.read_duration_secs = 125;
        entry.daily_duration_secs.insert(today_key(), 125);
        app.history
            .record("/library/Test Series/Chapter 1".to_owned(), entry);
        app.reading_stats = app.history.reading_stats();

        let text = render(&mut app, 100, 40);
        assert!(text.contains("Stats"));
        assert!(text.contains("All time"));
        assert!(text.contains("Top series"));
        assert!(text.contains("Test Series"));
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
                read_duration_secs: 0,
                daily_duration_secs: std::collections::BTreeMap::new(),
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
