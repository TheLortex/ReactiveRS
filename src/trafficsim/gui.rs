extern crate reactivers;

use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use std;
extern crate rand;

use super::network::*;
use super::car::*;
use super::road::*;

use super::opengl_graphics::GlGraphics;
use super::piston::event_loop::*;
use super::piston::input::*;
use super::piston::window::WindowSettings;
use super::sdl2_window::{Sdl2Window, OpenGL};
use super::graphics::*;


/// Display for the Traffic Simulation
pub struct Gui {
    size_cell: f64,                         // Cell size (distance between crossroads)
    size_crossroad: f64,                    // Crossroad size
    car_place_width: f64,                   // Width occupied by a car.
                                            // Determines the space between cars.
    width: f64,                             // Width of the window.
    height: f64,                            // Height of the window.
    crossroad_rect: Rectangle,              // Crossroad rectangle color.
    crossroads: Vec<CrossroadId>,           // Vector of the crossroad coordinate.
    roads: Vec<RoadInfo>,                   // Vector of the road information.
    cars: Vec<Option<(RoadInfo, usize)>>,   // Vector of the car positions (RoadInfo, position).
    data: Arc<Mutex<Option<Vec<Move>>>>,    // Shared data used to transfer from reactive process.
    car_rectangle: [f64; 4],                // Car rectangle.
    car_animations: Vec<Animation>,         // Vector of animations.
    animation_duration: f64,                // Animation duration in seconds.
}

/*
        Animation definitions
    These animations define several moves that can be executed step by step to display all the
    car animations simultaneously.
*/

/// Animation Step type
/// It is a function that takes a progress (between 0 and 1), a Context and draws the corresponding
/// step on the specified GlGraphics object.
type AnimationStep = Box<Fn(f64, Context, &mut GlGraphics) -> () + 'static>;

/// An Animation
pub struct Animation {
    start: f64,             // Start time of the animation.
    duration: f64,          // Duration of the animation (in seconds).
    step: AnimationStep,    // Animation Step.
}

/// A transformation is some object that can apply a transformation to a given Context.
trait Transformation {
    /// Applies the transformation corresponding to the specified time `t` to the given `cont`.
    fn transform(&self, t: f64, cont: Context) -> Context;
}

/// A simple trajectory.
pub enum Trajectory {
    CIRCLE {            // A simple arc of circle.
        radius: f64,    // Circle radius.
                        // If it is positive, arc of circle turns right, left otherwise.
        angle: f64,     // Angle.
    },
    LINE (f64),         // A simple line with specified length.
}

impl Trajectory {
    /// Creates a circle. If the angle is positive, arc of circle turns right, left otherwise.
    pub fn circle(radius: f64, angle: f64) -> Trajectory {
        let dir = if angle < 0. { -1. } else { 1. };
        Trajectory::CIRCLE { radius: dir * radius, angle }
    }

    /// Creates a line.
    pub fn line(length: f64) -> Trajectory {
        Trajectory::LINE(length)
    }

    /// Returns the length of the trajectory.
    pub fn length(&self) -> f64 {
        match self {
            &Trajectory::LINE (length) => length,
            &Trajectory::CIRCLE { radius, angle} => radius * angle * std::f64::consts::PI / 180.
        }
    }
}

impl Transformation for Trajectory {
    fn transform(&self, t: f64, cont: Context) -> Context {
        match self {
            &Trajectory::LINE (length) => cont.trans(t * length, 0.),
            &Trajectory::CIRCLE { radius, angle} => cont.trans(0., radius).rot_deg(t*angle).trans(0., -radius),
        }
    }
}

/// A combination of Trajectories.
pub struct MultiTrajectory {
    trajectories: Vec<Trajectory>,  // Vector of Trajectories.
    lengths: Vec<f64>,              // Vector of lengths of Trajectories.
    length: f64,                    // Total length.
}

impl MultiTrajectory {
    /// Creates an empty trajectory.
    pub fn new() -> MultiTrajectory {
        MultiTrajectory { trajectories: vec!(), lengths: vec!(), length: 0. }
    }

