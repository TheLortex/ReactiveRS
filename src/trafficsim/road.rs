use super::car::*;
use super::network::*;

pub type RoadId = EdgeInfo;

pub struct Road {
    id: RoadId,
    queue: Vec<Option<CarId>>,
    new_guy: bool,
    has_moved: bool,
    length: f32,
    average_flow: f32,
    car_count: i32,
    last_index: usize,
}

impl Road {
    pub fn new(id: RoadId, length: i32) -> Road {
        Road {
            id,
            queue: (0..length).map(|_| { None }).collect(),
            new_guy: false,
            has_moved: false,
            length: length as f32,
            average_flow: 1.,
            car_count: 0,
            last_index: (length - 1) as usize,
        }
    }

    pub fn available(&self) -> bool {
        self.queue[self.last_index].is_none()
    }

    pub fn add(&mut self, car: CarId) -> bool {
        if !self.available() {
            false
        }
        else {
            self.queue[self.last_index] = Some(car);
            self.car_count += 1;
            self.new_guy = true;

            true
        }
    }

    pub fn pop(&mut self) {
        self.queue[0] = None;
        self.car_count -= 1;
        self.has_moved = true;
    }

    pub fn step_forward(&mut self, positions: &mut Vec<Position>) -> Weight {
        for i in 1..self.last_index {
            if self.queue[i].is_some() && self.queue[i-1].is_none() {
                self.queue[i - 1] = self.queue[i];
                self.queue[i] = None;
                positions[self.queue[i - 1].unwrap()].rank = i-1;
            }
        }
        if !self.new_guy && self.queue[self.last_index].is_some() && self.queue[self.last_index-1].is_none() {
            let i = self.last_index;
            self.queue[i-1] = self.queue[i];
            positions[self.queue[i - 1].unwrap()].rank = i-1;
            self.queue[i] = None;
        }
        self.new_guy = false;
        self.average_flow = update_flow(self.average_flow, self.has_moved);
        compute_weight(self.average_flow, self.length, self.car_count)
    }

    pub fn deliver(i: RoadId, j: RoadId, positions: &mut Vec<Position>, roads: &mut Vec<Road>) {
        if roads[i].queue[0].is_none() || roads[i].has_moved {
            return;
        }

        let car = roads[i].queue[0].unwrap();
        let mut pos = &mut positions[car];
        if pos.direction == roads[j].id {
            if roads[j].add(car) {
                roads[i].pop();
                pos.road = roads[j].id;
                pos.rank = roads[j].last_index;
                pos.has_moved = true;
            }
        }
    }
}

pub fn do_step(roads: &mut Vec<Road>, enabled_paths: Vec<Vec<RoadId>>, positions: &mut Vec<Position>)
    -> EdgesWeight
{
    // All the possibles enabled paths are tried.
    for i in 0..roads.len() {
        for &j in &enabled_paths[i] {
            Road::deliver(i, j, positions, roads);
        }
    }

    // We make a step for all remaining cars.
    let mut weights = vec!();
    for i in 0..roads.len() {
        weights.push(roads[i].step_forward(positions));
    }
    let mut edges_weight = EdgesWeight::new(weights.len());
    edges_weight.set_weights(weights);

    return edges_weight
}

pub fn update_flow(average_flow: f32, has_moved: bool) -> f32 {
    let alpha = 0.95;
    let new = if has_moved { 1. } else { 0. };
    alpha * average_flow + (1. - alpha) * new
}

pub fn compute_weight(average_flow: f32, length: f32, car_count: i32) -> Weight {
    length.max(car_count as f32 / average_flow)
}