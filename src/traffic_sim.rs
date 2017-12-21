#![feature(conservative_impl_trait)]

extern crate reactivers;

use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
extern crate rand;

mod trafficsim;

use trafficsim::network::*;
use trafficsim::car::*;
use trafficsim::road::*;

extern crate graphics;
extern crate opengl_graphics;
extern crate piston;
extern crate sdl2_window;

use opengl_graphics::GlGraphics;
use piston::event_loop::*;
use piston::input::*;
use piston::window::WindowSettings;
use sdl2_window::{Sdl2Window, OpenGL};
use graphics::*;

pub struct Gui {
    size_cell: f64,
    size_crossroad: f64,
    car_place_width: f64,
    width: f64,
    height: f64,
    crossroad_rect: Rectangle,
    crossroads: Vec<CrossroadId>,
    roads: Vec<RoadInfo>,
    cars: Vec<Option<(RoadInfo, usize)>>,
    data: Arc<Mutex<Option<Vec<Move>>>>,
    car_rectangle: [f64; 4],
    car_animations: Vec<Animation>,
    animation_duration: f64,
}

type AnimationStep = Box<Fn(f64, Context, &mut GlGraphics) -> () + 'static>;

pub struct Animation {
    start: f64,
    duration: f64,
    step: AnimationStep,
}

trait Transformation {
    fn transform(&self, t: f64, cont: Context) -> Context;
}

pub enum Trajectory {
    CIRCLE {
        radius: f64,
        angle: f64,
    },
    LINE (f64),
}

impl Trajectory {
    pub fn circle(radius: f64, angle: f64) -> Trajectory {
        let dir = if angle < 0. { -1. } else { 1. };
        Trajectory::CIRCLE { radius: dir * radius, angle }
    }

    pub fn line(length: f64) -> Trajectory {
        Trajectory::LINE(length)
    }

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

pub struct MultiTrajectory {
    trajectories: Vec<Trajectory>,
    lengths: Vec<f64>,
    length: f64,
}

impl MultiTrajectory {
    pub fn new() -> MultiTrajectory {
        MultiTrajectory { trajectories: vec!(), lengths: vec!(), length: 0. }
    }

    pub fn add(&mut self, t: Trajectory) {
        let length = t.length();
        if length == 0. {
            return;
        }

        self.trajectories.push(t);
        self.length += length;
        self.lengths.push(length);
    }
}

impl Transformation for MultiTrajectory {
    fn transform(&self, t: f64, cont: Context) -> Context {
        let mut t = t * self.length;
        let mut cont = cont;
        for (i, traj) in self.trajectories.iter().enumerate() {
            if t < self.lengths[i] {
                return traj.transform(t / self.lengths[i], cont);
            }
            else {
                cont = traj.transform(1., cont);
            }
            t -= self.lengths[i];
        }

        return cont;
    }
}

impl Animation {
    pub fn new(f: AnimationStep, time: f64, duration: f64)
        -> Animation
    {
        Animation { step: f,  start: time, duration }
    }

    pub fn unit() -> Animation {
        Animation { step: Box::new(| _, _, _ | {}), start: 0., duration: 1. }
    }

    pub fn step(&self, time: f64, c: Context, g: &mut GlGraphics) {
        let t = f64::max(0., f64::min(1., (time - self.start) / self.duration));
        (self.step)(t, c, g);
    }
}

impl Gui {
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

    pub fn pos_crossroad(&self, c: CrossroadId) -> (f64, f64) {
        (self.size_crossroad / 2. + c.x as f64 * self.size_cell,
         self.size_crossroad / 2. + c.y as f64 * self.size_cell)
    }

    pub fn crossroad_rect(&self, c: CrossroadId) -> [f64; 4] {
        [c.x as f64 * self.size_cell, c.y as f64 * self.size_cell, self.size_crossroad, self.size_crossroad]
    }

    pub fn draw_crossroad(&self, c: CrossroadId, cont: Context, g: &mut GlGraphics) {
        self.crossroad_rect.draw(self.crossroad_rect(c),
                                 &cont.draw_state, cont.transform, g);
    }

