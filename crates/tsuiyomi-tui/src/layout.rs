use ratatui::layout::{Constraint, Direction, Layout, Rect};

pub const TAB_COUNT: usize = 5;
pub const TAB_NAMES: [&str; TAB_COUNT] = ["Library", "Updates", "History", "Browse", "More"];

#[derive(Debug, Clone, Copy)]
pub struct AppLayout {
    /// Hidden in fullscreen mode.
    pub header: Option<Rect>,
    pub content: Rect,
    pub tabs: Rect,
    /// Hidden in fullscreen mode.
    pub commands: Option<Rect>,
}

pub fn app_layout(area: Rect, fullscreen: bool) -> AppLayout {
    if fullscreen {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(3)])
            .split(area);
        return AppLayout {
            header: None,
            content: chunks[0],
            tabs: chunks[1],
            commands: None,
        };
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(3),
            Constraint::Length(3),
        ])
        .split(area);

    AppLayout {
        header: Some(chunks[0]),
        content: chunks[1],
        tabs: chunks[2],
        commands: Some(chunks[3]),
    }
}

#[derive(Debug, Clone, Copy)]
pub struct TabLayout {
    pub index: usize,
    pub area: Rect,
}

/// Equal-width tab segments inside the tab bar, with columns reserved for separators.
pub fn tab_layouts(area: Rect) -> Vec<TabLayout> {
    let inner = inner(area);
    if inner.width == 0 || inner.height == 0 {
        return Vec::new();
    }

    let separators = TAB_COUNT as u16 - 1;
    let usable = inner.width.saturating_sub(separators);
    let base = usable / TAB_COUNT as u16;
    let extra = usable % TAB_COUNT as u16;

    let mut layouts = Vec::with_capacity(TAB_COUNT);
    let mut x = inner.x;
    for index in 0..TAB_COUNT {
        let width = base + u16::from((index as u16) < extra);
        layouts.push(TabLayout {
            index,
            area: Rect::new(x, inner.y, width, 1),
        });
        x += width + u16::from(index + 1 < TAB_COUNT);
    }

    layouts
}

#[derive(Debug, Clone, Copy)]
pub struct LibraryLayout {
    pub categories: Rect,
    pub list: Rect,
    pub details: Rect,
}

pub fn library_layout(area: Rect) -> LibraryLayout {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(area);

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(50),
            Constraint::Length(1),
            Constraint::Percentage(50),
        ])
        .split(rows[1]);

    LibraryLayout {
        categories: rows[0],
        list: chunks[0],
        details: chunks[2],
    }
}

/// Full-width area below the category row for the library tile grid.
pub fn library_grid_area(area: Rect) -> Rect {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(area);
    rows[1]
}

pub const TILE_GAP: u16 = 1;
pub const TILE_TEXT_HEIGHT: u16 = 2;

/// Cover height follows the configured tile width, capped so the viewport can show
/// two complete rows on short terminals.
pub fn tile_cover_height(min_width: u16, area_height: u16) -> u16 {
    let ideal = (u32::from(min_width) * 2 / 3).clamp(5, 16) as u16;
    let cap = area_height
        .saturating_div(2)
        .saturating_sub(TILE_TEXT_HEIGHT)
        .max(4);
    ideal.min(cap)
}

pub fn grid_columns(area: Rect, min_width: u16) -> usize {
    let width = area.width.max(1);
    let min_width = min_width.max(4);
    ((width + TILE_GAP) / (min_width + TILE_GAP)).max(1) as usize
}

pub fn grid_tile_width(area: Rect, columns: usize) -> u16 {
    let columns = columns.max(1) as u16;
    let gaps = TILE_GAP.saturating_mul(columns - 1);
    area.width.saturating_sub(gaps) / columns
}

/// Left margin that centers the whole grid when the tile width does not divide the area.
pub fn grid_margin(area: Rect, columns: usize) -> u16 {
    let count = columns.max(1) as u16;
    let used = grid_tile_width(area, columns)
        .saturating_mul(count)
        .saturating_add(TILE_GAP.saturating_mul(count.saturating_sub(1)));
    area.width.saturating_sub(used) / 2
}

pub fn grid_visible_rows(area: Rect, tile_height: u16) -> usize {
    (area.height / tile_height.max(1)).max(1) as usize
}

/// Keep the selected row visible, scrolling by whole tile rows.
pub fn grid_row_offset(
    area: Rect,
    columns: usize,
    selected: usize,
    current: usize,
    count: usize,
    tile_height: u16,
) -> usize {
    let columns = columns.max(1);
    let visible = grid_visible_rows(area, tile_height);
    let selected_row = selected / columns;
    let mut offset = current;
    if selected_row < offset {
        offset = selected_row;
    }
    if selected_row >= offset.saturating_add(visible) {
        offset = selected_row + 1 - visible;
    }
    let total_rows = count.div_ceil(columns);
    offset.min(total_rows.saturating_sub(visible))
}

pub fn grid_tile_rect(
    area: Rect,
    columns: usize,
    offset_row: usize,
    index: usize,
    tile_height: u16,
) -> Rect {
    let columns = columns.max(1);
    let tile_width = grid_tile_width(area, columns);
    let stride = tile_width.saturating_add(TILE_GAP);
    let row = index / columns;
    let column = index % columns;
    Rect::new(
        area.x + grid_margin(area, columns) + column as u16 * stride,
        area.y + (row.saturating_sub(offset_row)) as u16 * tile_height,
        tile_width,
        tile_height,
    )
}

