//! Ratatui frontend for tsuiyomi.
//!
//! The TUI owns terminal setup, input, and rendering. All library data comes from
//! `tsuiyomi-core`.

mod app;
mod draw;
mod input;
mod layout;
mod palette;
mod scroll_worker;

pub use app::AppState;

use std::{io, time::Duration};

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture},
    execute,
};
use ratatui::{layout::Rect, DefaultTerminal};
use ratatui_image::picker::{cap_parser::QueryStdioOptions, Picker};
use tsuiyomi_core::ProgressStore;

use crate::{
    app::{
        BrowseView, LibraryView, MoreFocus, MoreRow, MoreSection, SettingId, SettingsPage,
        SETTINGS_PAGES,
    },
    draw::draw,
    input::Action,
    layout::{
        app_layout, contains, grid_index_at, inner, library_grid_area, library_layout, more_layout,
        reader_bar_layout, setting_columns, settings_rows, tab_layouts,
    },
    scroll_worker::ScrollWorker,
};

/// Run the TUI until the user quits.
pub fn run(mut app: AppState) -> io::Result<()> {
    let mut terminal = ratatui::init();

    // Queries the terminal for Kitty/Sixel/iTerm2 support and font size. Must happen
    // before any input events are read. Prefer Kitty shared memory for transmissions.
    let picker = Picker::from_query_stdio_with_options(QueryStdioOptions {
        kitty_shared_memory_object: QueryStdioOptions::probe_kitty_smo(),
        ..Default::default()
    })
    .unwrap_or_else(|_| Picker::halfblocks());
    app.set_image_picker(picker);
    app.set_progress(ProgressStore::load_default());
    app.set_scroll_worker(ScrollWorker::spawn());

    // ratatui installs a panic hook that restores the terminal. Wrap it so a panic also
    // releases mouse capture.
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = execute!(std::io::stdout(), DisableMouseCapture);
        previous_hook(info);
    }));

    if let Err(error) = execute!(std::io::stdout(), EnableMouseCapture) {
        ratatui::restore();
        return Err(error);
    }

    let result = event_loop(&mut terminal, &mut app);

    let _ = execute!(std::io::stdout(), DisableMouseCapture);
    ratatui::restore();

    result
}

