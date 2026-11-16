use ratatui::{
    layout::Rect,
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

pub fn render_panel(frame: &mut Frame, area: Rect) {
    let paragraph = Paragraph::new("Config is not implemented yet.")
        .block(Block::default().title("Config").borders(Borders::ALL))
        .wrap(Wrap { trim: true });
    frame.render_widget(paragraph, area);
}
