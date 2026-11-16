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
