extern crate core;
extern crate git2;
extern crate itertools;
extern crate termion;
extern crate tui;

use core::cmp;
use git2::Branch;
use git2::Branches;
use git2::BranchType;
use git2::build::CheckoutBuilder;
use git2::Odb;
use git2::Reference;
use git2::References;
use git2::Repository;
use git2::Tree;
use itertools::Itertools;
use std::convert::AsRef;
use std::io;
use std::io::stdin;
use std::sync::mpsc;
use std::sync::mpsc::Receiver;
use std::sync::mpsc::Sender;
use std::thread;
use termion::event;
use termion::input::TermRead;
use tui::backend::MouseBackend;
use tui::backend::RawBackend;
use tui::layout::Direction;
use tui::layout::Group;
use tui::layout::Rect;
use tui::layout::Size;
use tui::style::Color;
use tui::style::Modifier;
use tui::style::Style;
use tui::Terminal;
use tui::widgets::{Block, Borders, SelectableList, Widget};
use tui::widgets::Gauge;
use tui::widgets::Paragraph;

enum MoveDirection {
    UP,
    DOWN,
    LEFT,
    RIGHT,
}

enum Message {
    MOVE {
        direction: MoveDirection
    },
    SHUTDOWN,
    ENTER,
    CHECKOUT {
        selected: usize
    },
    PROGRESS {
        p: u16
    },
}

struct Envelope {
    s: Message
}

unsafe impl Send for Message {}

enum AppState {
    LISTING,
    CHECKOUT,
}

struct App {
    position: usize,
    progress: u16,
    state: AppState,
}

impl Default for App {
    fn default() -> Self {
        App {
            position: 0,
            progress: 0,
            state: AppState::LISTING,
        }
    }
}

fn main() {
    let backend = MouseBackend::new().expect("Can get a backend io device");
    let mut terminal = Terminal::new(backend).expect("Unable to get a terminal");

    terminal.hide_cursor();
    terminal.clear().unwrap();

    let mut app = App::default();

    let repo = Repository::discover("/Users/pault/src/canva/web").expect("Not a git repository");
    let branches = get_local_branches(&repo);

    let (tx, rx) = mpsc::channel::<Envelope>();
    let tx2 = tx.clone();

    // Event thread
    thread::spawn(move || {
        for c in io::stdin().keys() {
            let evt = c.unwrap();
            match evt {
                // Directions
                event::Key::Char('j') => { tx.send(Envelope { s: Message::MOVE { direction: MoveDirection::DOWN } }); }
                event::Key::Down => { tx.send(Envelope { s: Message::MOVE { direction: MoveDirection::DOWN } }); }
                event::Key::Char('k') => { tx.send(Envelope { s: Message::MOVE { direction: MoveDirection::UP } }); }
                event::Key::Up => { tx.send(Envelope { s: Message::MOVE { direction: MoveDirection::UP } }); }

                // Actions
                event::Key::Char('\n') => { tx.send(Envelope { s: Message::ENTER }); }
                event::Key::Char('q') => {
                    tx.send(Envelope { s: Message::SHUTDOWN });
                    return;
                }
                _ => ()
            };
        }
    });
    let (io_tx, io_rx) = mpsc::channel::<Envelope>();

    // IO Thread
    thread::spawn(move || {
//        let repo = Repository::discover("/Users/pault/src/canva/web").expect("Not a git repository");
        loop {
            match io_rx.recv() {
                Ok(t) => {
                    match t {
                        Envelope { s: Message::CHECKOUT { selected: r } } => {
                            let branches = get_local_branches(&repo);
                            let gitref = &branches[r];
                            switch_to(&repo, gitref.to_string(), |perc| {
                                tx2.send(Envelope {
                                    s: Message::PROGRESS { p: perc }
                                });
                            })
                        }
                        _ => ()
                    }
                }
                Err(e) => ()
            }
        }
    });

    // First Draw
    draw(&mut terminal, &branches, &app);

    // Reactor thread
    loop {
        let size = terminal.size().unwrap();
        match rx.recv() {
            Ok(t) => {
                match t {
                    Envelope { s: Message::MOVE { direction: MoveDirection::DOWN } } => {
                        app.position = if app.position < branches.len() - 1 { app.position + 1 } else { app.position }
                    }
                    Envelope { s: Message::MOVE { direction: MoveDirection::UP } } => {
                        app.position = if app.position == 0 { 0 } else { app.position - 1 }
                    }
                    Envelope { s: Message::ENTER } => {
                        app.state = AppState::CHECKOUT;
                        io_tx.send(Envelope { s: Message::CHECKOUT { selected: app.position } });
                    }
                    Envelope { s: Message::PROGRESS { p } } => {
                        app.progress = p;
                        if app.progress >= 100 {
                            app.state = AppState::LISTING;
                            app.progress = 0;
                        }
                    }
                    Envelope { s: Message::SHUTDOWN } => {
                        terminal.clear().unwrap();
                        return;
                    }
                    _ => ()
                }
            }
            Err(_) => break
        }
        draw(&mut terminal, &branches, &app);
    }
}

