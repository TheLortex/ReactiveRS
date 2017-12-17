extern crate reactivers;
use reactivers::engine::process::*;
use reactivers::engine::signal::*;

use super::graph::*;
use super::road::*;
use super::network::*;

use std::f32;

use std::sync::Arc;

#[derive(Copy, Clone)]
pub enum Action {
    VANISH,
    CROSS(RoadId),
}

pub type CarId = usize;
pub struct Car {
    id: CarId,
    position: NodeId,
    destination: NodeInfo,
    action: Action,
    path: Vec<EdgeId>,
    d: Weight,
    graph: Arc<Graph>
}

pub struct GlobalInfos {
    pub weights: EdgesWeight,
    pub moves: Vec<Move>,
}

impl Car {
    pub fn new(id: CarId, source: NodeId, destination: NodeInfo, graph: Arc<Graph>) -> Car {
        Car { id,
            position: source,
            destination,
            action: Action::VANISH,
            path: vec!(),
            d: f32::MAX,
            graph
        }
    }

    fn compute_path(&mut self, weights: &EdgesWeight) {
        let (path, d) = dijkstra(self.position, |x| {*x == self.destination}, &self.graph, weights);
        self.path = path;
        self.d = d;
        self.action = Action::CROSS(self.next_road());
    }

    fn next_road(&self) -> EdgeInfo {
        *self.graph.get_edge(*self.path.last().unwrap()).info()
    }

    fn compute_action(&mut self, m: &Move, weights: &EdgesWeight) -> Action {
        match m {
            &Move::NONE | &Move::STEP(_) => (),
            &Move::VANISH => (),//println!("Car {} really arrived at destination {}", self.id, self.destination),
            &Move::CROSS(ref r) => {
                self.position = r.destination;
            },
        }

        if *self.graph.get_node(self.position).info() == self.destination {
//            println!("Car {} just arrived at destination {}", self.id, self.destination);
            // TODO: Change destination or choose to die.
            self.action = Action::VANISH;
        }
        else {
            self.compute_path(weights);
        }

        self.action.clone()
    }

    pub fn process(mut self, central_signal: SPMCSignal<Arc<GlobalInfos>>,
                   pos_signal: MPSCSignal<(CarId, Action), Vec<Action>>,
                   global_infos: Arc<GlobalInfos>) -> impl Process<Value=()> {
        let id = self.id;
//        self.compute_path(&graph, &global_infos.weights);
//        let v = (id, self.action);

        let mut cont = move |infos: Arc<GlobalInfos>| {
//            println!("{}", self);
            (id, self.compute_action(&infos.moves[id], &infos.weights))
        };
        let v = cont(global_infos);
        let p = value(v).emit(&pos_signal).then(
            central_signal.await_in().map(cont).emit(&pos_signal).loop_inf()
        );
        return p;
    }
}

use std::fmt;
impl fmt::Display for Car {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Car {} at node {} (crossroad {}), going to crossroad {}.",
            self.id,
            self.position,
            self.graph.get_node(self.position).info(),
            self.destination
        )
    }
}