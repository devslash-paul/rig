#![deny(clippy_pedantic)]
#![allow(cast_possible_truncation)]
extern crate core;
extern crate git2;
extern crate itertools;
extern crate linker;
extern crate termion;
extern crate tui;

use git2::build::CheckoutBuilder;
use git2::FetchOptions;
use git2::Reference;
use git2::RemoteCallbacks;
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
use git2::Cred;
use termion::event::Key;

#[derive(PartialEq, Debug)]
enum MoveDirection {
    UP,
    DOWN,
}

#[derive(PartialEq, Debug)]
enum Message {
    MOVE {
        direction: MoveDirection
    },

    // ACTIONS
    SHUTDOWN,
    FETCH,
    FETCH_FILLED {
        refspec: usize
    },

    CHECKOUT {
        selected: usize
    },

    ENTER,
    PROGRESS {
        p: u16,
        path: String
    },
}

#[derive(PartialEq, Debug)]
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
            if let Some(t) = handle_event(evt) {
                let _ = tx.send(t);
            }
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
                    Envelope { s: Message::FETCH_FILLED { refspec } } => {
                        let branches = get_local_branches(&repo);
                        let gitref = branches.get(refspec).unwrap();
                        fetch(&repo, &[gitref.as_str()]);
                    }
                    _ => ()
                }
            }
        }
    });

    // First Draw
    draw(&mut terminal, &branches, &app);

    // ui thread
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
            Envelope { s: Message::FETCH } => {
                let _ = io_tx.send(Envelope { s: Message::FETCH_FILLED { refspec: app.position } });
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

fn handle_event(evt: Key) -> Option<Envelope> {
    match evt {
        // Directions
        event::Key::Char('j') | event::Key::Down => Some(Envelope { s: Message::MOVE { direction: MoveDirection::DOWN } }),
        event::Key::Char('k') | event::Key::Up => Some(Envelope { s: Message::MOVE { direction: MoveDirection::UP } }),

        // Actions
        event::Key::Char('\n') => Some(Envelope { s: Message::ENTER }),
        event::Key::Char('f') => Some(Envelope { s: Message::FETCH }),

        event::Key::Char('q') => Some(Envelope { s: Message::SHUTDOWN }),
        _ => None
    }
}

fn fetch(repo: &Repository, refspecs: &[&str]) {
    let mut remote = repo.find_remote("origin").expect("MUST HAVE A REMOTE CALLED ORIGIN");
    let mut cbs = RemoteCallbacks::default();
    cbs.credentials(|url, user, allowed| {
        Ok(Cred::ssh_key_from_agent("pault").unwrap())
    });

    let mut fo = FetchOptions::default();
    fo.remote_callbacks(cbs);

    remote.fetch(refspecs, Some(&mut fo), None).unwrap();
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
            f(String::from(path), ((f32::from(up)) / f32::from(to) * 100.0) as u16)
        });

    let name = rev.name().unwrap();

    repo.checkout_tree(&tree.as_object(), Option::Some(&mut co)).unwrap();
    repo.set_head(name).unwrap();
    // This will force a 100 percent to be reported in the event that the progress function
    // cannot (such as no progress to do)
    f("".to_string(), 100);
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

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_enter_key() {
        let result = handle_event(event::Key::Char('\n')).expect("Enter should create a command");
        assert_eq!(Envelope { s: Message::ENTER }, result);
    }
}
