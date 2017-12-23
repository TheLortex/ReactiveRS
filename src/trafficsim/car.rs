extern crate reactivers;
use reactivers::engine::process::*;
use reactivers::engine::signal::*;
use reactivers::engine::signal::mpsc_signal::MPSCSignalSender;
use reactivers::engine::signal::spmc_signal::SPMCSignalReceiver;

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

/// A simple Car speed.
pub type Speed = usize;

/// Car identifier.
pub type CarId = usize;

/// A Car
pub struct Car {
    id: CarId,              // Car identifier
    position: NodeId,       // Next crossroad node the car will reach.
    destination: NodeInfo,  // Destination crossroad.
    action: Action,         // Action to take at next crossroad.
    path: Vec<EdgeId>,      // Path to destination crossroad.
    d: Weight,              // Estimated distance to the destination.
    graph: Arc<Graph>,      // Graph of roads and crossroad nodes.
    speed: usize,           // Current speed.
}


impl Car {

    /// Creates a new car.
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

    /// Computes the path to the solution, using Dijkstra algorithm with specified estimations of
    /// edges lengths.
    fn compute_path(&mut self, weights: &EdgesWeight) {
        // Computes the path and updates state.
        let (path, d) =
            dijkstra(self.position,
                     |x| { *x == self.destination },
                     &self.graph,
                     weights);
        self.path = path;
        self.d = d;

        if self.path.is_empty() {
            println!("Car {} has to disappear, no solution.", self.id);
            self.action = Action::VANISH;
        }
        else {
            // We choose the direction to take at the next crossroad.
            self.action = Action::CROSS(self.next_road());
        }
    }

    /// Returns the next road to take.
    fn next_road(&self) -> EdgeInfo {
        *self.graph.get_edge(*self.path.last().unwrap()).info()
    }

    /// Updates the car state given the specified `move`, and computes the next action to take.
    /// Also returns the current speed.
    fn compute_action(&mut self, m: &Move, weights: &EdgesWeight) -> (Action, Speed) {
        match m {
            &Move::NONE => self.speed = 0,              // The car did not move.
            &Move::STEP(i) => self.speed = i as usize,  // The car did a step of length `i`.
            &Move::VANISH => {                          // The car vanished at a crossroad.
                self.speed = 0;                         // Resets the speed.
                self.action = Action::SPAWN;            // Chooses to respawn.
                return (self.action, 0);
            },
            &Move::CROSS(r) => {
                self.position = r.destination;          // Updates position.
                self.speed = 0;                         // Resets speed.
            },
            &Move::SPAWN(r, _, dest) => {
                self.destination = dest;                // Updates position, destination and speed.
                self.position = r.destination;
                self.speed = 0;
            }
        }

        if *self.graph.get_node(self.position).info() == self.destination {
            // The car chooses to vanish.
            self.action = Action::VANISH;
        }
        else {
            // Otherwise, we recompute the path.
            self.compute_path(weights);
        }

        (self.action, self.speed)
    }

    /// Returns the reactive process corresponding to the car.
    pub fn process(mut self,
                   central_signal: SPMCSignalReceiver<Arc<GlobalInfo>>,
                   pos_signal: MPSCSignalSender<(CarId, (Action, Speed)),
                                                (Vec<Action>, Vec<Speed>)>)
                   -> impl Process<Value=()>
    {
        let id = self.id;

        // Main loop: converts the move into an action.
        let cont = move |info: Arc<GlobalInfo>| {
            (id, self.compute_action(&info.moves[id], &info.weights))
        };

        // We initialize the car with a Spawn action.
        let v = (id, (Action::SPAWN, 0));
        let p =
            value(v).emit(&pos_signal).then(
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