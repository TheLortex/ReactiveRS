use super::car::*;
use super::graph::*;
use super::network::*;

pub type RoadId = EdgeInfo;

#[derive(Copy, Clone)]
pub struct RoadInfo {
    pub id: RoadId,
    pub start: CrossroadId,
    pub end: CrossroadId,
    pub side: Side,
    pub destination: NodeId,
}

#[derive(Clone)]
pub struct Road {
    info: RoadInfo,

    length: f32,
    queue: Vec<Option<CarId>>,
    last_index: usize,
    average_flow: f32,
    car_count: i32,

    new_guy: bool,
    has_moved: bool,
    enabled: bool,
}

impl Road {
    pub fn new(info: RoadInfo, length: i32) -> Road {
        Road {
            info,

            length: length as f32,
            queue: (0..length).map(|_| { None }).collect(),
            average_flow: 1.,
            car_count: 0,
            last_index: (length - 1) as usize,

            new_guy: false,
            has_moved: false,
            enabled: false,
        }
    }

    pub fn available(&self) -> bool {
        self.queue[self.last_index].is_none()
    }

    pub fn enable(&mut self) {
        self.enabled = true;
    }

    pub fn get_car_count(&self) -> i32 {
        self.car_count
    }

    pub fn info(&self) -> RoadInfo {
        self.info
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

    pub fn spawn_car(&mut self, id: CarId) -> bool {
        for place in &mut self.queue {
            if place.is_none() {
                *place = Some(id);
                self.car_count += 1;
                return true;
            }
        }
        return false;
    }

    pub fn update_status(&mut self) {
        self.average_flow = update_flow(self.average_flow, self.has_moved);
        self.new_guy = false;
        self.has_moved = false;
        self.enabled = false;
    }

    pub fn weight(&self) -> Weight {
        compute_weight(self.average_flow, self.length, self.car_count)
    }

    pub fn step_forward(&mut self, moves: &mut Vec<Move>) -> Weight {
        for i in 1..self.last_index {
            if self.queue[i].is_some() && self.queue[i-1].is_none() {
                self.queue[i - 1] = self.queue[i];
                self.queue[i] = None;
                moves[self.queue[i - 1].unwrap()] = Move::STEP(1);
            }
        }
        if !self.new_guy && self.queue[self.last_index].is_some() && self.queue[self.last_index-1].is_none() {
            let i = self.last_index;
            self.queue[i-1] = self.queue[i];
            moves[self.queue[i - 1].unwrap()] = Move::STEP(1);
            self.queue[i] = None;
        }

        self.update_status();

        self.weight()
    }

    pub fn deliver(i: RoadId, actions: &mut Vec<Action>, moves: &mut Vec<Move>, roads: &mut Vec<Road>) {
        if roads[i].queue[0].is_none() || !roads[i].enabled {
            return;
        }

        let car = roads[i].queue[0].unwrap();

        // We assume the action is valid.
        match actions[car] {
            Action::VANISH => {
//                println!("Car {} vanishes.", car);
                roads[i].pop();
                moves[car] = Move::VANISH;
            },
            Action::CROSS(j) => {
                if roads[j].add(car) {
                    roads[i].pop();
                    moves[car] = Move::CROSS(roads[j].info.clone());
                }
            },
        }
    }
}

pub fn update_flow(average_flow: f32, has_moved: bool) -> f32 {
    let alpha = 0.95;
    let new = if has_moved { 1. } else { 0. };
    let new_value = alpha * average_flow + (1. - alpha) * new;

    if new_value < 1e-14 {
//        println!("OVERFLOW");
    }

    f32::max(new_value, 1e-10)
}

pub fn compute_weight(average_flow: f32, length: f32, car_count: i32) -> Weight {
    length.max(car_count as f32 / average_flow)
}