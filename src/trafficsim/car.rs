extern crate reactivers;

use reactivers::engine::process::*;
use reactivers::engine::signal::*;

use super::network::*;
use std::f32;

use std::sync::Arc;

#[derive(Clone)]
struct Position {
    road: EdgeInfo,
    rank: i32,
    direction: EdgeInfo,
}

struct Car {
    source: NodeId,
    destination: NodeInfo,
    position: Position,
    path: Vec<EdgeId>,
    d: Weight,
}

impl Car {
    fn new(source: NodeId, destination: NodeInfo) -> Car {
        let p = Position { road: -1, rank: -1, direction: -1};
        Car {source, destination, position: p, path: vec!(), d: f32::MAX }
    }

    fn compute_path(&mut self, network: &Network, weights: &EdgesWeight) {
        let (path, d) = dijkstra(self.source, |x| {*x == self.destination}, network, weights);
        self.path = path;
        self.d = d;
        self.position.direction = self.next_road(network);
    }

    fn next_road(&self, network: &Network) -> EdgeInfo {
        *network.get_edge(*self.path.last().unwrap()).info()
    }
}


fn car_process(car: Car, central_signal: SPMCSignal<Arc<EdgesWeight>>,
               pos_signal: MPSCSignal<Position, Vec<Position>>,
               network: Arc<Network>) {
    let mut c = car;
    let cont = move |weights: Arc<EdgesWeight>| {
        c.compute_path(&network, &*weights);
        c.position.clone()
    };
    let p = central_signal.await_in().map(cont).emit(pos_signal).loop_inf();
}
