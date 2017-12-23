use super::car::*;
use super::graph::*;
use super::network::*;

/// Road identifier.
pub type RoadId = usize;

/// Road information.
#[derive(Copy, Clone)]
pub struct RoadInfo {
    pub id: RoadId,             // Road identifier.
    pub start: CrossroadId,     // Starting crossroad coordinates.
    pub end: CrossroadId,       // Ending crossroad coordinates.
    pub side: Side,             // Side of the road (RIGHT is the outermost road.
    pub destination: NodeId,    // Destination crossroad node.
    pub length: usize,          // Length of the road, i.e. number of cars fitting in the road.
}

/// A simple road.
#[derive(Clone)]
pub struct Road {
    info: RoadInfo,             // Road information.

    queue: Vec<Option<CarId>>,  // Vector of cars present on this road.
    last_index: usize,          // Last place on this road.
    average_flow: f32,          // Average number of cars leaving the road per instant.
    car_count: i32,             // Number of cars on the road.

    new_guy: bool,              // Indicates if a new car arrived on the road at the current step.
    has_moved: bool,            // Indicates if a car left the road at the current step.
    enabled: bool,              // Indicates if cars from the road can leave the road at this step.
}

impl Road {
    /// Creates a new road.
    pub fn new(info: RoadInfo) -> Road {
        Road {
            info,

            queue: (0..info.length).map(|_| { None }).collect(),
            average_flow: 1.,
            car_count: 0,
            last_index: (info.length - 1) as usize,

            new_guy: false,
            has_moved: false,
            enabled: false,
        }
    }

    /// Returns true if the last place of the road is free.
    pub fn available(&self) -> bool {
        self.queue[self.last_index].is_none()
    }

    /// Enables the road for this step.
    pub fn enable(&mut self) {
        self.enabled = true;
    }

    /// Returns the number of cars on this road.
    pub fn get_car_count(&self) -> i32 {
        self.car_count
    }

    /// Returns the road information.
    pub fn info(&self) -> RoadInfo {
        self.info
    }

    /// Tries to add car `car` at the end of the road.
    /// Returns `true` if it succeeded, `false` otherwise.
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

    /// Releases the first car of the road (i.e. the crossing car).
    pub fn pop(&mut self) {
        self.queue[0] = None;
        self.car_count -= 1;
        self.has_moved = true;
    }

    /// Spawns a car on this road, at the first free location. Returns the chosen position.
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

    /// Updates the average flow and resets the status of the road.
    /// This has to be done after each end of step.
    pub fn update_status(&mut self) {
        self.average_flow = update_flow(self.average_flow, self.has_moved, self.queue[0].is_none());
        self.new_guy = false;
        self.has_moved = false;
        self.enabled = false;
    }

    /// Returns the estimated weight of the road.
    pub fn weight(&self) -> Weight {
        compute_weight(self.average_flow, self.info.length as f32, self.car_count)
    }

    /// Performs a step on all possible cars on the road, returns the updated weight estimation,
    /// resets the status of the road.
    pub fn step_forward(&mut self, moves: &mut Vec<Move>, speeds: &Vec<Speed>) -> Weight {
        // The speed can increase at most by speed_increase per cycle.
        let speed_increase = 3;

        // The speed cannot decrease more than speed_decrease per cycle.
        let speed_decrease = 2;

        // The two following variables are a bit technical.
        // They allow to compute the maximum allowed speed of the next car to ensure that the car
        // never decreases its speed more than the threshold in a row.
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

                    // We compute the length of the step, based on maximal allowed speed and based
                    // on the previous speed of the car.
                    let step = last_speed.min(speeds[id] + speed_increase);

                    // If there was some error, panics.
                    if self.queue[i - step].is_some() {
                        panic!("Just overwrote some car!");
                    }

                    // Otherwise, updates the car location.
                    self.queue[i - step] = self.queue[i];
                    self.queue[i] = None;

                    // Adds the move.
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

    /// Indicates if some car wants to cross at this road.
    pub fn is_waiting(&self) -> bool {
        self.queue[0].is_some()
    }

    /// Tries to do a step on the first car of the road.
    /// If there is no car ready to cross, does nothing.
    pub fn deliver(i: RoadId, actions: &mut Vec<Action>, moves: &mut Vec<Move>, roads: &mut Vec<Road>) {
        if roads[i].queue[0].is_none() || !roads[i].enabled {
            // Nothing to do.
            return;
        }

        let car = roads[i].queue[0].unwrap();

        // We assume the action the car asked is valid.
        match actions[car] {
            Action::VANISH => {
                // We remove the car and updates the move.
                roads[i].pop();
                moves[car] = Move::VANISH;
            },
            Action::CROSS(j) => {
                // If the destination road accepts the car, we transfer it.
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

/// Returns the updated average flow.
pub fn update_flow(average_flow: f32, has_moved: bool, is_no_one: bool) -> f32 {
    // First, if no car tried to cross, we don't change anything.
    if !has_moved && is_no_one {
        return average_flow;
    }

    // Otherwise, we update the moving flow average.
    let alpha = 0.95;
    let new = if has_moved { 1. } else { 0. };
    let new_value = alpha * average_flow + (1. - alpha) * new;

    // We add some minimum threshold to avoid weights to go to infinity.
    f32::max(new_value, 1e-12)
}

/// Returns the estimation of the real length of the road.
pub fn compute_weight(average_flow: f32, length: f32, car_count: i32) -> Weight {
    length.max(car_count as f32 / average_flow)
}