pub fn grid_index_at(
    area: Rect,
    columns: usize,
    offset_row: usize,
    column: u16,
    row: u16,
    count: usize,
    tile_height: u16,
) -> Option<usize> {
    if !contains(area, column, row) {
        return None;
    }
    let columns = columns.max(1);
    let tile_width = grid_tile_width(area, columns);
    let stride = tile_width.saturating_add(TILE_GAP);
    let left = area.x + grid_margin(area, columns);
    if column < left {
        return None;
    }
    let relative_x = column - left;
    let relative_y = row - area.y;
    let grid_column = relative_x / stride;
    if grid_column as usize >= columns || relative_x % stride >= tile_width {
        return None;
    }
    let grid_row = relative_y as usize / tile_height.max(1) as usize;
    if grid_row >= grid_visible_rows(area, tile_height) {
        return None;
    }
    let index = (offset_row + grid_row) * columns + grid_column as usize;
    (index < count).then_some(index)
}

#[derive(Debug, Clone, Copy)]
pub struct MoreLayout {
    pub sections: Rect,
    pub content: Rect,
}

pub fn more_layout(area: Rect) -> MoreLayout {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(28),
            Constraint::Length(1),
            Constraint::Min(0),
        ])
        .split(area);

    MoreLayout {
        sections: chunks[0],
        content: chunks[2],
    }
}

/// One-row areas for settings inside a bordered panel.
pub fn settings_rows(area: Rect, count: usize) -> Vec<Rect> {
    let inner = inner(area);
    let mut rows = Vec::with_capacity(count);
    for index in 0..count {
        let y = inner.y.saturating_add(index as u16);
        if y >= inner.y.saturating_add(inner.height) {
            break;
        }
        rows.push(Rect::new(inner.x, y, inner.width, 1));
    }
    rows
}

pub const SETTING_LABEL_WIDTH: u16 = 20;
pub const SETTING_VALUE_WIDTH: u16 = 18;

/// Shared label/slider/value columns for rendering and mouse hit-testing.
pub fn setting_columns(row: Rect) -> Vec<Rect> {
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(SETTING_LABEL_WIDTH),
            Constraint::Min(10),
            Constraint::Length(SETTING_VALUE_WIDTH),
        ])
        .split(row)
        .to_vec()
}

#[derive(Debug, Clone, Copy)]
pub struct ReaderBarLayout {
    pub previous: Rect,
    pub progress: Rect,
    pub next: Rect,
    pub mode: Rect,
}

/// Controls replacing the tab bar while the reader is open.
pub fn reader_bar_layout(area: Rect) -> ReaderBarLayout {
    let inner = inner(area);
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(14),
            Constraint::Min(10),
            Constraint::Length(14),
            Constraint::Length(12),
        ])
        .split(inner);

    ReaderBarLayout {
        previous: chunks[0],
        progress: chunks[1],
        next: chunks[2],
        mode: chunks[3],
    }
}

/// Area inside a one-cell border.
pub fn inner(area: Rect) -> Rect {
    Rect {
        x: area.x.saturating_add(1),
        y: area.y.saturating_add(1),
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    }
}

pub fn contains(area: Rect, column: u16, row: u16) -> bool {
    column >= area.x
        && column < area.x.saturating_add(area.width)
        && row >= area.y
        && row < area.y.saturating_add(area.height)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_geometry_matches_rendered_tiles() {
        let area = Rect::new(0, 0, 78, 14);
        let columns = grid_columns(area, 14);
        assert_eq!(columns, 5);
        assert_eq!(grid_tile_width(area, columns), 14);
        assert_eq!(
            grid_visible_rows(area, tile_cover_height(14, area.height) + TILE_TEXT_HEIGHT),
            2
        );

        let columns = grid_columns(area, 18);
        assert_eq!(columns, 4);
        assert_eq!(grid_tile_width(area, columns), 18);
    }

    #[test]
    fn grid_centers_when_the_tile_width_does_not_divide_the_area() {
        // 160 / 9 rounds to 16, which used to leave eight columns of dead space.
        let area = Rect::new(0, 0, 160, 40);
        let columns = grid_columns(area, 16);
        assert_eq!(columns, 9);
        let width = grid_tile_width(area, columns);
        assert_eq!(width, 16);
        let margin = grid_margin(area, columns);
        assert_eq!(margin, 4);
        let last = grid_tile_rect(area, columns, 0, columns - 1, 12);
        assert_eq!(last.x + last.width + margin, area.width);
    }

    #[test]
    fn grid_hit_testing_ignores_gaps_and_partial_rows() {
        let area = Rect::new(4, 3, 78, 16);
        let tile_height = tile_cover_height(14, area.height) + TILE_TEXT_HEIGHT;
        let left = area.x + grid_margin(area, 5);
        assert_eq!(left, 6);
        assert_eq!(grid_index_at(area, 5, 0, left, 3, 20, tile_height), Some(0));
        assert_eq!(
            grid_index_at(area, 5, 0, left + 15, 3, 20, tile_height),
            Some(1)
        );
        assert_eq!(
            grid_index_at(area, 5, 0, left + 14, 3, 20, tile_height),
            None
        );
        assert_eq!(grid_index_at(area, 5, 0, area.x, 3, 20, tile_height), None);
        assert_eq!(
            grid_index_at(area, 5, 0, left, 13, 20, tile_height),
            Some(5)
        );
        assert_eq!(
            grid_index_at(area, 5, 0, left, 14, 20, tile_height),
            Some(5)
        );
        assert_eq!(grid_index_at(area, 5, 0, left, 19, 20, tile_height), None);
    }
}
