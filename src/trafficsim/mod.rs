pub mod car;
pub mod network;
pub mod road;

extern crate reactivers;
use reactivers::engine::process::*;
use reactivers::engine::signal::*;
use self::network::*;
use self::car::*;
use self::road::*;

use std::sync::Arc;

fn compute_enabled_paths(roads: &mut Vec<Road>, network: Arc<Network>) -> Vec<Vec<RoadId>> {
    let n = roads.len();
    (0..n).map(|_| { vec!() }).collect()
}

fn global_process(car_count: usize, central_signal: SPMCSignalSender<Arc<GlobalInfos>>,
                  pos_signal: MPSCSignalReceiver<Position, Vec<Position>>,
                  network: Arc<Network>, mut roads: Vec<Road>) -> impl Process<Value=()> {

    let n = network.clone();
    let cont = move | mut positions: Vec<Position> | {
        let mut roads = &mut roads;
        let enabled_paths = compute_enabled_paths(roads, n.clone());
        let weights = do_step(roads, enabled_paths, &mut positions);
        let res = Arc::new(GlobalInfos {weights, positions });
        res
    };

    let p = pos_signal.await_in().map(cont).emit_consume(central_signal).loop_inf();
    return p;
}