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
    pub length: usize,
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

    pub fn spawn_car(&mut self, id: CarId) -> i32 {
        for (i, place) in self.queue.iter_mut().enumerate() {
            if place.is_none() {
                *place = Some(id);
                self.car_count += 1;
                return i as i32;
            }
        }
        return -1;
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

    pub fn step_forward(&mut self, moves: &mut Vec<Move>, speeds: &Vec<Speed>) -> Weight {
        // The speed can increase at most by speed_increase per cycle.
        let speed_increase = 3;

        // The speed cannot decrease more than speed_decrease per cycle.
        let speed_decrease = 2;

        let mut free_space= 0;
        let mut last_speed = 0;
        if self.queue[0].is_none() {
            free_space += 1;
            last_speed = 1;
        }
        for i in 1..(self.last_index+1) {
            if self.queue[i].is_some() {
                if last_speed > 0 {
                    if (i == self.last_index) && self.new_guy {
                        // The car was just added to the end of the queue.
                        break;
                    }
                    let id = self.queue[i].unwrap();
                    // We compute the length of the step.

                    let step = last_speed.min(speeds[id] + speed_increase);

                    if self.queue[i - step].is_some() {
                        panic!("Just overwrote some car!");
                    }
                    self.queue[i - step] = self.queue[i];
                    self.queue[i] = None;
                    moves[id] = Move::STEP(step as i32);
                    free_space = 0;
                }
                else {
                    free_space = 0;
                }
            }
            else {
                free_space += 1;
                if free_space >= last_speed / speed_decrease + 1 {
                    free_space = 0;
                    last_speed += 1;
                }
            }
        }

        self.update_status();

        self.weight()
    }

    pub fn is_waiting(&self) -> bool {
        self.queue[0].is_some()
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
            Action::SPAWN => {
                panic!("A car on a road cannot be teleported.");
            }
        }
    }
}

pub fn update_flow(average_flow: f32, has_moved: bool) -> f32 {
    let alpha = 0.5;
    let new = if has_moved { 1. } else { 0. };
    let new_value = alpha * average_flow + (1. - alpha) * new;

    if new_value < 1e-14 {
//        println!("OVERFLOW");
    }

    f32::max(new_value, 1e-12)
}

pub fn compute_weight(average_flow: f32, length: f32, car_count: i32) -> Weight {
    length.max(car_count as f32 / average_flow)
}