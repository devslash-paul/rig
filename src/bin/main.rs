#![deny(clippy_pedantic)]
#![allow(cast_possible_truncation)]
extern crate core;
extern crate git2;
extern crate itertools;
extern crate linker;
extern crate termion;
extern crate tui;

use git2::build::CheckoutBuilder;
use git2::Reference;
use git2::Repository;
use git2::Tree;
use itertools::Itertools;
use std::io;
use std::sync::mpsc;
use std::thread;
use termion::event;
use termion::input::TermRead;
use tui::backend::MouseBackend;
use tui::Terminal;

enum MoveDirection {
    UP,
    DOWN,
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
        p: u16,
        path: String
    },
}

struct Envelope {
    s: Message
}

unsafe impl Send for Message{}

enum AppState {
    LISTING,
    CHECKOUT,
}

struct App {
    position: usize,
    progress: u16,
    state: AppState,
    path: String,
}

impl Default for App {
    fn default() -> Self {
        Self {
            position: 0,
            progress: 0,
            state: AppState::LISTING,
            path: String::from(""),
        }
    }
}

fn main() {
    let backend = MouseBackend::new().expect("Can get a backend io device");
    let mut terminal = Terminal::new(backend).expect("Unable to get a terminal");

    terminal.hide_cursor().unwrap();
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
                event::Key::Char('j') | event::Key::Down => { let _ = tx.send(Envelope { s: Message::MOVE { direction: MoveDirection::DOWN } }); }
                event::Key::Char('k') | event::Key::Up => { let _ = tx.send(Envelope { s: Message::MOVE { direction: MoveDirection::UP } }); }

                // Actions
                event::Key::Char('\n') => { let _ = tx.send(Envelope { s: Message::ENTER }); }
                event::Key::Char('q') => {
                    let _ = tx.send(Envelope { s: Message::SHUTDOWN });
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
            if let Ok(t) = io_rx.recv() {
                match t {
                    Envelope { s: Message::CHECKOUT { selected: r } } => {
                        let branches = get_local_branches(&repo);
                        let gitref = &branches[r];
                        switch_to(&repo, gitref, |path, perc| {
                            let _ = tx2.send(Envelope {
                                s: Message::PROGRESS { p: perc, path }
                            });
                        })
                    }
                    _ => ()
                }
            }
        }
    });

    // First Draw
    draw(&mut terminal, &branches, &app);

    // Reactor thread
    while let Ok(t) = rx.recv() {
        match t {
            Envelope { s: Message::MOVE { direction: MoveDirection::DOWN } } => {
                app.position = if app.position < branches.len() - 1 { app.position + 1 } else { app.position }
            }
            Envelope { s: Message::MOVE { direction: MoveDirection::UP } } => {
                app.position = if app.position == 0 { 0 } else { app.position - 1 }
            }
            Envelope { s: Message::ENTER } => {
                app.state = AppState::CHECKOUT;
                let _ = io_tx.send(Envelope { s: Message::CHECKOUT { selected: app.position } });
            }
            Envelope { s: Message::PROGRESS { p, path } } => {
                app.progress = p;
                app.path = path;
                if app.progress >= 100 {
                    app.state = AppState::LISTING;
                    app.progress = 0;
                    app.path = String::from("");
                }
            }
            Envelope { s: Message::SHUTDOWN } => {
                terminal.clear().unwrap();
                return;
            }
            _ => ()
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

    sorted.iter()
        .map(|t| {
            String::from(t.shorthand().unwrap())
        })
        .collect()
}

fn switch_to<F: Fn(String, u16) -> ()>(repo: &Repository, refs: &str, f: F) {
    let rev = repo.revparse_ext(&refs).unwrap().1.unwrap();
    let tree: Tree = rev.peel_to_tree().unwrap().clone();
    let mut cob = CheckoutBuilder::new();
    let mut co = cob
        .progress(|p, b, c| {
            let path = p.map_or("No Path", |pth| { pth.to_str().unwrap() });

            let up = b as u16;
            let to = c as u16;
            f(String::from(path), (((f32::from(up)) / f32::from(to)) * 100.0) as u16)
        });

    let name = rev.name().unwrap();

    repo.checkout_tree(&tree.as_object(), Option::Some(&mut co)).unwrap();
    repo.set_head(name).unwrap();
    f("DONE".to_string(), 100);
}

fn draw(terminal: &mut Terminal<MouseBackend>, b: &[String], a: &App) {
    match a.state {
        AppState::LISTING => {
            linker::draw_listing(terminal, b, a.position);
        }
        AppState::CHECKOUT => {
            linker::draw_checkout(terminal, a.progress, &a.path);
        }
    }

    let _ = terminal.draw();
}
