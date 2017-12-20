extern crate reactivers;
extern crate itertools;
extern crate ncurses;

use reactivers::engine::process::*;
use reactivers::engine::signal::*;


use self::itertools::Itertools;
use std::thread;

use std::io;
use std::io::Write;

pub struct Watcher {
    auto: bool,
}

impl Watcher {
    pub fn new() -> Watcher  {
        ncurses::initscr();
        ncurses::noecho();

        Watcher {
            auto: false,
        }
    }

    pub fn process(mut self, mut signal_grid: Vec<Vec<MCSignal<bool, bool>>>) -> impl Process<Value=()> {
        let mut updater_processes = vec!();
        let n: i32 = signal_grid.len() as i32;
        let m: i32 = signal_grid[0].len() as i32;


        let mut x = 0;
        let mut y = 0;
        while let Some(mut line) = signal_grid.pop() {
            y = 0;
            while let Some(signal) = line.pop() {
                let (x_, y_) = (x, y);
                let mut cont = move |status| {
                    (status, x, y)
                };
                updater_processes.push(signal.await_in().map(cont));
                y += 1;
            }
            x += 1;
        }

        let mut show_and_sleep_cont = move |data: Vec<(bool, usize, usize)>| {
            let mut max_x = 0;
            let mut max_y = 0;
            ncurses::getmaxyx(ncurses::stdscr(), &mut max_y, &mut max_x);
            let (start_y, start_x) = ((max_y - m) / 2, (max_x - n) / 2);
            let win = ncurses::newwin(m+2, n+2, start_y, start_x);
            ncurses::box_(win, 0, 0);
            ncurses::wrefresh(win);

            for (st, x, y) in data {
                if st {
                    ncurses::mvwaddch(win, (y+1) as i32, (x+1) as i32,'#' as ncurses::chtype);
                } else {
                    ncurses::mvwaddch(win, (y+1) as i32, (x+1) as i32,' ' as ncurses::chtype);
                }
            }
            ncurses::wrefresh(win);
            let chr = ncurses::getch();
            if chr == 'a' as i32 {
                self.auto = !self.auto;

                if self.auto {
                    ncurses::timeout(500);
                } else {
                    ncurses::timeout(-1);
                }
            }

        };
        multi_join(updater_processes).map(show_and_sleep_cont).loop_inf()
    }
}