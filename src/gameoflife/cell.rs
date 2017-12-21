extern crate reactivers;
extern crate itertools;

use reactivers::engine::process::*;
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

    pub fn update(&mut self, alive_neighbor_count: usize) -> bool {
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


    pub fn process(mut self, life_signal: MCSignal<bool, bool>, neighbors_signal: Vec<MCSignal<bool, bool>>) -> impl Process<Value=()> {
        let wait_neighbors = neighbors_signal.iter().map(|signal  | {
            signal.await_in()
        }).collect_vec();

        let status_is_alive = self.status_is_alive;

        let mut update_cell = move |signal_result: Vec<bool>| {
            let alive_neighbor_count = signal_result.iter().fold(0, |tot, value|
                {
                    if *value {
                        tot + 1
                    } else {
                        tot
                    }
                });
            self.update(alive_neighbor_count)
        };

        let p = value(status_is_alive).emit(&life_signal)
            .then(
                multi_join(wait_neighbors).map(update_cell).emit(&life_signal)
                    .loop_inf()
            );
        p
    }
}