fn event_loop(terminal: &mut DefaultTerminal, app: &mut AppState) -> io::Result<()> {
    let mut needs_draw = true;
    loop {
        if app.poll_scroll_results() {
            needs_draw = true;
        }
        if needs_draw {
            terminal.draw(|frame| draw(frame, app))?;
            needs_draw = false;
        }

        if !event::poll(Duration::from_millis(30))? {
            continue;
        }
        let Some(action) = input::read(app.picker.is_some())? else {
            continue;
        };

        if app.picker.is_some() {
            match action {
                Action::Quit => return Ok(()),
                Action::Previous => app.picker_previous(),
                Action::Next => app.picker_next(),
                Action::Open => app.picker_confirm(),
                Action::Back => app.picker_back(),
                Action::Char(character) => app.picker_char(character),
                Action::Backspace => app.picker_backspace(),
                _ => {}
            }
            needs_draw = true;
            continue;
        }

        match action {
            Action::Quit => return Ok(()),
            Action::Previous => {
                if app.tab == 3 {
                    app.browse_previous();
                } else if app.tab == 4 {
                    if app.more_focus == MoreFocus::Sections {
                        app.previous_more_section();
                    } else {
                        app.previous_setting();
                    }
                } else {
                    app.previous();
                }
            }
            Action::Next => {
                if app.tab == 3 {
                    app.browse_next();
                } else if app.tab == 4 {
                    if app.more_focus == MoreFocus::Sections {
                        app.next_more_section();
                    } else {
                        app.next_setting();
                    }
                } else {
                    app.next();
                }
            }
            Action::PreviousPage => {
                if app.tab == 3 {
                    if app.browse_view == BrowseView::Details {
                        app.browse_parent();
                    } else {
                        app.browse_previous_column();
                    }
                } else if app.tab == 4 {
                    if app.more_focus == MoreFocus::Content {
                        app.adjust_setting(-1);
                    }
                } else if app.tab == 0 && app.view == LibraryView::Series {
                    app.previous_column();
                } else {
                    app.previous_page();
                }
            }
            Action::NextPage => {
                if app.tab == 3 {
                    if app.browse_view == BrowseView::Details {
                        app.browse_open();
                    } else {
                        app.browse_next_column();
                    }
                } else if app.tab == 4 {
                    if app.more_focus == MoreFocus::Content {
                        app.adjust_setting(1);
                    }
                } else if app.tab == 0 && app.view == LibraryView::Series {
                    app.next_column();
                } else {
                    app.next_page();
                }
            }
            Action::PreviousScreen => app.previous_screen(),
            Action::NextScreen => app.next_screen(),
            Action::PreviousChapter => app.previous_chapter(),
            Action::NextChapter => app.next_chapter(),
            Action::ScrollUp => {
                if app.tab == 3 {
                    app.browse_previous();
                } else if app.tab == 4 {
                    if app.more_focus == MoreFocus::Sections {
                        app.previous_more_section();
                    } else {
                        app.previous_setting();
                    }
                } else {
                    app.previous();
                }
            }
            Action::ScrollDown => {
                if app.tab == 3 {
                    app.browse_next();
                } else if app.tab == 4 {
                    if app.more_focus == MoreFocus::Sections {
                        app.next_more_section();
                    } else {
                        app.next_setting();
                    }
                } else {
                    app.next();
                }
            }
            Action::PreviousTab => app.previous_tab(),
            Action::NextTab => app.next_tab(),
            Action::PreviousCategory => {
                if app.tab == 4 {
                    app.previous_more_section();
                } else {
                    app.previous_category();
                }
            }
            Action::NextCategory => {
                if app.tab == 4 {
                    app.next_more_section();
                } else {
                    app.next_category();
                }
            }
            Action::SelectTab(index) => app.select_tab(index),
            Action::Refresh => {
                if app.tab == 3 {
                    app.refresh_browse();
                } else {
                    app.refresh();
                }
            }
            Action::Open => {
                if app.tab == 3 {
                    app.browse_open();
                } else if app.tab == 4 {
                    if app.more_focus == MoreFocus::Sections {
                        app.focus_more_content();
                    } else {
                        app.activate_setting();
                    }
                } else {
                    app.open();
                }
            }
            Action::Back => {
                if app.tab == 3 {
                    app.browse_parent();
                } else if app.tab == 4 {
                    app.leave_more();
                } else {
                    app.back();
                }
            }
            Action::ToggleFullscreen => app.toggle_fullscreen(),
            Action::ToggleReaderMode => app.toggle_reader_mode(),
            Action::ZoomIn => {
                if app.tab == 4 {
                    if app.more_focus == MoreFocus::Content
                        && app.more_section == MoreSection::Settings
                    {
                        app.adjust_setting(1);
                    }
                } else {
                    app.zoom_in();
                }
            }
            Action::ZoomOut => {
                if app.tab == 4 {
                    if app.more_focus == MoreFocus::Content
                        && app.more_section == MoreSection::Settings
                    {
                        app.adjust_setting(-1);
                    }
                } else {
                    app.zoom_out();
                }
            }
            Action::OpenConfig => app.select_tab(4),
            Action::Redraw | Action::Char(_) | Action::Backspace => {}
            Action::Click { column, row } => {
                let (width, height) = crossterm::terminal::size()?;
                handle_click(Rect::new(0, 0, width, height), app, column, row);
            }
        }
        needs_draw = true;
    }
}

