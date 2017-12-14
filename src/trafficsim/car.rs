extern crate reactivers;
use reactivers::engine::process::*;
use reactivers::engine::signal::*;

use super::network::*;
use std::f32;

use std::sync::Arc;

#[derive(Clone)]
pub struct Position {
    pub id: CarId,
    pub road: EdgeInfo,
    pub rank: usize,
    pub direction: EdgeInfo,
    pub has_moved: bool,
}

pub type CarId = usize;
struct Car {
    source: NodeId,
    destination: NodeInfo,
    position: Position,
    path: Vec<EdgeId>,
    d: Weight,
}

pub struct GlobalInfos {
    pub weights: EdgesWeight,
    pub positions: Vec<Position>,
}

impl Car {
    fn new(id: CarId, source: NodeId, destination: NodeInfo) -> Car {
        let p = Position { id, road: 0, rank: 0, direction: 0, has_moved: false };
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


fn car_process(car: Car, central_signal: SPMCSignal<Arc<GlobalInfos>>,
               pos_signal: MPSCSignal<Position, Vec<Position>>,
               network: Arc<Network>) -> impl Process<Value=()> {
    let mut c = car;
    let cont = move |infos: Arc<GlobalInfos>| {
        c.position = infos.positions[c.position.id].clone();
        if c.position.has_moved {
            c.source = network.get_edge(c.path.pop().unwrap()).destination()
        }
        c.compute_path(&network, &infos.weights);
        c.position.clone()
    };
    let p = central_signal.await_in().map(cont).emit(&pos_signal).loop_inf();
    return p;
}
