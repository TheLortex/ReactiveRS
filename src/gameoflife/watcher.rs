extern crate reactivers;
extern crate itertools;
extern crate ncurses;

use reactivers::engine::process::*;
use reactivers::engine::signal::*;


use self::itertools::Itertools;
use std::thread;

use std::io;
use std::io::Write;

pub struct TerminalWatcher {
    auto: bool,
    width: i32,
    height: i32,
}

impl TerminalWatcher {
    pub fn new(n: i32, m: i32) -> TerminalWatcher {
        TerminalWatcher {
            auto: true,
            width: n,
            height: m,
        }
    }

    pub fn render_grid(&self, data: Vec<(bool, usize, usize)>) -> (i32, i32, ncurses::WINDOW) {
        let mut max_x = 0;
        let mut max_y = 0;
        ncurses::getmaxyx(ncurses::stdscr(), &mut max_y, &mut max_x);


        let (start_y, start_x) = ((max_y - self.height) / 2, (max_x - self.width) / 2);
        let win = ncurses::newwin(self.height + 2, self.width + 2, start_y, start_x);
        ncurses::box_(win, 0, 0);

        for (st, x, y) in data {
            if st {
                ncurses::mvwaddch(win, (self.height - y as i32) as i32, (self.width - x as i32) as i32, '#' as ncurses::chtype);
            } else {
                ncurses::mvwaddch(win, (self.height - y as i32) as i32, (self.width - x as i32) as i32, ' ' as ncurses::chtype);
            }
        }
        ncurses::wrefresh(win);
        (start_y, start_x, win)
    }

    pub fn process(mut self, mut signal_grid: Vec<Vec<MCSignal<bool, bool>>>) -> impl Process<Value=()> {
        if self.auto {
            ncurses::timeout(500);
        } else {
            ncurses::timeout(-1);
        }

        let mut updater_processes = vec!();

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
            self.render_grid(data);

            let chr = ncurses::getch();
            if chr == 'a' as i32 || chr == ' ' as i32 {
                self.auto = !self.auto;

                if self.auto {
                    ncurses::timeout(500);
                } else {
                    ncurses::timeout(-1);
                }
            } else if chr == 'q' as i32 {
                ncurses::endwin();
                panic!("Exited.");
            }

        };
        multi_join(updater_processes).map(show_and_sleep_cont).loop_inf()
    }
}