fn handle_click(area: Rect, app: &mut AppState, column: u16, row: u16) {
    let layout = app_layout(area, app.fullscreen);

    if app.tab == 0 && app.view == LibraryView::Reader {
        handle_reader_click(layout.tabs, app, column, row);
        return;
    }

    if contains(layout.tabs, column, row) {
        if let Some(tab) = tab_layouts(layout.tabs)
            .into_iter()
            .find(|tab| contains(tab.area, column, row))
        {
            app.select_tab(tab.index);
        }
        return;
    }

    if app.tab == 3 {
        handle_browse_click(layout.content, app, column, row);
        return;
    }

    if app.tab == 4 {
        handle_more_click(layout.content, app, column, row);
        return;
    }

    if app.tab != 0 {
        return;
    }

    if app.view == LibraryView::Series {
        let grid_area = library_grid_area(layout.content);
        if let Some(index) = grid_index_at(
            grid_area,
            app.library_columns.max(1),
            app.library_grid_offset,
            column,
            row,
            app.visible_count(),
        ) {
            app.select_index(index);
        }
        return;
    }

    let library = library_layout(layout.content);
    let list_area = inner(library.list);
    if contains(list_area, column, row) {
        let visible_row = (row - list_area.y) as usize;
        let index = app.chapter_list_state.offset() + visible_row;
        app.select_index(index);
    }
}

fn handle_browse_click(area: Rect, app: &mut AppState, column: u16, row: u16) {
    match app.browse_view {
        BrowseView::Local => {
            let grid_area = library_grid_area(area);
            if let Some(index) = grid_index_at(
                grid_area,
                app.browse_columns.max(1),
                app.browse_grid_offset,
                column,
                row,
                app.browse_item_count(),
            ) {
                app.select_browse_index(index);
            }
        }
        BrowseView::Sources => {
            let list = library_grid_area(area);
            if contains(list, column, row) {
                app.select_browse_index((row - list.y) as usize);
            }
        }
        BrowseView::Details => {}
    }
}

fn handle_more_click(area: Rect, app: &mut AppState, column: u16, row: u16) {
    let layout = more_layout(area);
    let sections = inner(layout.sections);
    if contains(sections, column, row) {
        app.set_more_section((row - sections.y) as usize);
        app.focus_more_content();
        return;
    }

    if app.more_section != MoreSection::Settings {
        return;
    }

    if app.settings_page == SettingsPage::Main {
        for (index, page_row) in settings_rows(layout.content, SETTINGS_PAGES.len())
            .into_iter()
            .enumerate()
        {
            if contains(page_row, column, row) {
                app.focus_more_content();
                app.setting_index = index;
                app.activate_setting();
                return;
            }
        }
        return;
    }

    let page = app.settings_page;
    let rows = page.rows();
    let page_rows = settings_rows(layout.content, rows.len());
    let mut selectable = 0;
    for (row_index, entry) in rows.iter().enumerate() {
        let Some(setting_row) = page_rows.get(row_index).copied() else {
            return;
        };
        let MoreRow::Setting(setting) = entry else {
            continue;
        };
        if !contains(setting_row, column, row) {
            selectable += 1;
            continue;
        }

        app.focus_more_content();
        app.setting_index = selectable;
        if matches!(
            setting,
            SettingId::Zoom
                | SettingId::PrefetchBehind
                | SettingId::PrefetchAhead
                | SettingId::CacheMargin
        ) {
            let columns = setting_columns(setting_row);
            if contains(columns[1], column, row) {
                let width = columns[1].width.max(1);
                let ratio = f64::from(column.saturating_sub(columns[1].x)) / f64::from(width);
                app.set_setting_ratio(selectable, ratio);
            }
        }
        return;
    }
}

fn handle_reader_click(area: Rect, app: &mut AppState, column: u16, row: u16) {
    let bar = reader_bar_layout(area);

    if contains(bar.previous, column, row) {
        if app.chapter_index + 1 < app.chapter_count() {
            app.previous_chapter();
        }
        return;
    }

    if contains(bar.next, column, row) {
        if app.chapter_index > 0 {
            app.next_chapter();
        }
        return;
    }

    if contains(bar.mode, column, row) {
        app.toggle_reader_mode();
        return;
    }

    if contains(bar.progress, column, row) {
        let width = bar.progress.width.max(1);
        let ratio = f64::from(column.saturating_sub(bar.progress.x)) / f64::from(width);
        let ratio = ratio.clamp(0.0, 1.0);
        let total = app.reader.as_ref().map_or(0, |reader| reader.pages.len());
        if total > 0 {
            let page = ((ratio * total as f64).floor() as usize).min(total - 1);
            app.jump_to_page(page);
        }
    }
}
