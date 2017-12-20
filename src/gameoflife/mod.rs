extern crate reactivers;
extern crate itertools;

use reactivers::engine::signal::*;
use reactivers::engine::process::*;
use reactivers::engine;

mod cell;
mod watcher;

use self::cell::*;
use self::watcher::*;

use self::itertools::Itertools;

fn is_valid(x: isize, y: isize, n: usize, m: usize) -> bool {
    return x >= 0 && y >= 0 && x < n as isize && y < m as isize;
}

pub fn run_simulation(starting_grid: Vec<Vec<bool>>) {
    let n = starting_grid.len();
    if n == 0 {
        return;
    }
    let m = starting_grid[0].len();

    // Create cells and associated signals.
    let mut cell_signal_grid = starting_grid.iter().map(|line| {
        line.iter().map(|start_status| {
            (GameCell::new(*start_status), MCSignal::new(false, |x, _| x))
        }).collect_vec()
    }).collect_vec();

    // Create for each cell references to neighbor signals + save a copy for the watcher process.
    let mut watcher_signals: Vec<Vec<MCSignal<bool, bool>>> = vec!();

    let mut neighbors_grid = starting_grid.iter().enumerate().map(|(x, line)| {
        let mut watcher_line: Vec<MCSignal<bool, bool>> = vec!();

        let neighbors_line = line.iter().enumerate().map(|(y, _)| {
            let (_, ref signal) = cell_signal_grid[x][y];
            watcher_line.push(signal.clone());

            let mut ref_signals: Vec<MCSignal<bool, bool>> = vec!();

            for px in -1..1 {
                for py in -1..1 {
                    if is_valid(x as isize + px, y as isize + py, n, m) && (px != 1 || py != 1)   {
                        let x_as_usize = (x as isize + px) as usize;
                        let y_as_usize = (y as isize + py) as usize;

                        let (_, ref signal) = cell_signal_grid[x_as_usize][y_as_usize];
                        ref_signals.push(signal.clone())
                    }
                }
            };
            ref_signals
        }).collect_vec();
        watcher_signals.push(watcher_line);
        neighbors_line
    }).collect_vec();

    // Create processes.
    let mut cell_processes = vec!();

    while let Some(mut cell_signal_line) = cell_signal_grid.pop() {
        let mut neighbors_line = neighbors_grid.pop().unwrap();
        while let Some((cell, signal)) = cell_signal_line.pop() {
            let mut neighbors = neighbors_line.pop().unwrap();
            cell_processes.push(cell.process(signal, neighbors));
        }
    };

    //let watcher = Watcher::new();
    //let watcher_process = watcher.process(watcher_signals);

    let simulation_process = /*watcher_process.*/multi_join(cell_processes);

    // Run the thing
    engine::execute_process(simulation_process);
}