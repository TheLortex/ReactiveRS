extern crate reactivers;
use reactivers::engine::signal::*;
use reactivers::engine::process::*;
use reactivers::engine;

pub mod car;
pub mod graph;
pub mod road;
pub mod network;

use self::network::*;
use self::car::*;

use std::sync::Arc;

pub fn run_simulation(network: Network, cars: Vec<Car>) {
    let (central_signal, central_sender) = SPMCSignal::new();
    let (pos_signal, pos_signal_receiver) =
        MPSCSignal::new(|(id, action): (CarId, Action), mut v: Vec<Action>| {
            for _ in (v.len())..(id+1) {
                v.push(Action::VANISH);
            }
            v[id] = action;
            v
        });
    let graph = Arc::new(network.clone_graph().clone());

    let (mut network_process, global_infos) = network.process(central_sender, pos_signal_receiver);
    let car_processes = cars.into_iter().map(|c| {
        c.process(
            central_signal.clone(),
            pos_signal.clone(),
            global_infos.clone()
        )
    }).collect();

    let process = network_process.multi_join(car_processes);

    engine::execute_process(process, 8, -1);
}