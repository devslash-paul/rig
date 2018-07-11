extern crate tui;

use tui::layout::Group;
use tui::layout::Direction;
use tui::layout::Size;
use tui::widgets::Block;
use tui::widgets::SelectableList;
use tui::style::Color;
use tui::style::Modifier;
use tui::backend::MouseBackend;
use tui::Terminal;
use tui::style::Style;
use tui::widgets::Borders;
use tui::widgets::Widget;
use tui::layout::Rect;
use tui::widgets::Gauge;

static STYLE: Style = Style {
    fg: Color::White,
    bg: Color::Black,
    modifier: Modifier::Reset,
};

pub fn draw_listing(terminal: &mut Terminal<MouseBackend>, b: &[String], pos: usize) {
    let size = terminal.size().unwrap();

    Group::default()
        .direction(Direction::Horizontal)
        .sizes(&[Size::Percent(70), Size::Percent(30)])
        .render(terminal, &size, |t, chunks| {
            let block = Block::default()
                .title("Working branches")
                .borders(Borders::ALL);

            SelectableList::default()
                .select(pos)
                .block(block)
                .style(STYLE)
                .highlight_style(STYLE.fg(Color::Black).bg(Color::White))
                .items(&b[..])
                .render(t, &chunks[0]);

            Block::default()
                .borders(Borders::ALL)
                .title("Branch details")
                .render(t, &chunks[1]);
        });


}

pub fn draw_checkout(terminal: &mut Terminal<MouseBackend>, progress: u16, path: &str) {
    let size = terminal.size().unwrap();
    let progress_rect = &Rect {
        x: 2,
        y: size.height / 2 - 2,
        width: size.width - 4,
        height: 3,
    };
    Block::default()
        .title("Checking out")
        .borders(Borders::ALL)
        .style(STYLE.fg(Color::Black).bg(Color::Black))
        .render(terminal, progress_rect);
    Group::default()
        .direction(Direction::Vertical)
        .sizes(&[Size::Percent(100)])
        .margin(1)
        .render(terminal, progress_rect, |t, chunks| {
            Gauge::default()
                .style(STYLE.fg(Color::White))
                .percent(progress)
                .label(&path)
                .render(t, &chunks[0])
        });
}