    /// Appends the specified Trajectory.
    pub fn add(&mut self, t: Trajectory) {
        let length = t.length();
        if length == 0. {
            return;
        }

        // Updates the parameters.
        self.trajectories.push(t);
        self.length += length;
        self.lengths.push(length);
    }
}

impl Transformation for MultiTrajectory {
    fn transform(&self, t: f64, cont: Context) -> Context {
        // The speed is assumed to be constant over the whole trajectory.
        let mut t = t * self.length;
        let mut cont = cont;
        for (i, traj) in self.trajectories.iter().enumerate() {
            if t < self.lengths[i] {
                // We apply partial i transformation.
                return traj.transform(t / self.lengths[i], cont);
            } else {
                // We apply full i transformation.
                cont = traj.transform(1., cont);
            }
            t -= self.lengths[i];
        }

        return cont;
    }
}

impl Animation {
    /// Creates a new animation.
    pub fn new(f: AnimationStep, time: f64, duration: f64) -> Animation {
        Animation { step: f,  start: time, duration }
    }

    /// Creates an empty animation with no transformation.
    pub fn unit() -> Animation {
        Animation { step: Box::new(| _, _, _ | {}), start: 0., duration: 1. }
    }


    /// Executes one step of the animation.
    pub fn step(&self, time: f64, c: Context, g: &mut GlGraphics) {
        // We normalize the time to get the progress of the animation.
        // We crop it if it is out of range [0., 1.].
        let t = f64::max(0., f64::min(1., (time - self.start) / self.duration));
        (self.step)(t, c, g);
    }
}

impl Gui {
    /// Creates a new GUI for the specified network.
    /// The network must already contain its cars.
    pub fn new(network: &Network, animation_duration: f64) -> Gui {
        let car_height = 4.;
        let car_width = 8.;
        let car_place_width = 10.;

        let width = network.width;
        let height = network.height;
        let size_cell = network.cars_per_unit as f64 * car_place_width;
        let size_crossroad = network.cars_per_crossroad as f64 * car_place_width;

        let crossroads = network.crossroads.clone();
        let roads = network.roads.iter().map(| r | { r.info() }).collect();
        Gui {
            size_cell,
            size_crossroad,
            car_place_width,
            width: (width - 1) as f64 * size_cell + size_crossroad,
            height: (height - 1) as f64 * size_cell + size_crossroad,
            crossroad_rect: Rectangle::new([0.5, 0.5, 0.5, 1.]),
            crossroads,
            roads,
            cars: (0..network.car_count).map(|_| { None }).collect(),
            data: Arc::new(Mutex::new(None)),
            car_rectangle: [
                (car_place_width - car_width) * 0.5, - car_height / 2., car_width, car_height,
            ],
            car_animations: (0..network.car_count).map(|_| { Animation::unit() }).collect(),
            animation_duration,
        }
    }

    /// Returns the crossroad pixel coordinates of the center of a crossroad.
    pub fn pos_crossroad(&self, c: CrossroadId) -> (f64, f64) {
        (self.size_crossroad / 2. + c.x as f64 * self.size_cell,
         self.size_crossroad / 2. + c.y as f64 * self.size_cell)
    }

    /// Returns the rectangle corresponding to the crossroad.
    pub fn crossroad_rect(&self, c: CrossroadId) -> [f64; 4] {
        [c.x as f64 * self.size_cell, c.y as f64 * self.size_cell, self.size_crossroad, self.size_crossroad]
    }

    /// Draws the corresponding crossroad.
    pub fn draw_crossroad(&self, c: CrossroadId, cont: Context, g: &mut GlGraphics) {
        self.crossroad_rect.draw(self.crossroad_rect(c),
                                 &cont.draw_state, cont.transform, g);
    }

    /// Draws all the crossroads.
    pub fn draw_crossroads(&self, cont: Context, g: &mut GlGraphics) {
        for &c in &self.crossroads {
            self.draw_crossroad(c, cont, g);
        }
    }

