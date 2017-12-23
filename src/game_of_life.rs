#![feature(conservative_impl_trait)]
#![type_length_limit="2097152"]

extern crate reactivers;
extern crate rand;
extern crate ncurses;

mod gameoflife;


use self::rand::distributions::IndependentSample;
use ncurses::*;
use std::cmp;

pub fn game_of_life () {
    initscr();

    keypad(stdscr(), false);
    noecho();

    ncurses::timeout(10);
    ncurses::getch();
    ncurses::timeout(-1);

    let (mut x, mut y) = (0, 0);
    ncurses::getmaxyx(ncurses::stdscr(), &mut y, &mut x);

    let (n, m) = (cmp::min(60, x as usize - 4), cmp::min(30, y as usize - 2));

    let watcher = gameoflife::watcher::TerminalWatcher::new(60, 30);

    let mut starting_grid = vec!();
    for i in 0..n {
        let mut line = vec!();
        for j in 0..m {
            line.push((i as isize - 10)*(i as isize - 10) + (j as isize - 10)*(j as isize - 10) < 5)
        }
        starting_grid.push(line);
    }

    let (mut ofs_y, mut ofs_x, mut win) = watcher.render_grid(gameoflife::grid_to_data(&starting_grid));
    ncurses::mvprintw(ofs_y, x/2 - 6, "Game of life");
    ncurses::mvprintw(ofs_y  + (m as i32) + 2, x / - 39, "q: Quit | r: Randomize | Click to toggle cells | Enter: start the simulation");

    keypad(win , true);
    mousemask((ncurses::BUTTON1_PRESSED  | ncurses::REPORT_MOUSE_POSITION) as u64, None);
    refresh();

    let mut rng = rand::thread_rng();
    let between = rand::distributions::Range::new(0f64, 1f64);

    let mut c = ncurses::wgetch(win);

    while c != 10 {

        if c == ncurses::KEY_MOUSE {
            let mut event: ncurses::MEVENT = ncurses::MEVENT { id: 0,  x: 0,  y: 0,  z: 0,  bstate: 0};
            if ncurses::getmouse(&mut event) == ncurses::OK {
                if (event.bstate & ncurses::BUTTON1_PRESSED as u64) > 0 {
                    let x = event.x - ofs_x - 1;
                    let y = event.y - ofs_y - 1;

                    if gameoflife::is_valid(x as isize, y as isize, n, m) {
                        starting_grid[x as usize][y as usize] = !starting_grid[x as usize][y as usize];

                    }
                }
            }
        } else if c == 3 || c == 'q' as i32 { // Quit
            clear();
            keypad(win, false);
            endwin();
            mousemask(0, None);
            refresh();
            return;
        } else if c == 'r' as i32 { // Randomize.
            let p = 0.2;

            for x in 0..n {
                for y in 0..m {
                    if between.ind_sample(&mut rng) < p {
                        starting_grid[x][y] = true;
                    }
                }
            }
        }

        let (ofs_y_, ofs_x_, win_) = watcher.render_grid(gameoflife::grid_to_data(&starting_grid));
        ofs_y = ofs_y_;
        ofs_x = ofs_x_;
        win = win_;
        wrefresh(win);
        keypad(win, true);
        c = ncurses::wgetch(win);
    }

    gameoflife::run_simulation(starting_grid, watcher);
    endwin();
}

pub fn main() {
    game_of_life();
}