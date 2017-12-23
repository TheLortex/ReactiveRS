extern crate reactivers;
extern crate rand;

use reactivers::engine::process::*;
use reactivers::engine::signal::*;
use reactivers::engine::signal::spmc_signal::*;
use reactivers::engine::signal::mpsc_signal::*;

use std::sync::Arc;
use std::fs::File;
use std::io::prelude::*;

use self::rand::Rng;

use super::graph::*;
use super::car::*;
use super::road::*;

// These constant directions are used to index roads and nodes at a crossroad.
const NORTH:    usize = 0;
const EAST:     usize = 1;
const SOUTH:    usize = 2;
const WEST:     usize = 3;

pub type Side = usize;
const LEFT:     usize = 0;
const RIGHT:    usize = 1;

/// Global update network information.
pub struct GlobalInfo {
    pub weights: EdgesWeight,   // Last estimation of the edges weights.
    pub moves: Vec<Move>,       // Moves of all the cars.
}

/// Move of a car.
#[derive(Copy, Clone)]
pub enum Move {
    NONE,                                   // None happened.
    SPAWN(RoadInfo, usize, CrossroadId),    // The car has spawned at specified road, position,
                                            // with specified destination crossroad.
    STEP(i32),                              // The car performs a step of specified length.
    VANISH,                                 // The car vanished.
    CROSS(RoadInfo),                        // The car crossed and is now on specified road.
}

/// Network structure containing all the information relative to crossroads and roads.
#[derive(Clone)]
pub struct Network {
    pub width: usize,                   // Width of the network.
    pub height: usize,                  // Height of the network.
    pub car_count: usize,               // Number of cars.
    pub cars_per_unit: i32,             // Number of cars between two centers of crossroads.
    pub cars_per_crossroad: i32,        // Number of cars fitting in a crossroad.
    grid: Vec<Vec<Option<Crossroad>>>,  // Grid containing the crossroads.
    pub roads: Vec<Road>,               // Vector containing the roads.
    graph: Graph,                       // Corresponding abstract graph.
    car_graph: Option<Arc<Graph>>,      // Shared reference to the same graph.
    pub crossroads: Vec<CrossroadId>,   // Vector containing all the coordinates of existing
                                        // crossroads.
}

/// Crossroad Coordinates.
#[derive(Copy, Clone, Eq, PartialEq)]
pub struct CrossroadId {
    pub x: usize,   // Abscissa
    pub y: usize,   // Ordinate
}


use std::ops::{ Index, IndexMut };
/// Allows indexing the grid by the crossroad coordinates.
impl Index<CrossroadId> for Vec<Vec<Option<Crossroad>>> {
    type Output = Option<Crossroad>;

    #[inline]
    fn index(&self, index: CrossroadId) -> &Option<Crossroad> {
        &self[index.y][index.x]
    }
}

/// Allows mutable indexing of the grid by the crossroad coordinates.
impl IndexMut<CrossroadId> for Vec<Vec<Option<Crossroad>>> {
    #[inline]
    fn index_mut(&mut self, index: CrossroadId) -> &mut Option<Crossroad> {
        &mut self[index.y][index.x]
    }
}

use std::fmt;
impl fmt::Display for CrossroadId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "({}, {})", self.x, self.y)
    }
}

use std::ops::Add;
/// Allows to add some move to crossroad coordinates.
impl Add<(i32, i32)> for CrossroadId {
    type Output = CrossroadId;

    fn add(self, (x, y): (i32, i32)) -> CrossroadId {
        CrossroadId {
            x: (self.x as i32 + x) as usize,
            y: (self.y as i32 + y) as usize,
        }
    }
}

impl CrossroadId {
    /// Creates new crossroad identifier.
    pub fn new(x: usize, y: usize) -> CrossroadId {
        CrossroadId { x, y }
    }

    /// Computes the unit move (dx, dy) and the length to join the destination crossroad.
    pub fn join(&self, dest: CrossroadId) -> (i32, i32, i32) {
        if self.x == dest.x {
            let dy = (dest.y as i32) - (self.y as i32);
            let len = i32::abs(dy);
            (0, dy / len, len)
        }
        else if self.y == dest.y {
            let dx = (dest.x as i32) - (self.x as i32);
            let len = i32::abs(dx);
            (dx / len, 0, len)
        }
        else {
            panic!("Crossroads {} and {} are not linkable.", self, dest);
        }
    }
}

