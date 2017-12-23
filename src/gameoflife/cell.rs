extern crate reactivers;
extern crate itertools;

use reactivers::engine::process::*;
use reactivers::engine::signal::value_signal::ValueSignal;
use reactivers::engine::signal::mpsc_signal::MPSCSignalSender;
use reactivers::engine::signal::*;


use self::itertools::Itertools;

/// A cell in the game of life, it can be alive or dead.
pub struct GameCell {
    /// Current status of the cell.
    status_is_alive: bool,
}

impl GameCell {
    /// Creates a new game cell with a given status.
    pub fn new(start_status: bool) -> GameCell {
        GameCell {
            status_is_alive: start_status,
        }
    }

    /// Update cell according to own status, alive neighbor count and the rules of the game of life.
    pub fn update(&mut self, alive_neighbor_count: i32) -> bool {
        if self.status_is_alive {
            if (alive_neighbor_count <= 1) || (alive_neighbor_count >= 4) {
                self.status_is_alive = false
            }
        } else {
            if alive_neighbor_count == 3 {
                self.status_is_alive = true
            }
        };
        self.status_is_alive
    }

    /// Consume self to create a reactive process that will live according to the rules of the game
    /// of life.
    pub fn process(mut self,
                   life_signal: ValueSignal<(), i32>,
                   neighbors_signal: Vec<ValueSignal<(), i32>>,
                   (status_signal, x, y): (MPSCSignalSender<(usize, usize), Vec<(usize, usize)>>, usize, usize)) -> impl Process<Value=()> {
        // A vector of processes, each process being the transmission of the alive signal to a neighbor.
        let write_neighbors = neighbors_signal.iter().map(|signal| {
            value(()).emit(signal)
        }).collect_vec();
        let write_neighbors2 = neighbors_signal.iter().map(|signal| {
            value(()).emit(signal)
        }).collect_vec();

        // A process that sends cell's coordinates to the watcher process.
        let send_status_alive = value((x, y)).emit(&status_signal);
        let send_status_alive2 = value((x, y)).emit(&status_signal);

        // Copy value before move.
        let status_is_alive = self.status_is_alive;

        // A continuation that updates internal structure according to the number of alive neighbors.
        let update_cell = move |alive_neighbor_count: i32| {
            self.update(alive_neighbor_count)
        };

        // A continuation that ignores input and returns unit (for type checker).
        let cont_unit = |_| ();

        let main_loop =
            life_signal
                .await_in() // Wait for neighbors to tell if they're alive.
                .map(update_cell) // Update own status
                .then_else( // If cell is alive
                    send_status_alive2.multi_join(write_neighbors2).map(cont_unit), // Send life signal to neighbors and watcher
                    value(()) // Else do nothing
                )
                .loop_inf();

        // The cell process.
        let p =
            value(status_is_alive)
            .then_else(
                send_status_alive.multi_join(write_neighbors),
                value(((), vec!())))
            .then(main_loop);
        p
    }
}