    pub fn draw_crossroads(&self, cont: Context, g: &mut GlGraphics) {
        for &c in &self.crossroads {
            self.draw_crossroad(c, cont, g);
        }
    }

    pub fn draw_road(&self, r: RoadInfo, cont: Context, g: &mut GlGraphics) {
        let (dx, dy, length) = r.start.join(r.end);
        let (dx, dy, length) = (dx as f64, dy as f64, length as f64);
        let length = length * self.size_cell - self.size_crossroad;

        let (mut x, mut y) = self.pos_crossroad(r.start);
        x += (dx / 2. - dy * r.side as f64 / 4.) * self.size_crossroad;
        y += (dy / 2. + dx * r.side as f64 / 4.) * self.size_crossroad;

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

    pub fn draw_roads(&self, cont: Context, g: &mut GlGraphics) {
        for &r in &self.roads {
            self.draw_road(r, cont, g);
        }
    }

    pub fn spawn_car(&mut self, id: CarId, r: RoadInfo, pos: usize) -> AnimationStep {
        self.cars[id] = Some((r, pos));
        let (x, y, angle) = self.car_position(id);

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

            rectangle([0., 1.-t as f32, t as f32, 1.], r, cont.scale(t, t).transform, g);
        });

        return f;
    }

    pub fn step_car(&mut self, id: CarId, step: usize) -> AnimationStep {
        let (x1, y1, angle) = self.car_position(id);
        self.cars[id].as_mut().unwrap().1 -= step;
        let (x2, y2, _) = self.car_position(id);

        let r = self.car_rectangle;
        let f: Box<Fn(f64, Context, &mut GlGraphics)> = Box::new(move | t, cont, g | {
            rectangle([0., 0., 1., 1.],r, cont.trans((1.-t) * x1 + t * x2, (1.-t)*y1 + t*y2).rot_deg(angle).transform, g);
        });

        return f;
    }