/// A Crossroad.
#[derive(Clone)]
pub struct Crossroad {
    id: CrossroadId,                            // Coordinates
    pub nodes: Vec<NodeId>,                     // Vector of its 4 quarter nodes.
                                                // They are indexed by direction.
    roads: Vec<Vec<Option<RoadId>>>,            // Roads leaving this crossroad.
                                                // They are indexed by direction and side.
    roads_arriving: Vec<Vec<Option<RoadId>>>,   // Roads arriving at this crossroad.
                                                // They are indexed by direction and side.
}

impl Crossroad {
    /// Creates a new crossroad with four nodes without any roads.
    pub fn new(id: CrossroadId, g: &mut Graph) -> Crossroad {
        let mut c = Crossroad {
            id,
            nodes: vec!(),
            roads: none_array(4, 2),
            roads_arriving: none_array(4, 2),
        };

        for _ in 0..4 {
            c.nodes.push(g.add_node(c.id));
        }
        c
    }

    /// Enables some roads. Only the cars from enabled roads are able to cross a crossroad.
    fn enable_path(&self, roads: &mut Vec<Road>) {
        // First policy: we enable the most loaded road with some guy waiting.
//        let mut max = -1;
//        let mut r_max = 0;
//        for r in self.existing_roads_arriving() {
//            if roads[r].is_waiting() && roads[r].get_car_count() > max {
//                r_max = r;
//                max = roads[r].get_car_count();
//            }
//        }
//        roads[r_max].enable();

        // Second policy: we enable the most loaded roads with guys waiting, but in pairs.
        // We compute the pair of compatible roads with the maximum cumulated load.
        let mut max_pair = ((NORTH, LEFT), (NORTH, LEFT));
        let mut max_load = 0;

        for d in 0..4 {
            for s in 0..2 {
                for x in 0..2 {
                    let (d2, s2) = {
                        if x == 0 {
                            (d, 1 - s)
                        }
                        else {
                            ((d + 2) % 4, s)
                        }
                    };
                    let load = self.compute_load(d, s, roads) +
                        self.compute_load(d2, s2, roads);

                    if load > max_load {
                        max_load = load;
                        max_pair = ((d, s), (d2, s2));
                    }
                }
            }
        }

        let ((d1, s1), (d2, s2)) = max_pair;
        if self.roads_arriving[d1][s1].is_some() {
            roads[self.roads_arriving[d1][s1].unwrap()].enable();
        }
        if self.roads_arriving[d2][s2].is_some() {
            roads[self.roads_arriving[d2][s2].unwrap()].enable();
        }
    }

    /// Computes the load of a road, i.e. the numbers of cars on this road.
    /// If there is no car ready to cross, returns 0.
    fn compute_load(&self, direction: usize, side: usize, roads: &mut Vec<Road>) -> i32 {
        let r = self.roads_arriving[direction][side];
        if r.is_none() || !roads[r.unwrap()].is_waiting() {
            return 0;
        }
        return roads[r.unwrap()].get_car_count();
    }
}


impl Network {
    /// Creates a new empty Network, with specified width and heights.
    pub fn new(width: usize, height: usize) -> Network {
        Network {
            width,
            height,
            car_count: 0,
            cars_per_unit: 10,
            cars_per_crossroad: 4,
            grid: none_array(height, width),
            roads: vec!(),
            graph: Graph::new(),
            car_graph: None,
            crossroads: vec!(),
        }
    }

    /// Adds a crossroad to specified location.
    pub fn add_crossroad(&mut self, x: usize, y: usize) {
        let c = CrossroadId::new(x, y);
        // We check the crossroad does not exist.
        self.assert_crossroad_not_exists(c);

        // We add it to the graph and update the network.
        self.grid[c] = Some(Crossroad::new(c, &mut self.graph));
        self.crossroads.push(c);
    }

    /// Adds a new specific road.
    pub fn new_road(&mut self, src: CrossroadId, dest: CrossroadId, side: Side){
        // We get the parameters of the road.
        let (dx, dy, length) = src.join(dest);
        let length = length * self.cars_per_unit - self.cars_per_crossroad;
        let (d1, d2) = compute_directions(dx, dy, side);
        let id = self.roads.len();

        // First, it builds the road in the network.
        let road_info = RoadInfo {
            id,
            start: src,
            end: dest,
            side,
            destination: self.crossroad(dest).nodes[d2],
            length: length as usize,
        };

        // Then, we add it to the crossroads and the roads.
        let road = Road::new(road_info);
        self.roads.push(road);
        self.crossroad_mut(src).roads[d1][side] = Some(id);
        self.crossroad_mut(dest).roads_arriving[d1][side] = Some(id);

        // Then, it builds the two corresponding edges in the graph.
        let (n1, n2) = {
            let c = self.crossroad(src);
            (c.nodes[d1], c.nodes[previous_direction(d1)])
        };
        let n3 = self.crossroad(dest).nodes[d2];

        self.graph.add_edge(n1, n3, id);
        self.graph.add_edge(n2, n3, id);
    }

