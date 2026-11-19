//! Ratatui frontend for tsuiyomi.
//!
//! The TUI owns terminal setup, input, and rendering. All library data comes from
//! `tsuiyomi-core`.

mod app;
mod download_worker;
mod draw;
mod host_worker;
mod input;
mod layout;
mod palette;
mod scroll_worker;
mod story_worker;

pub use app::AppState;

use std::{
    io,
    time::{Duration, Instant},
};

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture},
    execute,
};
use ratatui::{layout::Rect, DefaultTerminal};
use ratatui_image::picker::{cap_parser::QueryStdioOptions, Picker};
use tsuiyomi_core::{BookmarkStore, ProgressStore};

use crate::{
    app::{
        BrowseTab, BrowseView, LibraryView, MoreFocus, MoreRow, MoreSection, SettingId,
        SettingsPage, SETTINGS_PAGES,
    },
    download_worker::DownloadWorker,
    draw::draw,
    host_worker::HostWorker,
    input::Action,
    layout::{
        app_layout, contains, grid_index_at, inner, library_grid_area, library_layout, more_layout,
        reader_bar_layout, setting_columns, settings_rows, tab_layouts, tile_cover_height,
        TILE_TEXT_HEIGHT,
    },
    scroll_worker::ScrollWorker,
    story_worker::StoryWorker,
};

/// Run the TUI until the user quits.
pub fn run(mut app: AppState) -> io::Result<()> {
    // Query capabilities before entering the alternate screen. This also drains the
    // terminal's query responses before the event loop starts, so the first keypress
    // does not trigger a stray redraw/flash.
    let picker = Picker::from_query_stdio_with_options(QueryStdioOptions {
        kitty_shared_memory_object: QueryStdioOptions::probe_kitty_smo(),
        // Shared memory avoids the transmission entirely when available. Otherwise
        // zlib shrinks the base64 payload that the UI thread has to copy and write.
        kitty_compression: true,
        ..Default::default()
    })
    .unwrap_or_else(|_| Picker::halfblocks());

    let mut terminal = ratatui::init();
    app.set_image_picker(picker);
    app.set_progress(ProgressStore::load_default());
    app.set_bookmarks(BookmarkStore::load_default());
    app.set_scroll_worker(ScrollWorker::spawn());
    app.set_size_worker(crate::scroll_worker::SizeWorker::spawn());
    app.set_download_worker(DownloadWorker::spawn());
    app.set_story_worker(StoryWorker::spawn());
    app.set_host_worker(HostWorker::spawn());

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
    app.flush_progress();

    let _ = execute!(std::io::stdout(), DisableMouseCapture);
    ratatui::restore();

    result
}