    /// Draws a road.
    pub fn draw_road(&self, r: RoadInfo, cont: Context, g: &mut GlGraphics) {
        // We compute the start coordinates.
        let (dx, dy, length) = r.start.join(r.end);
        let (dx, dy, length) = (dx as f64, dy as f64, length as f64);
        let length = length * self.size_cell - self.size_crossroad;

        let (mut x, mut y) = self.pos_crossroad(r.start);
        x += (dx / 2. - dy * r.side as f64 / 4.) * self.size_crossroad;
        y += (dy / 2. + dx * r.side as f64 / 4.) * self.size_crossroad;

        // We compute the orientation.
        let rot = match (dx as i32, dy as i32) {
            (1, 0)  => 0.,
            (0, 1)  => 1.,
            (-1, 0) => 2.,
            (0, -1) => 3.,
            _       => panic!("Invalid direction."),
        };
        let line_width1 = 2.;
        let line_width2 = 1.;
        let c = cont.trans(x, y).rot_deg( rot * 90.);

        let t1 = Context::new().trans(0., (r.side as f64) * self.size_crossroad / 4.).prepend_transform(c.transform);
        let t2 = Context::new().trans(0., -(r.side as f64) * self.size_crossroad / 4.).prepend_transform(c.transform);

        // The road.
        rectangle([0.8, 0.8, 0.8, 1.], [0., 0., length, self.size_crossroad / 4.],
                  c.transform, g);

        // Lines separating lanes.
        // External line (black one)
        if r.side == 0 {
            rectangle([0., 0., 0., 1.], [0., -line_width1 / 2., length, line_width1],
                      t1.transform, g);
        }

        // Internal line (lighter one).
        rectangle([0.5, 0.5, 0.5, 1.], [0., self.size_crossroad / 4. - line_width2 / 2., length,
            line_width2],
                  t2.transform, g);
    }

    /// Draws all the roads.
    pub fn draw_roads(&self, cont: Context, g: &mut GlGraphics) {
        for &r in &self.roads {
            self.draw_road(r, cont, g);
        }
    }

    /// Returns the animation step corresponding to a spawning car.
    pub fn spawn_car(&mut self, id: CarId, r: RoadInfo, pos: usize) -> AnimationStep {
        // We get the car and its position.
        self.cars[id] = Some((r, pos));
        let (x, y, angle) = self.car_position(id);

        // We define its trajectory.
        let r = self.car_rectangle;
        let radius = self.car_place_width;
        let shift = self.size_crossroad / 4.;

        let mut multi_traj = MultiTrajectory::new();
        multi_traj.add(Trajectory::line(shift));
        multi_traj.add(Trajectory::circle(radius, 90.));

        let f: Box<Fn(f64, Context, &mut GlGraphics)> = Box::new(move | t, cont, g | {
            let cont = cont.trans(x, y).rot_deg(angle)
                .trans(0., radius).rot_deg(-90.).trans(0., -radius)
                .trans(-shift, 0.);

            let cont = multi_traj.transform(t, cont);

            // We combine an increasing scaling to this trajectory, and a color change.
            rectangle([0., 1.-t as f32, t as f32, 1.], r, cont.scale(t, t).transform, g);
        });

        return f;
    }

    /// Returns the animation step for a car going forwards.
    pub fn step_car(&mut self, id: CarId, step: usize) -> AnimationStep {
        // We get its start and end coordinates.
        let (x1, y1, angle) = self.car_position(id);
        self.cars[id].as_mut().unwrap().1 -= step;
        let (x2, y2, _) = self.car_position(id);

        let r = self.car_rectangle;
        // We perform a simple interpolation.
        let f: Box<Fn(f64, Context, &mut GlGraphics)> = Box::new(move | t, cont, g | {
            rectangle([0., 0., 1., 1.],r, cont.trans((1.-t) * x1 + t * x2, (1.-t)*y1 + t*y2).rot_deg(angle).transform, g);
        });

        return f;
    }