    /// Add the two road linking the first crossroad to the second one.
    pub fn add_road(&mut self, (src_x, src_y): (usize, usize), (dest_x, dest_y): (usize, usize)) {
        let (src, dest) =
            (CrossroadId::new(src_x, src_y), CrossroadId::new(dest_x, dest_y));

        // Checks the source and destination crossroads exist.
        self.assert_crossroad_exists(src);
        self.assert_crossroad_exists(dest);

        // Checks that they are aligned.
        let (dx, dy, length) = src.join(dest);

        // Checks that the road can be built between the two crossroads, i.e. that it does not
        // generate any collision.
        for k in 1..length {
            self.assert_crossroad_not_exists(src + (k*dx, k*dy));
        }

        // Creates both roads.
        self.new_road(src, dest, LEFT);
        self.new_road(src, dest, RIGHT);
    }

    /// Adds all roads between the crossroads `c1` and `c2`.
    pub fn add_all_roads(&mut self, c1: (usize, usize), c2: (usize, usize)) {
        self.add_road(c1, c2);
        self.add_road(c2, c1);
    }

    /// Panics if the crossroad exists.
    pub fn assert_crossroad_exists(&self, c: CrossroadId) {
        if self.grid[c].is_none() {
            panic!("This crossroad {} does not exist.", c);
        }
    }

    /// Panics if the crossroad does not exist.
    pub fn assert_crossroad_not_exists(&self, c: CrossroadId) {
        if self.grid[c].is_some() {
            panic!("This crossroad {} already exists.", c);
        }
    }

    /// Retrieves the specified crossroad. Panics if it does not exist.
    pub fn crossroad(&self, c: CrossroadId) -> &Crossroad {
        self.grid[c].as_ref().unwrap()
    }

    /// Retrieves a mutable reference to the specified crossroad. Panics if it does not exist.
    pub fn crossroad_mut(&mut self, c: CrossroadId) -> &mut Crossroad {
        self.grid[c].as_mut().unwrap()
    }

    /// Creates a new car. It transfers the current graph to the car, with a fresh identifier.
    pub fn create_car(&mut self) -> Car {
        if self.car_graph.is_none() {
            // If needed, we generate this shared reference.
            self.car_graph = Some(Arc::new(self.clone_graph()));
        }
        let id = self.car_count;
        self.car_count += 1;

        Car::new(id, 0, CrossroadId::new(0, 0), self.car_graph.clone().unwrap())
    }

    /// Spawns a car on a random road, and finds a random destination.
    pub fn generate_request(&mut self, id: CarId) -> (RoadInfo, usize, CrossroadId) {
        // First, it finds a road to spawn the car.
        let mut rng = rand::thread_rng();
        let mut road_id = rng.gen_range(0, self.roads.len());

        let mut pos = self.roads[road_id].spawn_car(id);
        while pos == -1 {
            road_id = rng.gen_range(0, self.roads.len());
            pos = self.roads[road_id].spawn_car(id);
        }

        // Then, it gets the crossroad at the end of this road.
        let road_info = self.roads[road_id].info();
        let source_c = road_info.end;

        // It randomly chooses a crossroad different from the previous crossroad.
        let mut destination = self.random_crossroad();
        while destination == source_c {
            destination = self.random_crossroad();
        }

        // Returns the final spawn position and destination.
        (road_info, pos as usize, destination)
    }

    /// Spawns all the car that requested to be. Updates the move vector with the resulting spawns.
    pub fn spawn_cars(&mut self, actions: Vec<Action>, moves: &mut Vec<Move>) {
        for (i, a) in actions.iter().enumerate() {
            if let Action::SPAWN = *a {
                let (road_info, pos, destination) = self.generate_request(i);
                moves[i] = Move::SPAWN(road_info, pos, destination);
            }
        }
    }

    /// Makes the crossroads enable some roads.
    pub fn enable_paths(&mut self) {
        for &c in &self.crossroads {
            self.grid[c].as_ref().unwrap().enable_path(&mut self.roads);
        }
    }

