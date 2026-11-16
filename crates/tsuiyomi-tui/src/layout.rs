use ratatui::layout::{Constraint, Direction, Layout, Rect};

pub const TAB_COUNT: usize = 4;
pub const TAB_NAMES: [&str; TAB_COUNT] = ["Library", "Search", "Updates", "Config"];

#[derive(Debug, Clone, Copy)]
pub struct AppLayout {
    pub tabs: Rect,
    pub content: Rect,
    pub footer: Rect,
}

pub fn app_layout(area: Rect) -> AppLayout {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(area);

    AppLayout {
        tabs: chunks[0],
        content: chunks[1],
        footer: chunks[2],
    }
}

#[derive(Debug, Clone)]
pub struct TabLayout {
    pub index: usize,
    pub label: String,
    pub area: Rect,
}

/// Geometry of the tab labels. Rendering and mouse hit-testing both use this.
pub fn tab_layouts(area: Rect) -> Vec<TabLayout> {
    let mut layouts = Vec::with_capacity(TAB_COUNT);
    let mut x = area.x;

    for (index, name) in TAB_NAMES.iter().enumerate() {
        let label = format!(" {} {} ", name, index + 1);
        let width = label.chars().count() as u16;
        if x + width > area.x + area.width {
            break;
        }
        layouts.push(TabLayout {
            index,
            label,
            area: Rect::new(x, area.y, width, 1),
        });
        x += width + 1;
    }

    layouts
}

#[derive(Debug, Clone, Copy)]
pub struct LibraryLayout {
    pub list: Rect,
    pub details: Rect,
}

pub fn library_layout(area: Rect) -> LibraryLayout {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(50),
            Constraint::Length(1),
            Constraint::Percentage(50),
        ])
        .split(area);

    LibraryLayout {
        list: chunks[0],
        details: chunks[2],
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