    /// Returns the animation step for a car crossing a crossroad.
    pub fn cross_car(&mut self, id: CarId, info: RoadInfo) -> AnimationStep {
        // We compute the start coordinates and orientation.
        let side1 = self.cars[id].unwrap().0.side as f64;
        let (x1, y1, angle1) = self.car_position(id);

        // We update the car position.
        self.cars[id] = Some((info, info.length - 1));

        // We compute the end coordinates and orientation.
        let side2 = info.side as f64;
        let (x2, y2, angle2) = self.car_position(id);

        // We create an empty trajectory.
        let mut multi_traj = MultiTrajectory::new();

        if angle1 == angle2 {
            // The car goes straight forwards.
            // If the car changes its side, it uses two arcs of circle.
            let angle = (side2 - side1) * 90.;
            let radius = f64::abs(side1 - side2) * self.size_crossroad / 8.;
            let line = (self.size_crossroad + self.car_place_width - 2.*radius) / 2.;

            multi_traj.add(Trajectory::line(line));
            multi_traj.add(Trajectory::circle(radius, angle));
            multi_traj.add(Trajectory::circle(radius, -angle));
            multi_traj.add(Trajectory::line(line));

        } else if f64::abs(angle1 - angle2) == 180. {
            // The car turns back.
            // Its goes a bit forwards, turns back, and goes a bit forward again.
            // the radius changes depends on the start and end sides.
            let radius = self.size_crossroad / 8. * (1. + side1 + side2);
            let line = 1.5 * self.car_place_width;

            multi_traj.add(Trajectory::line(line));
            multi_traj.add(Trajectory::circle(radius, -180.));
            multi_traj.add(Trajectory::line(line - self.car_place_width));

        } else {
            // The car turns right or left.
            // We use a maximal arc of circle to perform the turn.
            let dx = f64::abs(x2 - x1);
            let dy = f64::abs(y2 - y1);
            let d1 = if angle1 == 0. || angle1 == 180. { dx } else { dy };
            let d2 = if angle1 == 0. || angle1 == 180. { dy } else { dx };
            let radius = f64::min(d1, d2);
            let angle = -(((angle2 - angle1) % 360. + 360.) % 360. - 180.);

            multi_traj.add(Trajectory::line(d1 - radius));
            multi_traj.add(Trajectory::circle(radius, angle));
            multi_traj.add(Trajectory::line(d2 - radius));
        }

        // Finally, we return the computed animation step.
        let r = self.car_rectangle;
        let f: Box<Fn(f64, Context, &mut GlGraphics)> = Box::new(move | t, cont, g | {
            rectangle([0., 0., 1., 1.], r, multi_traj.transform(t, cont.trans(x1, y1).rot_deg(angle1)).transform, g);
        });

        return f;
    }

    /// Returns the animation step corresponding to a vanishing car.
    pub fn vanish_car(&mut self, id: CarId) -> AnimationStep {
        let (x, y, angle) = self.car_position(id);
        let r = self.car_rectangle;

        let dx = self.size_crossroad;
        let f: Box<Fn(f64, Context, &mut GlGraphics)> = Box::new(move | t, cont, g | {
            // The car goes forward and progressively disappears.
            rectangle([t as f32, 0., 1.-t as f32, 1.],
                      r,
                      cont.trans(x, y).rot_deg(angle).trans(t * dx, 0.)
                          .scale(1.-t*t, 1.-t*t).transform,
                      g);
        });

        // We delete car position.
        self.cars[id] = None;

        f
    }

    /// Returns the animation step for a static car.
    pub fn static_car(&mut self, id: CarId) -> AnimationStep {
        // If the car does not exist, we return unit Animation Step.
        if self.cars[id].is_none() {
            return Box::new(|_, _, _| {} );
        }

        // Otherwise, we just compute the coordinates and the orientation of the car.
        let (x, y, angle) = self.car_position(id);
        let r = self.car_rectangle;

        let f: Box<Fn(f64, Context, &mut GlGraphics)> = Box::new(move | _, cont, g | {
            rectangle([0., 0., 1., 1.],r, cont.trans(x, y).rot_deg(angle).transform, g);
        });

        return f;
    }

