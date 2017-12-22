extern crate reactivers;
extern crate itertools;

use reactivers::engine::process::*;
use reactivers::engine::signal::value_signal::MCSignal;
use reactivers::engine::signal::mpsc_signal::MPSCSignalSender;
use reactivers::engine::signal::*;


use self::itertools::Itertools;

pub struct GameCell {
    status_is_alive: bool,
}

impl GameCell {
    pub fn new(start_status: bool) -> GameCell {
        GameCell {
            status_is_alive: start_status,
        }
    }

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

    pub fn process(mut self,
                   life_signal: MCSignal<(), i32>,
                   neighbors_signal: Vec<MCSignal<(), i32>>,
                   (status_signal, x, y): (MPSCSignalSender<(usize, usize), Vec<(bool, usize, usize)>>, usize, usize)) -> impl Process<Value=()> {
        let write_neighbors = neighbors_signal.iter().map(|signal  | {
            value(()).emit(signal)
        }).collect_vec();
        let write_neighbors2 = neighbors_signal.iter().map(|signal  | {
            value(()).emit(signal)
        }).collect_vec();

        let mut send_status_alive = value((x, y)).emit(&status_signal);
        let mut send_status_alive2 = value((x, y)).emit(&status_signal);

        let status_is_alive = self.status_is_alive;

        let mut update_cell = move |alive_neighbor_count: i32| {
            self.update(alive_neighbor_count)
        };

        let cont_unit = |_| ();

        let p =
            value(status_is_alive)
            .then_else(
                send_status_alive.multi_join(write_neighbors),
                value(((), vec!())))
            .then(life_signal
                .await_in()
                .map(update_cell)
                .then_else(
                 send_status_alive2.multi_join(write_neighbors2).map(cont_unit),
                 value(())

                )
                .loop_inf());
        p
    }
}