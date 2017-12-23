extern crate reactivers;
extern crate itertools;

use reactivers::engine::signal::*;
use reactivers::engine::signal::value_signal::ValueSignal;
use reactivers::engine::process::*;
use reactivers::engine;

mod cell;
pub mod watcher;

use self::cell::*;
use self::watcher::*;
use self::itertools::Itertools;

/// Check if coordinates (x,y) are in a n*m grid.
pub fn is_valid(x: isize, y: isize, n: usize, m: usize) -> bool {
    return x >= 0 && y >= 0 && x < n as isize && y < m as isize;
}

/// Converts the boolean grid format to a vector of positions and state for rendering.
pub fn grid_to_data (starting_grid: &Vec<Vec<bool>>) -> Vec<(usize, usize)>{
    let mut data_vec = vec!();

    let n = starting_grid.len();
    let m = starting_grid[0].len();

    for (x, line) in starting_grid.iter().enumerate() {
        for (y, elem) in line.iter().enumerate() {
            if *elem {
                data_vec.push((n-1-x, m-1-y));
            }
        }
    };
    data_vec
}

pub fn run_simulation (starting_grid: Vec<Vec<bool>>, watcher: Option<TerminalWatcher>) {
    run_simulation_steps(starting_grid, watcher, 4, -1);
}

/// Run a simulation, with a given starting grid and a watcher process that can render what is happening.
pub fn run_simulation_steps (starting_grid: Vec<Vec<bool>>, watcher: Option<TerminalWatcher>, n_workers: usize, max_iters: i32)
{
    let n = starting_grid.len();
    if n == 0 {
        return;
    }
    let m = starting_grid[0].len();

    // Create the signal that the renderer will listen on.
    let (multi_producer, single_consumer) = mpsc_signal::new(|(x, y), mut alive_list: Vec<(usize, usize)>| {
        alive_list.push((x, y));
        alive_list
    });

    // Create cells and associated signals.
    let mut cell_signal_grid = starting_grid.iter().map(|line| {
        line.iter().map(|start_status| {
            (GameCell::new(*start_status), value_signal::new(0, |(), y| 1 + y), multi_producer.clone())
        }).collect_vec()
    }).collect_vec();

    // Create for each cell references to neighbor signals.
    let mut neighbors_grid = starting_grid.iter().enumerate().map(|(x, line)| {
        let neighbors_line = line.iter().enumerate().map(|(y, _)| {
            let mut ref_signals: Vec<ValueSignal<(), i32>> = vec!();

            for px in -1..2 {
                for py in -1..2 {
                    if is_valid(x as isize + px, y as isize + py, n, m) && (px != 0 || py != 0)   {
                        let x_as_usize = (x as isize + px) as usize;
                        let y_as_usize = (y as isize + py) as usize;

                        let (_, ref signal, _) = cell_signal_grid[x_as_usize][y_as_usize];
                        ref_signals.push(signal.clone())
                    }
                }
            };
            ref_signals
        }).collect_vec();
        neighbors_line
    }).collect_vec();

    // Create processes.
    let mut cell_processes = vec!();
    let mut i = 0;
    while let Some(mut cell_signal_line) = cell_signal_grid.pop() {

        let mut j = 0;
        let mut neighbors_line = neighbors_grid.pop().unwrap();

        while let Some((cell, signal, status_emitter)) = cell_signal_line.pop() {

            let mut neighbors = neighbors_line.pop().unwrap();
            cell_processes.push(cell.process(signal, neighbors, (status_emitter, i, j)));
            j += 1;
        }

        i += 1;
    };

    if let Some(watcher) = watcher {
        // Create renderer process.
        let watcher_process = watcher.process(single_consumer);
        // Combine processes.
        let simulation_process = watcher_process.multi_join(cell_processes);
        // Run the thing
        engine::execute_process_steps(simulation_process, n_workers, max_iters);
    } else {
        let simulation_process = multi_join(cell_processes);
        engine::execute_process_steps(simulation_process, n_workers, max_iters);
    }
}
