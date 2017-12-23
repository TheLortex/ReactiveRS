extern crate reactivers;
extern crate ncurses;

use reactivers::engine::process::*;
use reactivers::engine::signal::mpsc_signal::MPSCSignalReceiver;
use reactivers::engine::signal::*;

/// Watcher structure that will listen to a signal to render game status.
pub struct TerminalWatcher {
    auto: bool,
    width: i32,
    height: i32,
}

impl TerminalWatcher {
    /// Create a new `TerminalWatcher`
    pub fn new(n: i32, m: i32) -> TerminalWatcher {
        TerminalWatcher {
            auto: true,
            width: n,
            height: m,
        }
    }

    /// Render game status, data being the list of alive cells.
    /// Returns coordinates of the created window.
    pub fn render_grid(&self, data: Vec<(usize, usize)>) -> (i32, i32, ncurses::WINDOW) {
        let mut max_x = 0;
        let mut max_y = 0;
        ncurses::getmaxyx(ncurses::stdscr(), &mut max_y, &mut max_x);


        let (start_y, start_x) = ((max_y - self.height) / 2, (max_x - self.width) / 2);
        let win = ncurses::newwin(self.height + 2, self.width + 2, start_y, start_x);
        ncurses::box_(win, 0, 0);

        for (x, y) in data {
            ncurses::mvwaddch(win, (self.height - y as i32) as i32, (self.width - x as i32) as i32, '#' as ncurses::chtype);
        }
        ncurses::wrefresh(win);
        (start_y, start_x, win)
    }

    /// Consumes self to create a reactive process that will listen to `alive_signal` to render the game status.
    pub fn process(mut self, alive_signal: MPSCSignalReceiver<(usize, usize), Vec<(usize, usize)>>) -> impl Process<Value=()> {
        if self.auto {
            ncurses::timeout(500); // In automatic mode, 500ms between steps.
        } else {
            ncurses::timeout(-1);// In manual mode, wait for input.
        }

        let show_and_sleep_cont = move |data: Vec<(usize, usize)>| {
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
        alive_signal.await_in().map(show_and_sleep_cont).loop_inf()
    }
}