    /// Performs an update step on all roads, based on the Actions and Speeds vector.
    /// Updates the resulting Moves vector, and returns the EdgesWeight estimation.
    pub fn roads_step(&mut self, actions: &mut Vec<Action>, moves: &mut Vec<Move>, speeds: &Vec<Speed>)
                   -> EdgesWeight
    {
        let roads = &mut self.roads;

        // All the possibles enabled paths are tried.
        for i in 0..roads.len() {
            // Each roads tries to make its first car cross, if enabled.
            Road::deliver(i, actions, moves, roads);
        }

        // We make a step for all remaining cars, and get the weights estimations.
        let mut weights = vec!();
        for i in 0..roads.len() {
            weights.push(roads[i].step_forward(moves, speeds));
        }
        let edges_weight = EdgesWeight::new(weights);

        return edges_weight
    }

    /// Returns the central reactive process of the network.
    pub fn process(mut self, central_signal: SPMCSignalSender<Arc<GlobalInfo>>,
                   pos_signal: MPSCSignalReceiver<(CarId, (Action, Speed)), (Vec<Action>, Vec<Speed>)>)
                   -> impl Process<Value=()> {

        let mut weights = vec!();
        for r in &self.roads {
            weights.push(r.weight());
        }

        let mut step = 0;
        let mut mean_moves = self.car_count as f32;
        let beta = 0.99;

        let cont = move | (mut actions, speeds): (Vec<Action>, Vec<Speed>) | {
            // We count the steps.
            step += 1;

            // We enable some path.
            self.enable_paths();

            // We compute the road step and get back some weights.
            let mut moves = (0..actions.len()).map(|_| { Move::NONE }).collect();
            let weights = self.roads_step(&mut actions, &mut moves, &speeds);

            // We spawn the cars that requested to be.
            self.spawn_cars(actions, &mut moves);

            // We count the number of cars that did something.
            let nb_moves: i32 = moves.iter().map(| m | { match m {
                &Move::NONE => 0,
                _ => 1,
            }}).sum();

            // We keep some moving mean of this number. If it is too low, nothing is happening, so
            // it panics.
            mean_moves = beta * mean_moves + (1. - beta) * (nb_moves as f32);
            if mean_moves < 1e-3 {
                panic!("It looks like a stationary state: not enough moves.");
            }

            // Returns the updated information about the step.
            Arc::new(GlobalInfo { weights, moves })
        };

        let p =
            pos_signal.await_in()                   // Awaits the car actions
                .map(cont)                          // Computes the resulting moves and weights.
                .emit_consume(central_signal)       // Emits this information.
                .loop_inf();                        // Loops.
        return p;
    }

    /// Returns a String representing the network.
    pub fn to_string(&self) -> String {
        // We first build the corresponding char two-dimensional vector.
        let (width, height) = (2 * self.width - 1, 2 * self.height - 1);
        let mut char_map: Vec<Vec<char>> = (0..height).map(|_| { (0..width).map(|_| { ' ' }).collect()}).collect();

        // Then we add the crossroads.
        for c in &self.crossroads {
            char_map[2 * c.y][2 * c.x] = 'C';
        }

        // Then we add the roads.
        for r in &self.roads {
            let start = r.info().start;
            let (dx, dy, length) = start.join(r.info().end);

            // Chooses the right symbol.
            let c = if dx == 0 { '|' } else { '-' };
            let (x, y) = (2*start.x, 2*start.y);

            for k in 1..(2*length) {
                char_map[(y as i32 + k * dy) as usize][(x as i32 + k * dx) as usize] = c;
            }
        }

        // We collect the characters into a string.
        char_map.into_iter().map(|line| { line.into_iter().collect::<String>().add("\n") }).collect()
    }

    /// Loads a network from a file located in trafficsim/maps/.
    pub fn load_file(&mut self, filename: &str) {
        let mut f = File::open(format!("./src/trafficsim/maps/{}", filename)).expect("File not found");

        let mut contents = String::new();
        f.read_to_string(&mut contents)
            .expect("Something went wrong reading the file");

        self.load_string(&contents);
    }

