extern crate reactivers;
extern crate itertools;
extern crate ncurses;

use reactivers::engine::process::*;
use reactivers::engine::signal::*;

use self::ncurses::*;

use self::itertools::Itertools;
use std::thread;

pub struct Watcher {}

impl Watcher {
    pub fn new() -> Watcher  {
        /* Setup ncurses. */
        initscr();
        raw();

        /* Allow for extended keyboard (like F1). */
        keypad(stdscr(), true);
        noecho();

        /* Invisible cursor. */
        curs_set(CURSOR_VISIBILITY::CURSOR_INVISIBLE);

        Watcher {}
    }

    pub fn process(self, mut signal_grid: Vec<Vec<MCSignal<bool, bool>>>) -> impl Process<Value=()> {
        let mut updater_processes = vec!();
        let mut x = 0;
        let mut y = 0;
        while let Some(mut line) = signal_grid.pop() {
            while let Some(signal) = line.pop() {
                let (x_, y_) = (x, y);
                let mut cont = move |status| {
                    mv(x_, y_);
                    if status {
                        addch('#' as u64);
                    } else {
                        addch(' ' as u64);
                    }
                };
                updater_processes.push(signal.await_in().map(cont));
                y += 1;
            }
            x += 1;
        }

        let mut sleep_cont = |_| thread::sleep_ms(500);

        multi_join(updater_processes).map(sleep_cont).loop_inf()
    }
}