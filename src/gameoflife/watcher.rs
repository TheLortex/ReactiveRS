extern crate reactivers;
extern crate itertools;
extern crate ansi_escapes;

use reactivers::engine::process::*;
use reactivers::engine::signal::*;


use self::itertools::Itertools;
use std::thread;

use std::io;
use std::io::Write;

pub struct Watcher {}

impl Watcher {
    pub fn new() -> Watcher  {
        Watcher {}
    }

    pub fn process(self, mut signal_grid: Vec<Vec<MCSignal<bool, bool>>>) -> impl Process<Value=()> {
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

        let mut show_and_sleep_cont = |data: Vec<(bool, usize, usize)>| {
            for (st, x, y) in data {
                if y == 0 {
                    println!();
                    print!("     ");
                }

                if st {
                    print!("#");
                } else {
                    print!(" ");
                }
            }
            print!("{}", ansi_escapes::CursorTo::AbsoluteXY(0, 0));
            thread::sleep_ms(500);
        };
        print!("{}", ansi_escapes::ClearScreen);
        multi_join(updater_processes).map(show_and_sleep_cont).loop_inf()
    }
}