fn get_local_branches(repo: &Repository) -> Vec<String> {
    let refs = repo.references().unwrap();
    let branches: Vec<Reference> = refs.flat_map(|f| {
        match f {
            Ok(t) => {
                if t.is_remote() || !t.is_branch() {
                    Option::None
                } else {
                    Option::Some(t)
                }
            }
            Err(_) => Option::None
        }
    }).collect::<Vec<_>>();

    let sorted: Vec<&Reference> = branches
        .iter()
        .sorted_by(|i, b| {
            i.peel_to_commit().unwrap().time().cmp(&b.peel_to_commit().unwrap().time()).reverse()
        });

    let s = sorted.iter()
        .map(|t| {
            String::from(t.shorthand().unwrap())
        })
        .collect();
    return s;
}

fn switch_to<F: Fn(u16) -> ()>(repo: &Repository, refs: String, mut f: F) {
    let rev = repo.revparse_ext(&refs).unwrap().1.unwrap();
    let tree: Tree = rev.peel_to_tree().unwrap().clone();
    let mut cob = CheckoutBuilder::new();
    let mut co = cob
        .progress(|a, b, c| {
            let up = b as u16;
            let to = c as u16;
            f((((up as f32) / to as f32) * 100.0) as u16)
        });

    let x = repo.checkout_tree(&tree.as_object(), Option::Some(&mut co));
    repo.set_head(rev.name().unwrap().clone());
    match x {
        Ok(_) => (),
        Err(e) => println!("{}", e)
    }
}

fn draw(terminal: &mut Terminal<MouseBackend>, b: &Vec<String>, a: &App) -> Result<(), io::Error> {
    let size = terminal.size()?;

    let style = Style::default().fg(Color::White).bg(Color::Black);
    match a.state {
        AppState::LISTING => {
            Group::default()
                .direction(Direction::Horizontal)
                .sizes(&[Size::Percent(70), Size::Percent(30)])
                .render(terminal, &size, |t, chunks| {
                    let block = Block::default()
                        .title("Working branches")
                        .borders(Borders::ALL);

                    SelectableList::default()
                        .select(a.position)
                        .block(block)
                        .style(style)
                        .highlight_style(style.clone().fg(Color::Black).bg(Color::White).modifier(Modifier::Bold))
                        .items(&b[..])
                        .render(t, &chunks[0]);

                    Block::default()
                        .borders(Borders::ALL)
                        .title("Branch details")
                        .render(t, &chunks[1]);
                });
        }
        AppState::CHECKOUT => {
            let progress = &Rect {
                x: 2,
                y: size.height / 2 - 2,
                width: size.width - 4,
                height: 3,
            };
            Block::default()
                .title("Checking out")
                .borders(Borders::ALL)
                .style(style.clone().fg(Color::Black).bg(Color::Black))
                .render(terminal, progress);
            Group::default()
                .direction(Direction::Vertical)
                .sizes(&[Size::Percent(100)])
                .margin(1)
                .render(terminal, progress, |t, chunks| {
                    Gauge::default()
                        .style(style.clone().fg(Color::White))
                        .percent(a.progress)
                        .render(t, &chunks[0])
                });
        }
    }

    terminal.draw()
}