    /// Loads a network from a string.
    pub fn load_string(&mut self, s: &str) {
        // We remove ending blank lines.
        let s = s.trim_right();

        // We split lines and remove ending spaces and `\n`.
        let mut char_map: Vec<Vec<char>> =
            s.split("\n")
                .map(| line | { line.trim_right().chars().collect() })
                .collect();

        // We compute the resulting width and height of the character array.
        let width = char_map.iter().map(| line | { line.len() }).max().unwrap();
        let height = char_map.len();

        // We add missing spaces.
        for line in char_map.iter_mut() {
            for _ in 0..(width - line.len()) {
                line.push(' ');
            }
        }

        // We change the network size.
        *self = Network::new((width + 1) / 2, (height + 1) / 2);

        // Then, we add all the crossroads.
        for (j, line) in char_map.iter().enumerate() {
            for (i, c) in line.iter().enumerate() {
                if *c == 'C' {
                    self.add_crossroad(i / 2, j / 2);
                }
            }
        }

        // Then we add the horizontal roads.
        for (j, line) in char_map.iter().enumerate() {
            let mut last_crossroad = None;
            let mut road_length = 0;
            for (i, c) in line.iter().enumerate() {
                if *c == 'C' {
                    if last_crossroad.is_some() && road_length > 0 {
                        self.add_all_roads(last_crossroad.unwrap(), (i / 2, j / 2));
                    }
                    last_crossroad = Some((i / 2, j / 2));
                    road_length = 0;
                }
                else if *c == '-' {
                    if last_crossroad.is_none() {
                        panic!("Invalid road at position ({}, {}): no crossroad to join.", i, j);
                    }
                    else {
                        road_length += 1;
                    }
                }
                else {
                    if road_length > 0 {
                        panic!("Invalid road at position ({}, {}): no crossroad to join.", i, j);
                    }
                    last_crossroad = None;
                }
            }
        }

        // Then we add the vertical roads.
        for i in 0..width {
            let mut last_crossroad = None;
            let mut road_length = 0;
            for j in 0..height {
                let c = char_map[j][i];
                if c == 'C' {
                    if last_crossroad.is_some() && road_length > 0 {
                        self.add_all_roads(last_crossroad.unwrap(), (i / 2, j / 2));
                    }
                    last_crossroad = Some((i / 2, j / 2));
                    road_length = 0;
                }
                    else if c == '|' {
                        if last_crossroad.is_none() {
                            panic!("Invalid road at position ({}, {}): no crossroad to join.", i, j);
                        }
                            else {
                                road_length += 1;
                            }
                    }
                        else {
                            if road_length > 0 {
                                panic!("Invalid road at position ({}, {}): no crossroad to join.", i, j);
                            }
                            last_crossroad = None;
                        }
            }
        }
    }

    /// Returns the cloned graph.
    pub fn clone_graph(&self) -> Graph {
        self.graph.clone()
    }

    /// Returns random crossroad coordinates (of an existing crossroad).
    pub fn random_crossroad(&self) -> CrossroadId {
        let i = rand::thread_rng().gen_range(0, self.crossroads.len());
        self.crossroads[i]
    }

    /// Removes unused roads, i.e. dead ends.
    pub fn simplify(&mut self) {
        println!("The network has {} crossroads and {} roads.",
                 self.crossroads.len(), self.roads.len());

        // First, we identify nodes that have no escape.
        let dead_ends: Vec<bool> = self.graph.nodes.iter().map(| n | {
            n.edges().is_empty()
        }).collect();

        // Then, we mark all the roads that do not end in a dead end.
        let used_roads: Vec<bool> = self.roads.iter().map(|r| {
            !dead_ends[r.info().destination]
        }).collect();

        // We create a fresh network.
        let mut network = Network::new(self.width, self.height);

        // Then, we add all the interesting crossroads, i.e. that don't have 4 dead end nodes.
        for &c in &self.crossroads {
            let c = self.crossroad(c);
            if c.nodes.iter()
                .map(|id| { !dead_ends[*id] })
                .fold(false, |x, y| { x || y }) {
                network.add_crossroad(c.id.x, c.id.y);
            }
        }

        // Finally, we add only the used edges.
        for r in &self.roads {
            let r = r.info();
            if used_roads[r.id] {
                network.new_road(r.start, r.end, r.side);
            }
        }

        // We change the initial network.
        *self = network;
        println!("After simplification, it only has {} crossroads and {} roads.",
                 self.crossroads.len(), self.roads.len());
    }
}

impl fmt::Display for Network {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.to_string())
    }
}

/// Returns an array of size `height` x `width`.
pub fn none_array<T>(height: usize, width: usize) -> Vec<Vec<Option<T>>> {
    (0..height).map(|_| { (0..width).map(|_| { None }).collect()}).collect()
}

/// Computes the road direction and its node direction.
pub fn compute_directions(dx: i32, dy: i32, side: Side) -> (usize, usize) {
    let d1 = match (dx, dy) {
        (1, 0)  => EAST,
        (0, 1)  => SOUTH,
        (-1, 0) => WEST,
        (0, -1) => NORTH,
        _       => panic!("Invalid direction."),
    };

    let d2 = (d1 + (1-side) * 2) % 4;
    (d1, d2)
}

/// Returns the previous (clockwise) direction.
pub fn previous_direction(d: usize) -> usize {
    (d + 3) % 4
}