fn event_loop(terminal: &mut DefaultTerminal, app: &mut AppState) -> io::Result<()> {
    let mut needs_draw = true;
    let mut pending_action = None;
    let mut startup = true;
    loop {
        if startup {
            startup = false;
            // Apply terminal resize events that arrive while entering the alternate screen
            // before the first frame, instead of letting them look like a resize triggered
            // by the first keypress.
            let deadline = Instant::now() + Duration::from_millis(20);
            while Instant::now() < deadline {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if !event::poll(remaining)? {
                    break;
                }
                let event = event::read()?;
                if matches!(event, event::Event::Resize(_, _)) {
                    terminal.autoresize()?;
                } else {
                    pending_action = input::action_from_event(event, false);
                    break;
                }
            }
            terminal.autoresize()?;
        }
        if app.progress_save_due() {
            app.flush_progress();
        }
        if app.poll_scroll_results() {
            needs_draw = true;
        }
        if app.poll_downloads() {
            needs_draw = true;
        }
        if app.poll_story_refresh() {
            needs_draw = true;
        }
        if app.poll_host_start() {
            needs_draw = true;
        }
        if needs_draw {
            let started = Instant::now();
            terminal.draw(|frame| draw(frame, app))?;
            let elapsed = started.elapsed();
            if elapsed > Duration::from_millis(40) {
                app::scroll_debug(&format!("frame {:?}", elapsed));
            }
            needs_draw = false;
            if app.needs_scroll_redraw() {
                // Reveal the next fresh scroll block right away instead of waiting
                // for another input event.
                continue;
            }
        }

        let action = if let Some(action) = pending_action.take() {
            Some(action)
        } else if event::poll(Duration::from_millis(30))? {
            input::read(app.text_mode())?
        } else {
            continue;
        };
        let Some(action) = action else {
            continue;
        };

        if app.quit_confirm {
            match action {
                Action::Quit | Action::Open | Action::Confirm => return Ok(()),
                Action::Back | Action::NextChapter => app.quit_confirm = false,
                _ => {}
            }
            needs_draw = true;
            continue;
        }

        if app.uninstall_confirm.is_some() {
            match action {
                Action::Confirm => app.confirm_uninstall_extension(),
                _ => app.cancel_uninstall_extension(),
            }
            needs_draw = true;
            continue;
        }

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

        if app.text_input.is_some() {
            match action {
                Action::Back => app.input_cancel(),
                Action::Open => app.input_submit(),
                Action::Char(character) => app.input_char(character),
                Action::Backspace => app.input_backspace(),
                _ => {}
            }
            needs_draw = true;
            continue;
        }

        if app.extension_search {
            match action {
                Action::Back => app.extension_search_stop(true),
                Action::Open => app.extension_search_stop(false),
                Action::Char(character) => app.extension_search_push(character),
                Action::Backspace => app.extension_search_pop(),
                Action::Previous => app.browse_previous(),
                Action::Next => app.browse_next(),
                _ => {}
            }
            needs_draw = true;
            continue;
        }

        // The reader owns the screen while it is open: tabs are hidden, and returning
        // goes back to the list it was opened from.
        if app.reader.is_some() {
            match action {
                Action::Quit => app.quit_confirm = true,
                Action::Back => app.close_reader(),
                Action::Previous | Action::ScrollUp => app.previous(),
                Action::Next | Action::ScrollDown => app.next(),
                Action::PreviousPage => app.previous_page(),
                Action::NextPage | Action::Open => app.next_page(),
                Action::PreviousScreen => app.previous_screen(),
                Action::NextScreen => app.next_screen(),
                Action::PreviousChapter => app.previous_chapter(),
                Action::NextChapter => app.next_chapter(),
                Action::ToggleReaderMode => app.toggle_reader_mode(),
                Action::ZoomIn => app.zoom_in(),
                Action::ZoomOut => app.zoom_out(),
                Action::ToggleFullscreen => app.toggle_fullscreen(),
                Action::Click { column, row } => {
                    let (width, height) = crossterm::terminal::size()?;
                    let layout = app_layout(Rect::new(0, 0, width, height), app.fullscreen);
                    handle_reader_click(layout.tabs, app, column, row);
                }
                _ => {}
            }
            needs_draw = true;
            continue;
        }

        match action {
            Action::Quit => app.quit_confirm = true,
            Action::Confirm => {}
            Action::Previous => {
                if app.tab == 3 {
                    if app.browse_view == BrowseView::GlobalResults {
                        app.global_previous_group();
                    } else {
                        app.browse_previous();
                    }
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
                    if app.browse_view == BrowseView::GlobalResults {
                        app.global_next_group();
                    } else {
                        app.browse_next();
                    }
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
                    match app.browse_view {
                        BrowseView::Local | BrowseView::RemoteResults => {
                            app.browse_previous_column();
                        }
                        BrowseView::Chapters | BrowseView::RemoteDetails | BrowseView::Migrate => {
                            app.browse_parent()
                        }
                        BrowseView::GlobalResults => app.global_previous_item(),
                        BrowseView::Sources | BrowseView::Extensions | BrowseView::Stores => {}
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
                    match app.browse_view {
                        BrowseView::Local | BrowseView::RemoteResults => {
                            app.browse_next_column();
                        }
                        BrowseView::Sources
                        | BrowseView::Chapters
                        | BrowseView::RemoteDetails
                        | BrowseView::Migrate => app.browse_open(),
                        BrowseView::GlobalResults => app.global_next_item(),
                        BrowseView::Extensions | BrowseView::Stores => {}
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
                if app.tab == 3 {
                    app.previous_browse_tab();
                } else if app.tab == 4 {
                    app.previous_more_section();
                } else if app.tab == 0 && app.view == LibraryView::Series {
                    app.previous_category();
                }
            }
            Action::NextCategory => {
                if app.tab == 3 {
                    app.next_browse_tab();
                } else if app.tab == 4 {
                    app.next_more_section();
                } else if app.tab == 0 && app.view == LibraryView::Series {
                    app.next_category();
                }
            }
            Action::SelectTab(index) => app.select_tab(index),
            Action::Refresh => {
                if app.tab == 3 {
                    app.refresh_browse();
                } else if app.tab == 0 && app.view == LibraryView::RemoteDetails {
                    app.reload_library_remote_chapters();
                } else {
                    app.refresh();
                }
            }
            Action::Search => {
                if app.tab == 3 {
                    match app.browse_view {
                        BrowseView::RemoteResults => {
                            let query = app.remote.query.clone();
                            app.open_text_input(
                                app::TextAction::SearchRemote,
                                "Search source",
                                &query,
                            );
                        }
                        BrowseView::Extensions => app.extension_search_start(),
                        BrowseView::Sources => {
                            app.open_text_input(
                                app::TextAction::GlobalSearch,
                                "Search pinned sources",
                                "",
                            );
                        }
                        _ => {}
                    }
                }
            }
            Action::Pin => {
                if app.tab == 3 {
                    match app.browse_view {
                        BrowseView::Sources => app.toggle_pin_for_selection(),
                        BrowseView::Extensions => app.toggle_selected_extension_pin(),
                        _ => {}
                    }
                }
            }
            Action::HostToggle => {
                if app.tab == 4 && app.more_section == MoreSection::About {
                    app.toggle_host();
                }
            }
            Action::Favorite => {
                if app.tab == 3 && app.browse_view == BrowseView::RemoteDetails {
                    app.toggle_current_favorite();
                }
            }
            Action::Latest => {
                if app.tab == 3 && app.browse_view == BrowseView::RemoteResults {
                    app.load_remote_latest();
                }
            }
            Action::Extensions => {
                if app.tab == 3 {
                    app.set_browse_tab(BrowseTab::Extensions);
                }
            }
            Action::Stores => {
                if app.tab == 3 {
                    if app.browse_view == BrowseView::Stores {
                        app.open_extensions();
                    } else {
                        app.set_browse_tab(BrowseTab::Extensions);
                        app.open_stores();
                    }
                }
            }
            Action::AddStore => {
                if app.tab == 3
                    && matches!(app.browse_view, BrowseView::Extensions | BrowseView::Stores)
                {
                    app.open_text_input(
                        app::TextAction::AddExtensionStore,
                        "Extension store URL",
                        "",
                    );
                }
            }
            Action::DeleteStore => {
                if app.tab == 3 && app.browse_view == BrowseView::Stores {
                    app.remove_selected_store();
                }
            }
            Action::Download => {
                if app.has_downloadable_chapter() {
                    app.download_selected_chapter();
                }
            }
            Action::Bookmark => app.toggle_bookmark(),
            Action::UpdateExtension => {
                if app.tab == 3 && app.browse_view == BrowseView::Extensions {
                    app.update_selected_extension();
                }
            }
            Action::InstallExtension => {
                if app.tab == 3 && app.browse_view == BrowseView::Extensions {
                    app.install_selected_extension();
                }
            }
            Action::UninstallExtension => {
                if app.tab == 3 && app.browse_view == BrowseView::Extensions {
                    app.uninstall_selected_extension();
                }
            }
            Action::Open => {
                if app.tab == 3 {
                    if app.browse_view == BrowseView::GlobalResults {
                        app.global_open();
                    } else {
                        app.browse_open();
                    }
                } else if app.tab == 4 {
                    if app.more_focus == MoreFocus::Sections {
                        app.focus_more_content();
                    } else {
                        app.activate_setting();
                    }
                } else if app.tab == 0 {
                    app.open();
                }
            }
            Action::Back => {
                if app.tab == 3 {
                    app.browse_parent();
                } else if app.tab == 4 {
                    app.leave_more();
                } else if app.tab == 0 {
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

    if app.reader.is_some() {
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

    if app.view == LibraryView::RemoteDetails {
        let library = library_layout(layout.content);
        let list_area = inner(library.list);
        if contains(list_area, column, row) {
            let visible_row = (row - list_area.y) as usize;
            let index = app.library_remote.list_state.offset() + visible_row;
            app.select_index(index);
        }
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
            tile_cover_height(app.config.thumbnail_width, grid_area.height) + TILE_TEXT_HEIGHT,
        ) {
            app.select_index(index);
        }
        return;
    }

    handle_chapter_click(layout.content, app, column, row);
}

fn handle_chapter_click(area: Rect, app: &mut AppState, column: u16, row: u16) {
    let library = library_layout(area);
    let list_area = inner(library.list);
    if contains(list_area, column, row) {
        let visible_row = (row - list_area.y) as usize;
        let index = app.chapter_list_state.offset() + visible_row;
        app.select_index(index);
    }
}

fn handle_browse_click(area: Rect, app: &mut AppState, column: u16, row: u16) {
    let inside = |top: u16| {
        Rect::new(
            area.x + 1,
            area.y + top,
            area.width.saturating_sub(2),
            area.height.saturating_sub(top),
        )
    };
    match app.browse_view {
        BrowseView::Sources => {
            let list = inside(2);
            if contains(list, column, row) {
                let offset = app.browse_list_state.offset();
                app.select_source_row(offset + (row - list.y) as usize);
            }
        }
        BrowseView::Extensions => {
            let list = inside(3);
            if contains(list, column, row) {
                let offset = app.browse_list_state.offset();
                app.select_browse_index(offset + (row - list.y) as usize);
            }
        }
        BrowseView::Stores => {
            let list = inside(2);
            if contains(list, column, row) {
                let offset = app.browse_list_state.offset();
                app.select_browse_index(offset + (row - list.y) as usize);
            }
        }
        BrowseView::Local | BrowseView::RemoteResults => {
            let grid_area = library_grid_area(area);
            if let Some(index) = grid_index_at(
                grid_area,
                app.browse_columns.max(1),
                app.browse_grid_offset,
                column,
                row,
                app.browse_item_count(),
                tile_cover_height(app.config.thumbnail_width, grid_area.height) + TILE_TEXT_HEIGHT,
            ) {
                app.select_browse_index(index);
            }
        }
        BrowseView::Chapters => {
            let content = library_grid_area(area);
            let library = library_layout(content);
            let list_area = inner(library.list);
            if contains(list_area, column, row) {
                let visible_row = (row - list_area.y) as usize;
                let index = app.browse_local_list_state.offset() + visible_row;
                app.select_browse_index(index);
            }
        }
        BrowseView::RemoteDetails => {
            let content = library_grid_area(area);
            let library = library_layout(content);
            let list_area = inner(library.list);
            if contains(list_area, column, row) {
                let visible_row = (row - list_area.y) as usize;
                let index = app.browse_list_state.offset() + visible_row;
                app.select_browse_index(index);
            }
        }
        BrowseView::GlobalResults | BrowseView::Migrate => {}
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
                | SettingId::ThumbnailSize
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