    pub fn cross_car(&mut self, id: CarId, info: RoadInfo) -> AnimationStep {
        let side1 = self.cars[id].unwrap().0.side as f64;
        let (x1, y1, angle1) = self.car_position(id);
        self.cars[id] = Some((info, info.length - 1));
        let side2 = info.side as f64;
        let (x2, y2, angle2) = self.car_position(id);

        let mut multi_traj = MultiTrajectory::new();

        if angle1 == angle2 {
            // The car goes straight forwards.
            let angle = (side2 - side1) * 90.;
            let radius = f64::abs(side1 - side2) * self.size_crossroad / 8.;
            let line = (self.size_crossroad + self.car_place_width - 2.*radius) / 2.;
            multi_traj.add(Trajectory::line(line));
            multi_traj.add(Trajectory::circle(radius, angle));
            multi_traj.add(Trajectory::circle(radius, -angle));
            multi_traj.add(Trajectory::line(line));
        }
        else if f64::abs(angle1 - angle2) == 180. {
            // The car turns back.
            let radius = self.size_crossroad / 8. * (1. + side1 + side2);
            let line = 1.5 * self.car_place_width;
            multi_traj.add(Trajectory::line(line));
            multi_traj.add(Trajectory::circle(radius, -180.));
            multi_traj.add(Trajectory::line(line - self.car_place_width));
        }
        else {
            // The car turns right or left.
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

        let r = self.car_rectangle;
        let f: Box<Fn(f64, Context, &mut GlGraphics)> = Box::new(move | t, cont, g | {
            rectangle([0., 0., 1., 1.], r, multi_traj.transform(t, cont.trans(x1, y1).rot_deg(angle1)).transform, g);
        });

        return f;
    }

    pub fn vanish_car(&mut self, id: CarId) -> AnimationStep {
        let (x, y, angle) = self.car_position(id);
        let r = self.car_rectangle;

        let dx = self.size_crossroad;
        let f: Box<Fn(f64, Context, &mut GlGraphics)> = Box::new(move | t, cont, g | {
            rectangle([t as f32, 0., 1.-t as f32, 1.],r, cont.trans(x, y).rot_deg(angle).trans(t * dx, 0.).scale(1.-t*t, 1.-t*t).transform, g);
        });
        self.cars[id] = None;

        f
    }

    pub fn static_car(&mut self, id: CarId) -> AnimationStep {
        if self.cars[id].is_none() {
            return Box::new(|_, _, _| {} );
        }
        let (x, y, angle) = self.car_position(id);
        let r = self.car_rectangle;

        let f: Box<Fn(f64, Context, &mut GlGraphics)> = Box::new(move | _, cont, g | {
            rectangle([0., 0., 1., 1.],r, cont.trans(x, y).rot_deg(angle).transform, g);
        });

        return f;
    }

    pub fn update(&mut self, time: f64) {
        let moves = {
            let mut moves = self.data.lock().unwrap();
            let mut new_moves = None;
            std::mem::swap(&mut new_moves, &mut *moves);
            new_moves
        };

        if moves.is_none() {
            return;
        }

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

    pub fn draw_cars(&mut self, time: f64, cont: Context, g: &mut GlGraphics) {
        for a in self.car_animations.iter() {
            let cont = cont;
            a.step(time, cont, g);
        }
    }

    pub fn car_position(&self, id: CarId) -> (f64, f64, f64) {
        let (r, pos): (RoadInfo, usize) = self.cars[id].unwrap();
        let (dx, dy, length) = r.start.join(r.end);
        let (dx, dy, length) = (dx as f64, dy as f64, length as f64);
        let length = length * self.size_cell - self.size_crossroad;

        let dist = length - (pos as f64 + 1.) * self.car_place_width;
        let (mut x, mut y) = self.pos_crossroad(r.start);
        x += (dx * 0.5 - dy * r.side as f64 / 4.) * self.size_crossroad;
        y += (dy * 0.5 + dx * r.side as f64 / 4.) * self.size_crossroad;

        // Centers the car on the road.
        x += -dy * self.size_crossroad / 8.;
        y +=  dx * self.size_crossroad / 8.;

        // Uses the index of the car on this road.
        x += dx * dist;
        y += dy * dist;

        let angle = match (dx as i32, dy as i32) {
            (1, 0)  => 0.,
            (0, 1)  => 1.,
            (-1, 0) => 2.,
            (0, -1) => 3.,
            _       => panic!("Invalid direction."),
        };

        return (x, y, angle*90.);
    }

    pub fn transfer_data(&self) -> Arc<Mutex<Option<Vec<Move>>>>
    {
        self.data.clone()
    }

    pub fn run(&mut self, network: Network, cars: Vec<Car>)
    {
        let data = self.transfer_data();
        let duration = self.animation_duration;
        thread::spawn(move |  | {
            thread::sleep(Duration::from_millis(1000));
            trafficsim::run_simulation(network, cars, Some((duration, data)));
        });

        let opengl = OpenGL::V3_2;
        let (w, h) = (self.width as u32, self.height as u32);
        let mut window: Sdl2Window = WindowSettings::new("Traffic Simulation", [w, h])
            .exit_on_esc(true)
            .opengl(opengl)
            .build()
            .unwrap();

        let mut gl = GlGraphics::new(opengl);
        let mut events = Events::new(EventSettings::new());

        let refresh_frequency = 100000.;
        let refresh_period = 1. / refresh_frequency;
        let mut time = 0.;
        let mut cur_period = 0.;
        self.update(time);

        while let Some(e) = events.next(&mut window) {
            if let Some(args) = e.render_args() {
                gl.draw(args.viewport(), |cont, g| {
                    clear([1., 1., 1., 1.0], g);

                    self.draw_crossroads(cont, g);
                    self.draw_roads(cont, g);
                    self.draw_cars(time, cont, g);
                });
            }
            if let Some(args) = e.update_args() {
                time += args.dt;
                cur_period += args.dt;

                if cur_period > refresh_period {
                    cur_period = 0.;
                    self.update(time);
                }
            }
        }
    }
}

pub fn main() {
    let mut network = Network::new(0, 0);
    network.load_file("map1");
    network.simplify();
    println!("{}", network);

    let (duration, car_count) = (1., 1);

    let cars: Vec<Car> = (0..car_count).map(|_| {
        network.create_car()
    }).collect();

    let mut gui = Gui::new(&network, duration);
    gui.run(network, cars);
}