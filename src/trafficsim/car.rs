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
    SPAWN,
    CROSS(RoadId),
}

pub type Speed = usize;

pub type CarId = usize;
pub struct Car {
    id: CarId,
    position: NodeId,
    destination: NodeInfo,
    action: Action,
    path: Vec<EdgeId>,
    d: Weight,
    graph: Arc<Graph>,
    speed: usize,
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
            graph,
            speed: 0,
        }
    }

    fn compute_path(&mut self, weights: &EdgesWeight) {
        let (path, d) = dijkstra(self.position, |x| {*x == self.destination}, &self.graph, weights);
        self.path = path;
        self.d = d;

        if self.path.is_empty() {
            println!("Car {} has tp disappear, no solution.", self.id);
            self.action = Action::VANISH;
        }
        else {
            self.action = Action::CROSS(self.next_road());
        }
    }

    fn next_road(&self) -> EdgeInfo {
        *self.graph.get_edge(*self.path.last().unwrap()).info()
    }

    fn compute_action(&mut self, m: &Move, weights: &EdgesWeight) -> (Action, Speed) {
        match m {
            &Move::NONE => self.speed = 0,
            &Move::STEP(i) => self.speed = i as usize,
            &Move::VANISH => {
                self.speed = 0;
                self.action = Action::SPAWN;
                return (self.action, 0);
            },//println!("Car {} really arrived at destination {}", self.id, self.destination),
            &Move::CROSS(r) => {
                self.position = r.destination;
                self.speed = 0;
            },
            &Move::SPAWN(r, _, dest) => {
                self.destination = dest;
                self.position = r.destination;
                self.speed = 0;
            }
        }

        if *self.graph.get_node(self.position).info() == self.destination {
//            println!("Car {} just arrived at destination {}", self.id, self.destination);
            // TODO: Change destination or choose to die.
            self.action = Action::VANISH;
        }
        else {
            self.compute_path(weights);
        }

        (self.action, self.speed)
    }

    pub fn process(mut self, central_signal: SPMCSignal<Arc<GlobalInfos>>,
                   pos_signal: MPSCSignal<(CarId, (Action, Speed)), (Vec<Action>, Vec<Speed>)>) -> impl Process<Value=()>
    {
        let id = self.id;

        let cont = move |infos: Arc<GlobalInfos>| {
//            println!("{}", self);
            (id, self.compute_action(&infos.moves[id], &infos.weights))
        };

        let v = (id, (Action::SPAWN, 0));
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