    /// Updates the GUI move information from the data shared with the reactive process.
    pub fn update(&mut self, time: f64) {
        // We try to retrieve the data.
        let moves = {
            let mut moves = self.data.lock().unwrap();
            let mut new_moves = None;
            std::mem::swap(&mut new_moves, &mut *moves);
            new_moves
        };

        // If there is no change, we return.
        if moves.is_none() {
            return;
        }

        // Otherwise, we compute the new animations for each car.
        let moves = moves.unwrap();
        let duration = self.animation_duration;
        let animations = moves.iter().enumerate().map(|(i, m)| {
            let animation_step = match m {
                &Move::NONE => self.static_car(i),
                &Move::STEP(x) => self.step_car(i, x as usize),
                &Move::VANISH => self.vanish_car(i),
                &Move::CROSS(r) => self.cross_car(i, r),
                &Move::SPAWN(r, x, _) => self.spawn_car(i, r, x)
            };
            Animation::new(animation_step, time, duration)
        }).collect();

        self.car_animations = animations;
    }

    /// Draws all the cars with the saved animations.
    pub fn draw_cars(&mut self, time: f64, cont: Context, g: &mut GlGraphics) {
        for a in self.car_animations.iter() {
            let cont = cont;
            a.step(time, cont, g);
        }
    }

    /// Returns the coordinates and the orientation of a car.
    pub fn car_position(&self, id: CarId) -> (f64, f64, f64) {
        let (r, pos): (RoadInfo, usize) = self.cars[id].unwrap();

        // We first get the direction and lengths of the road.
        let (dx, dy, length) = r.start.join(r.end);
        let (dx, dy, length) = (dx as f64, dy as f64, length as f64);
        let length = length * self.size_cell - self.size_crossroad;

        // We compute the distance to the start of the road.
        let dist = length - (pos as f64 + 1.) * self.car_place_width;

        // Gets the start of the road.
        let (mut x, mut y) = self.pos_crossroad(r.start);
        x += (dx * 0.5 - dy * r.side as f64 / 4.) * self.size_crossroad;
        y += (dy * 0.5 + dx * r.side as f64 / 4.) * self.size_crossroad;

        // Centers the car on the road.
        x += -dy * self.size_crossroad / 8.;
        y +=  dx * self.size_crossroad / 8.;

        // Uses the index of the car on this road.
        x += dx * dist;
        y += dy * dist;

        // Computes orientation.
        let angle = match (dx as i32, dy as i32) {
            (1, 0)  => 0.,
            (0, 1)  => 1.,
            (-1, 0) => 2.,
            (0, -1) => 3.,
            _       => panic!("Invalid direction."),
        };

        return (x, y, angle*90.);
    }

    /// Returns a shared pointer to the transfer data.
    pub fn transfer_data(&self) -> Arc<Mutex<Option<Vec<Move>>>>
    {
        self.data.clone()
    }

    /// Launches the GUI and the simulation on the specified network and cars.
    pub fn run(&mut self, network: Network, cars: Vec<Car>)
    {
        // We initialize the simulation with the shared data and animation duration.
        let data = self.transfer_data();
        let duration = self.animation_duration;
        thread::spawn(move |  | {
            thread::sleep(Duration::from_millis(1000));
            super::run_simulation(network, cars, Some((duration, data)));
        });

        // We create a window.
        let opengl = OpenGL::V3_2;
        let (w, h) = (self.width as u32, self.height as u32);
        let mut window: Sdl2Window = WindowSettings::new("Traffic Simulation", [w, h])
            .exit_on_esc(true)
            .opengl(opengl)
            .build()
            .unwrap();

        let mut gl = GlGraphics::new(opengl);
        let mut events = Events::new(EventSettings::new());

        let mut time = 0.;
        self.update(time);

        while let Some(e) = events.next(&mut window) {
            if let Some(args) = e.render_args() {
                gl.draw(args.viewport(), |cont, g| {
                    clear([1., 1., 1., 1.0], g);

                    // We draw the whole Network.
                    self.draw_crossroads(cont, g);
                    self.draw_roads(cont, g);
                    self.draw_cars(time, cont, g);
                });
            }
            if let Some(args) = e.update_args() {
                // We update the time and try to retrieve the data.
                time += args.dt;
                self.update(time);
            }
        }
    }
}
