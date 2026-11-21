//! Ratatui frontend for tsuiyomi.
//!
//! The TUI owns terminal setup, input, and rendering. All library data comes from
//! `tsuiyomi-core`.

mod app;
mod cover_cache;
mod cover_worker;
mod download_worker;
mod draw;
mod extensions_worker;
mod favorites_worker;
mod filters_worker;
mod host_worker;
mod input;
mod layout;
mod migration_worker;
mod palette;
mod prefs_worker;
mod result_worker;
mod scan_worker;
mod scroll_worker;
mod search_worker;
mod source_worker;
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
use tsuiyomi_core::{BookmarkStore, HistoryStore, ProgressStore, UpdateStore};

use crate::{
    app::{
        BrowseTab, BrowseView, LibraryView, MoreFocus, MoreRow, MoreSection, SettingId,
        SettingsPage, SETTINGS_PAGES,
    },
    cover_worker::CoverWorker,
    download_worker::DownloadWorker,
    draw::{commands_height, draw},
    host_worker::HostWorker,
    input::Action,
    layout::{
        app_layout_with_commands, contains, grid_index_at, inner, library_grid_area,
        library_layout, more_layout, reader_bar_layout, setting_columns, settings_rows,
        tab_layouts, tile_cover_height, TILE_TEXT_HEIGHT,
    },
    scroll_worker::ScrollWorker,
    search_worker::GlobalSearchWorker,
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
    app.set_history(HistoryStore::load_default());
    app.set_updates(UpdateStore::load_default());
    if !app.library.series.is_empty() {
        app.observe_updates();
    }
    app.set_scroll_worker(ScrollWorker::spawn());
    app.set_size_worker(crate::scroll_worker::SizeWorker::spawn());
    app.set_download_worker(DownloadWorker::spawn());
    app.set_story_worker(StoryWorker::spawn());
    app.set_host_worker(HostWorker::spawn());
    app.set_result_worker(crate::result_worker::ResultWorker::spawn());
    app.set_source_worker(crate::source_worker::SourceWorker::spawn());
    app.set_favorites_worker(crate::favorites_worker::FavoritesWorker::spawn());
    app.set_filters_worker(crate::filters_worker::FiltersWorker::spawn());
    app.set_prefs_worker(crate::prefs_worker::PrefsWorker::spawn());
    app.set_extensions_worker(crate::extensions_worker::ExtensionsWorker::spawn());
    app.set_migration_worker(crate::migration_worker::MigrationWorker::spawn());
    app.set_scan_worker(crate::scan_worker::ScanWorker::spawn());
    app.set_global_search_worker(GlobalSearchWorker::spawn());
    app.set_cover_worker(CoverWorker::spawn());
    if app.library.series.is_empty() {
        app.refresh_async();
    }
    app.maybe_recover_covers();

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
    app.finish_reading_session();
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
        if app.reading_session_due() {
            app.flush_reading_session();
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
        if app.poll_extension_jobs() {
            needs_draw = true;
        }
        if app.poll_result_worker() {
            needs_draw = true;
        }
        if app.poll_source_worker() {
            needs_draw = true;
        }
        if app.poll_favorites_worker() {
            needs_draw = true;
        }
        if app.poll_filters_worker() {
            needs_draw = true;
        }
        if app.poll_prefs_worker() {
            needs_draw = true;
        }
        if app.poll_extensions_worker() {
            needs_draw = true;
        }
        if app.poll_migration_worker() {
            needs_draw = true;
        }
        if app.poll_scan_worker() {
            needs_draw = true;
        }
        if app.poll_story_refresh() {
            needs_draw = true;
        }
        if app.poll_host_start() {
            needs_draw = true;
        }
        if app.poll_global_search() {
            needs_draw = true;
        }
        if app.poll_cover_lookups() {
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
        app.note_reading_activity();

        if app.quit_confirm {
            match action {
                Action::Quit | Action::Open | Action::Confirm => return Ok(()),
                Action::Back | Action::NextChapter => app.quit_confirm = false,
                _ => {}
            }
            needs_draw = true;
            continue;
        }

        if app.downloads_delete_confirm {
            match action {
                Action::Open | Action::Confirm => app.confirm_downloads_delete(),
                Action::Back => app.cancel_downloads_delete(),
                _ => {}
            }
            needs_draw = true;
            continue;
        }

        if app.storage_clear_downloads_confirm {
            match action {
                Action::Open | Action::Confirm => app.confirm_clear_downloads(),
                Action::Back => app.cancel_clear_downloads(),
                _ => {}
            }
            needs_draw = true;
            continue;
        }

        if app.migration_search.is_some() {
            match action {
                Action::Quit => return Ok(()),
                Action::Previous => app.migration_search_previous(),
                Action::Next => app.migration_search_next(),
                Action::Open | Action::Confirm => app.confirm_migration_search(),
                Action::Back => app.cancel_migration_search(),
                _ => {}
            }
            needs_draw = true;
            continue;
        }

        if app.migration_source_picker.is_some() {
            match action {
                Action::Quit => return Ok(()),
                Action::Previous => app.migration_sources_previous(),
                Action::Next => app.migration_sources_next(),
                Action::Select => app.migration_sources_toggle(),
                Action::Open | Action::Confirm => app.apply_migration_sources(),
                Action::AddStore => app.migration_sources_all(true),
                Action::NextChapter => app.migration_sources_all(false),
                Action::ZoomIn => app.migration_sources_move(-1),
                Action::ZoomOut => app.migration_sources_move(1),
                Action::Back => app.cancel_migration_sources(),
                _ => {}
            }
            needs_draw = true;
            continue;
        }

        if app.tag_picker.is_some() {
            match action {
                Action::Quit => return Ok(()),
                Action::Previous => app.tag_picker_previous(),
                Action::Next => app.tag_picker_next(),
                Action::Open | Action::Confirm | Action::Select => app.confirm_tag_picker(),
                Action::Back => app.cancel_tag_picker(),
                _ => {}
            }
            needs_draw = true;
            continue;
        }

        if app.save_categories.is_some() {
            match action {
                Action::Quit => return Ok(()),
                Action::Previous => app.save_categories_previous(),
                Action::Next => app.save_categories_next(),
                Action::Select => app.save_categories_toggle(),
                Action::Open | Action::Confirm => app.confirm_save_categories(),
                Action::Back => app.cancel_save_categories(),
                _ => {}
            }
            needs_draw = true;
            continue;
        }

        if app.category_picker.is_some() {
            match action {
                Action::Quit => return Ok(()),
                Action::Previous => app.category_picker_previous(),
                Action::Next => app.category_picker_next(),
                Action::Select => app.category_picker_toggle(),
                Action::Open | Action::Confirm => app.confirm_category_picker(),
                Action::Back => app.cancel_category_picker(),
                _ => {}
            }
            needs_draw = true;
            continue;
        }

        if app.download_delete_confirm {
            match action {
                Action::Open | Action::Confirm => app.confirm_download_delete(),
                Action::Back => app.cancel_download_delete(),
                _ => {}
            }
            needs_draw = true;
            continue;
        }

        if app.download_menu.is_some() {
            match action {
                Action::Quit => return Ok(()),
                Action::Previous => app.download_menu_previous(),
                Action::Next => app.download_menu_next(),
                Action::Open | Action::Confirm | Action::Select => app.download_menu_confirm(),
                Action::Back => app.close_download_menu(),
                _ => {}
            }
            needs_draw = true;
            continue;
        }

        if app.chapter_settings_modal.is_some() {
            match action {
                Action::Quit => return Ok(()),
                Action::Previous => app.chapter_settings_previous(),
                Action::Next => app.chapter_settings_next(),
                Action::PreviousPage => app.chapter_settings_adjust(-1),
                Action::NextPage | Action::Open | Action::Confirm | Action::Select => {
                    app.chapter_settings_adjust(1)
                }
                Action::Back => app.close_chapter_settings(),
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

        if app.library_delete.is_some() {
            match action {
                Action::Previous => app.library_delete_previous(),
                Action::Next => app.library_delete_next(),
                Action::NextScreen | Action::Select => app.library_delete_toggle(),
                Action::Open | Action::Confirm => app.confirm_library_delete(),
                Action::Back => app.cancel_library_delete(),
                _ => {}
            }
            needs_draw = true;
            continue;
        }

        if app.duplicate_dialog.is_some() {
            match action {
                Action::Previous => app.duplicate_previous(),
                Action::Next => app.duplicate_next(),
                Action::Open | Action::Confirm => app.duplicate_open(),
                Action::AddStore => app.duplicate_add_anyway(),
                Action::Migrate => app.duplicate_migrate(),
                Action::Back => app.duplicate_cancel(),
                _ => {}
            }
            needs_draw = true;
            continue;
        }

        if app.remove_favorite.is_some() {
            match action {
                Action::Open | Action::Confirm => app.confirm_remove_favorite(),
                Action::Back => app.cancel_remove_favorite(),
                _ => {}
            }
            needs_draw = true;
            continue;
        }

        if app.history_delete.is_some() {
            match action {
                Action::Select => app.toggle_history_delete_all(),
                Action::Open | Action::Confirm => app.confirm_history_delete(),
                Action::Back => app.cancel_history_delete(),
                _ => {}
            }
            needs_draw = true;
            continue;
        }

        if app.history_clear_confirm {
            match action {
                Action::Open | Action::Confirm => app.confirm_history_clear(),
                Action::Back => app.cancel_history_clear(),
                _ => {}
            }
            needs_draw = true;
            continue;
        }

        if app.multi_select.is_some() && matches!(action, Action::Back) {
            app.clear_multi_select();
            needs_draw = true;
            continue;
        }

        if app.filter_picker.is_some() {
            match action {
                Action::Quit => return Ok(()),
                Action::Previous => app.filter_picker_previous(),
                Action::Next => app.filter_picker_next(),
                Action::Open | Action::Confirm => app.filter_picker_confirm(),
                Action::Back => app.filter_picker_cancel(),
                _ => {}
            }
            needs_draw = true;
            continue;
        }

        if app.source_pref_picker.is_some() {
            match action {
                Action::Quit => return Ok(()),
                Action::Previous => app.source_pref_picker_previous(),
                Action::Next => app.source_pref_picker_next(),
                Action::Open | Action::Confirm => app.source_pref_picker_confirm(),
                Action::Back => app.source_pref_picker_cancel(),
                _ => {}
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

        if app.library_search {
            match action {
                Action::Back => app.library_search_stop(true),
                Action::Open => app.library_search_stop(false),
                Action::Char(character) => app.library_search_push(character),
                Action::Backspace => app.library_search_pop(),
                Action::Previous => app.previous(),
                Action::Next => app.next(),
                _ => {}
            }
            needs_draw = true;
            continue;
        }

        if app.history_search {
            match action {
                Action::Back => app.history_search_stop(true),
                Action::Open => app.history_search_stop(false),
                Action::Char(character) => app.history_search_push(character),
                Action::Backspace => app.history_search_pop(),
                Action::Previous => app.history_previous(),
                Action::Next => app.history_next(),
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
                Action::NextChapter => {
                    if app.story_screen_active() {
                        app.continue_reading();
                    } else {
                        app.next_chapter();
                    }
                }
                Action::ToggleReaderMode => app.toggle_reader_mode(),
                Action::ZoomIn => app.zoom_in(),
                Action::ZoomOut => app.zoom_out(),
                Action::ToggleFullscreen => app.toggle_fullscreen(),
                Action::Click { column, row } => {
                    let (width, height) = crossterm::terminal::size()?;
                    let area = Rect::new(0, 0, width, height);
                    let layout =
                        app_layout_with_commands(area, app.fullscreen, commands_height(app, area));
                    handle_reader_click(layout.tabs, app, column, row);
                }
                _ => {}
            }
            needs_draw = true;
            continue;
        }

        match action {
            Action::Quit => app.quit_confirm = true,
            Action::Select => {
                if app.reader.is_some() {
                    app.next_screen();
                } else if app.tab == 3 && app.browse_view == BrowseView::SourceFilters {
                    app.toggle_source_filter_selection();
                } else {
                    app.toggle_multi_select();
                }
            }
            Action::Confirm => {}
            Action::Previous => {
                if app.tab == 3 {
                    if app.browse_view == BrowseView::GlobalResults {
                        app.global_previous_group();
                    } else {
                        app.browse_previous();
                    }
                } else if app.tab == 1 {
                    app.updates_previous();
                } else if app.tab == 4 {
                    if app.more_focus == MoreFocus::Sections {
                        app.previous_more_section();
                    } else if app.more_section == MoreSection::Categories {
                        app.category_manage_previous();
                    } else if app.more_section == MoreSection::Downloads {
                        app.downloads_previous();
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
                } else if app.tab == 1 {
                    app.updates_next();
                } else if app.tab == 4 {
                    if app.more_focus == MoreFocus::Sections {
                        app.next_more_section();
                    } else if app.more_section == MoreSection::Categories {
                        app.category_manage_next();
                    } else if app.more_section == MoreSection::Downloads {
                        app.downloads_next();
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
                        BrowseView::Filters => app.adjust_filter_selection(-1),
                        BrowseView::Sources
                        | BrowseView::SourceFilters
                        | BrowseView::SourcePrefs
                        | BrowseView::Extensions
                        | BrowseView::ExtensionDetails
                        | BrowseView::Stores => {}
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
                        BrowseView::Filters => app.adjust_filter_selection(1),
                        BrowseView::Sources
                        | BrowseView::SourceFilters
                        | BrowseView::SourcePrefs
                        | BrowseView::ExtensionDetails
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
            Action::NextChapter => {
                if app.tab == 3 && app.browse_view == BrowseView::RemoteResults {
                    app.load_next_remote_page();
                } else {
                    app.next_chapter();
                }
            }
            Action::ScrollUp => {
                if app.tab == 3 {
                    app.browse_previous();
                } else if app.tab == 1 {
                    app.updates_previous();
                } else if app.tab == 4 {
                    if app.more_focus == MoreFocus::Sections {
                        app.previous_more_section();
                    } else if app.more_section == MoreSection::Categories {
                        app.category_manage_previous();
                    } else if app.more_section == MoreSection::Downloads {
                        app.downloads_previous();
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
                } else if app.tab == 1 {
                    app.updates_next();
                } else if app.tab == 4 {
                    if app.more_focus == MoreFocus::Sections {
                        app.next_more_section();
                    } else if app.more_section == MoreSection::Categories {
                        app.category_manage_next();
                    } else if app.more_section == MoreSection::Downloads {
                        app.downloads_next();
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
                if app.tab == 2 {
                    app.history_section_previous();
                } else if app.tab == 3 {
                    app.previous_browse_tab();
                } else if app.tab == 4 {
                    app.previous_more_section();
                } else if app.tab == 0 && app.view == LibraryView::Series {
                    app.previous_category();
                }
            }
            Action::NextCategory => {
                if app.tab == 2 {
                    app.history_section_next();
                } else if app.tab == 3 {
                    app.next_browse_tab();
                } else if app.tab == 4 {
                    app.next_more_section();
                } else if app.tab == 0 && app.view == LibraryView::Series {
                    app.next_category();
                }
            }
            Action::SelectTab(index) => app.select_tab(index),
            Action::Refresh => {
                if app.tab == 4 && app.more_section == MoreSection::Storage {
                    app.refresh_storage();
                } else if app.tab == 4 && app.more_section == MoreSection::Downloads {
                    app.refresh_downloads();
                } else if app.tab == 3 {
                    app.refresh_browse();
                } else if app.tab == 0 && app.view == LibraryView::RemoteDetails {
                    app.reload_library_remote_chapters();
                } else {
                    app.refresh_async();
                }
            }
            Action::Search => {
                if app.story_screen_active() {
                    app.chapter_settings_adjust_sort(1);
                } else if app.tab == 3
                    && app.browse_view == BrowseView::Migrate
                    && app.migrate.screen == app::MigrateScreen::Matches
                {
                    app.migrate_toggle_skip();
                } else if app.tab == 0 && app.view == LibraryView::Series {
                    app.library_search_start();
                } else if app.tab == 2 {
                    app.history_search_start();
                } else if app.tab == 3 {
                    match app.browse_view {
                        BrowseView::RemoteResults => {
                            let query = app.remote.query.clone();
                            app.open_text_input(
                                app::TextAction::SearchRemote,
                                "Search source",
                                &query,
                            );
                        }
                        BrowseView::Local => {
                            let query = app.browse_local_query.clone();
                            app.open_text_input(
                                app::TextAction::SearchLocal,
                                "Search local source",
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
            Action::GlobalScope => {
                if app.tab == 3
                    && matches!(
                        app.browse_view,
                        BrowseView::Sources | BrowseView::GlobalResults
                    )
                {
                    app.toggle_global_search_scope();
                }
            }
            Action::HostToggle => {
                if app.tab == 4 && app.more_section == MoreSection::About {
                    app.toggle_host();
                }
            }
            Action::Favorite => {
                if app.tab == 2 {
                    app.favorite_selected_history();
                } else if app.tab == 3 {
                    match app.browse_view {
                        BrowseView::RemoteDetails => app.toggle_current_favorite(),
                        BrowseView::RemoteResults | BrowseView::GlobalResults => {
                            app.toggle_selected_favorites();
                        }
                        _ => {}
                    }
                }
            }
            Action::Latest => {
                if app.tab == 3 {
                    match app.browse_view {
                        BrowseView::RemoteResults => app.load_remote_latest(),
                        BrowseView::Local => app.toggle_local_latest(),
                        _ => {}
                    }
                }
            }
            Action::DisplayMode => {
                if app.story_screen_active() {
                    app.cycle_chapter_display();
                } else if app.tab == 3 {
                    match app.browse_view {
                        BrowseView::Local => app.toggle_local_order(),
                        BrowseView::RemoteResults => app.cycle_source_display_mode(),
                        _ => {}
                    }
                }
            }
            Action::HideSaved => {
                if app.tab == 3 {
                    match app.browse_view {
                        BrowseView::RemoteResults => app.toggle_hide_saved_results(),
                        BrowseView::GlobalResults => app.toggle_global_search_only_results(),
                        _ => {}
                    }
                }
            }
            Action::Extensions => {
                if app.tab == 3 {
                    app.set_browse_tab(BrowseTab::Extensions);
                }
            }
            Action::SourceFilters => {
                if app.story_screen_active() {
                    app.open_story_source();
                } else if app.tab == 3
                    && app.browse_view == BrowseView::Migrate
                    && app.migrate.screen == app::MigrateScreen::Matches
                {
                    app.open_migration_search();
                } else if app.tab == 3 && app.browse_view == BrowseView::GlobalResults {
                    app.global_open_group();
                } else if app.tab == 3 && app.browse_view == BrowseView::Stores {
                    app.open_store_url();
                } else if app.tab == 2 {
                    app.open_history_details();
                } else if app.tab == 3 {
                    if app.browse_view == BrowseView::SourceFilters {
                        app.browse_parent();
                    } else if app.browse_view == BrowseView::Sources {
                        app.open_source_filters();
                    } else if app.browse_view == BrowseView::Extensions {
                        app.open_extension_details();
                    }
                }
            }
            Action::CopyTags => {
                if app.story_screen_active() {
                    app.copy_story_tags();
                }
            }
            Action::ExportMetadata => {
                if app.story_screen_active() {
                    app.export_story_metadata();
                }
            }
            Action::SourcePrefs => {
                if app.story_screen_active() {
                    app.copy_story_reference();
                } else if app.tab == 3 && app.browse_view == BrowseView::Stores {
                    app.copy_store_url();
                } else if app.tab == 3 {
                    match app.browse_view {
                        BrowseView::Sources => app.open_source_prefs(),
                        BrowseView::ExtensionDetails => app.open_source_prefs_for_selected(),
                        BrowseView::Migrate
                            if app.migrate.screen == app::MigrateScreen::Matches =>
                        {
                            app.migrate_apply(false)
                        }
                        _ => {}
                    }
                }
            }
            Action::Filters => {
                if app.tab == 3 && app.browse_view == BrowseView::RemoteResults {
                    app.open_filters();
                }
            }
            Action::Stores => {
                if app.story_screen_active() {
                    app.open_tag_picker();
                } else if app.tab == 3 && app.browse_view == BrowseView::Migrate {
                    app.open_migration_sources();
                } else if app.tab == 3 {
                    if app.browse_view == BrowseView::Stores {
                        app.open_extensions();
                    } else {
                        app.set_browse_tab(BrowseTab::Extensions);
                        app.open_stores();
                    }
                }
            }
            Action::AddStore => {
                if app.story_screen_active() {
                    app.select_all_story_chapters();
                } else if app.tab == 4 && app.more_section == MoreSection::Categories {
                    app.open_add_category();
                } else {
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
            }
            Action::DeleteStore => {
                if app.tab == 3 && app.browse_view == BrowseView::Stores {
                    app.remove_selected_store();
                } else if app.tab == 4 && app.more_section == MoreSection::Storage {
                    app.open_clear_downloads_confirm();
                } else if app.tab == 4 && app.more_section == MoreSection::Downloads {
                    app.open_downloads_delete_confirm();
                }
            }
            Action::Download => {
                if app.story_screen_active() {
                    app.open_download_menu();
                } else if app.has_downloadable_chapter() {
                    app.download_selected_chapter();
                }
            }
            Action::Bookmark => app.toggle_bookmark(),
            Action::RefreshCovers => {
                if app.tab == 0 && app.view == LibraryView::Series {
                    app.refresh_library_covers();
                }
            }
            Action::UpdateExtension => {
                if app.story_screen_active() {
                    app.mark_selected_read(false);
                } else if app.tab == 3 && app.browse_view == BrowseView::Extensions {
                    app.update_selected_extension();
                }
            }
            Action::UpdateAllExtensions => {
                if app.story_screen_active() {
                    app.mark_previous_read();
                } else if app.tab == 3 && app.browse_view == BrowseView::Extensions {
                    app.update_all_extensions();
                }
            }
            Action::InvertSelection => {
                if app.story_screen_active() {
                    app.invert_story_selection();
                }
            }
            Action::Migrate => {
                if app.story_screen_active() {
                    app.migrate_this_story();
                } else if app.tab == 3
                    && matches!(
                        app.browse_view,
                        BrowseView::RemoteResults | BrowseView::GlobalResults
                    )
                {
                    app.migrate_selected_result();
                } else if app.tab == 3 && app.browse_view == BrowseView::Migrate {
                    match app.migrate.screen {
                        app::MigrateScreen::Favorites => app.migrate_start_matching(),
                        app::MigrateScreen::Matches => app.migrate_apply(true),
                        app::MigrateScreen::Sources => {}
                    }
                }
            }
            Action::ClearHistory => {
                if app.tab == 1 {
                    app.clear_updates();
                } else if app.tab == 2 {
                    app.open_history_clear_confirm();
                }
            }
            Action::InstallExtension => {
                if app.tab == 3 && app.browse_view == BrowseView::Extensions {
                    app.install_selected_extension();
                }
            }
            Action::UninstallExtension => {
                if app.story_screen_active() {
                    app.open_download_delete_confirm();
                } else if app.tab == 1 {
                    app.dismiss_update();
                } else if app.tab == 3 && app.browse_view == BrowseView::Extensions {
                    app.uninstall_selected_extension();
                } else if app.tab == 0 && app.view == LibraryView::Series {
                    app.open_library_delete_dialog();
                } else if app.tab == 2 {
                    app.open_history_delete_dialog();
                } else if app.tab == 4 && app.more_section == MoreSection::Categories {
                    app.delete_manage_category();
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
                    } else if app.more_section == MoreSection::Categories {
                        app.open_rename_category();
                    } else {
                        app.activate_setting();
                    }
                } else if app.tab == 0 {
                    app.open();
                } else if app.tab == 1 {
                    app.open_update_entry();
                } else if app.tab == 2 {
                    app.open_history_entry();
                }
            }
            Action::Back => {
                if app.cover_overlay {
                    app.cover_overlay = false;
                } else if app.tab == 3 {
                    app.browse_parent();
                } else if app.tab == 4 {
                    app.leave_more();
                } else if app.tab == 0 {
                    app.back();
                } else if app.tab == 1 || app.tab == 2 {
                    app.select_tab(0);
                }
            }
            Action::ToggleFullscreen => {
                if app.story_screen_active() {
                    app.open_chapter_settings();
                } else {
                    app.toggle_fullscreen();
                }
            }
            Action::ToggleReaderMode => {
                if app.story_screen_active() {
                    app.mark_selected_read(true);
                } else {
                    app.toggle_reader_mode();
                }
            }
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
            Action::OpenConfig => {
                if app.story_screen_active() {
                    app.open_category_picker();
                } else if app.tab == 3
                    && matches!(
                        app.browse_view,
                        BrowseView::Extensions | BrowseView::ExtensionDetails
                    )
                {
                    app.cancel_extension_jobs();
                } else {
                    app.select_tab(4);
                }
            }
            Action::StoryNotes => {
                if app.story_screen_active() {
                    app.edit_story_notes();
                }
            }
            Action::ToggleCover => {
                if app.story_screen_active() {
                    app.toggle_cover_overlay();
                }
            }
            Action::CustomCover => {
                if app.story_screen_active() {
                    app.edit_custom_cover();
                }
            }
            Action::OpenSourceHome => {
                if app.tab == 3
                    && matches!(
                        app.browse_view,
                        BrowseView::Sources | BrowseView::ExtensionDetails
                    )
                {
                    app.open_selected_source_home();
                }
            }
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
    let layout = app_layout_with_commands(area, app.fullscreen, commands_height(app, area));

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

    if app.tab == 2 {
        handle_history_click(layout.content, app, column, row);
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
        let mut grid_area = library_grid_area(layout.content);
        if app.library_filter_active() && grid_area.height > 1 {
            grid_area = Rect::new(
                grid_area.x,
                grid_area.y + 1,
                grid_area.width,
                grid_area.height - 1,
            );
        }
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

fn handle_history_click(area: Rect, app: &mut AppState, column: u16, row: u16) {
    if app.history_section != crate::app::HistorySection::History {
        return;
    }
    let inner = Rect::new(
        area.x + 1,
        area.y + 1,
        area.width.saturating_sub(2),
        area.height.saturating_sub(2),
    );
    let body = if app.status.is_some() && inner.height > 0 {
        Rect::new(inner.x, inner.y, inner.width, inner.height - 1)
    } else {
        inner
    };
    let list_area = Rect::new(
        body.x,
        body.y + 1,
        body.width,
        body.height.saturating_sub(1),
    );
    if !contains(list_area, column, row) {
        return;
    }

    let entries = app.history_entries();
    if entries.is_empty() {
        return;
    }
    let row_height = 4u16;
    let visible = (list_area.height / row_height).max(1) as usize;
    if app.history_selection < app.history_scroll {
        app.history_scroll = app.history_selection;
    } else if app.history_selection >= app.history_scroll + visible {
        app.history_scroll = app.history_selection + 1 - visible;
    }
    let mut y = list_area.y;
    let mut previous_day: Option<String> = None;
    for (index, (_, entry)) in entries.iter().enumerate().skip(app.history_scroll) {
        if y + 3 > list_area.y + list_area.height {
            break;
        }
        let day = tsuiyomi_core::date_key_for_unix(entry.last_read);
        if previous_day.as_deref() != Some(day.as_str()) {
            if y + 4 > list_area.y + list_area.height {
                break;
            }
            y += 1;
            previous_day = Some(day);
        }
        if row >= y && row < y + 3 {
            app.history_selection = index;
            app.history_list_state.select(Some(index));
            return;
        }
        y += row_height;
    }
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
        BrowseView::SourceFilters => {
            let list = inside(2);
            if contains(list, column, row) {
                let offset = app.source_filter_list_state.offset();
                app.select_source_filter_row(offset + (row - list.y) as usize);
            }
        }
        BrowseView::SourcePrefs => {
            let list = inside(2);
            if contains(list, column, row) {
                let offset = app
                    .source_prefs
                    .as_ref()
                    .map_or(0, |state| state.list_state.offset());
                app.select_browse_index(offset + (row - list.y) as usize);
            }
        }
        BrowseView::Filters => {
            let list = inside(2);
            if contains(list, column, row) {
                let offset = app.remote.filter_list_state.offset();
                app.select_filter_row(offset + (row - list.y) as usize);
            }
        }
        BrowseView::ExtensionDetails => {
            let list = inside(8);
            if contains(list, column, row) {
                let offset = app
                    .extension_details
                    .as_ref()
                    .map_or(0, |state| state.list_state.offset());
                app.select_extension_detail_row(offset + (row - list.y) as usize);
            }
        }
        BrowseView::Extensions => {
            let list = inside(3);
            if contains(list, column, row) {
                let offset = app.browse_list_state.offset();
                app.select_extension_row(offset + (row - list.y) as usize);
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
            if app.browse_view == BrowseView::RemoteResults
                && app.config.source_display_mode == tsuiyomi_core::SourceDisplayMode::List
            {
                let list = library_grid_area(area);
                if contains(list, column, row) {
                    let offset = app.remote.remote_list_state.offset();
                    app.select_browse_index(offset + (row - list.y) as usize);
                }
                return;
            }
            let grid_area = library_grid_area(area);
            let text_height = if app.browse_view == BrowseView::RemoteResults
                && app.config.source_display_mode == tsuiyomi_core::SourceDisplayMode::Compact
            {
                1
            } else {
                TILE_TEXT_HEIGHT
            };
            if let Some(index) = grid_index_at(
                grid_area,
                app.browse_columns.max(1),
                app.browse_grid_offset,
                column,
                row,
                app.browse_item_count(),
                tile_cover_height(app.config.thumbnail_width, grid_area.height) + text_height,
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
        BrowseView::Migrate => {
            let list = inside(2);
            if contains(list, column, row) {
                let offset = app.migrate.list_state.offset();
                app.select_browse_index(offset + (row - list.y) as usize);
            }
        }
        BrowseView::GlobalResults => {}
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
