extern crate reactivers;
extern crate graphics;
extern crate opengl_graphics;
extern crate piston;
extern crate sdl2_window;

pub mod car;
pub mod graph;
pub mod road;
pub mod network;
pub mod gui;

use self::network::*;
use self::car::*;

use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use reactivers::engine::signal::*;
use reactivers::engine::process::*;
use reactivers::engine;


/// Launches a simulation
pub fn run_simulation(network: Network, cars: Vec<Car>, data: Option<(f64,Arc<Mutex<Option<Vec<Move>>>>)>)
{
    // We first define the signals.
    // A first SPMC signal to send information to the cars.
    let (central_sender, central_receiver) = spmc_signal::new();

    // Then a MPSC to allow the cars to send information to the control part.
    let (pos_signal_sender, pos_signal_receiver) =
        mpsc_signal::new(|(id, (action, speed)): (CarId, (Action, Speed)),
                         (mut v, mut s): (Vec<Action>, Vec<Speed>)|
            {
                // If needed, increases the size of the vector.
                for _ in (v.len())..(id+1) {
                    v.push(Action::VANISH);
                    s.push(0);
                }
                v[id] = action;
                s[id] = speed;
                (v, s)
            });

    // We get the network and the car processes.
    let network_process = network.process(central_sender, pos_signal_receiver);
    let car_processes = cars.into_iter().map(|c| {
        c.process(
            central_receiver.clone(),
            pos_signal_sender.clone()
        )
    }).collect();

    let process = network_process.multi_join(car_processes);

    // We build the process that transfers the data to the GUI, if there is one.

    // First the process that returns true or false if there is some GUI.
    let gui_bool = data.is_some();
    let (duration, data) = {
        if gui_bool {
            data.unwrap()
        }
        else {
            (1., Arc::new(Mutex::new(None)))
        }
    };

    // Second the main loop that transfers the data.
    let mut step = 0;
    let gui_c = move | infos: Arc<GlobalInfo> | {
        {
            let mut data = data.lock().unwrap();
            *data = Some(infos.moves.clone());
        }
        step += 1;
        // This process synchronizes with the GUI.
        thread::sleep(Duration::from_millis((duration * 1000.) as u64));
    };


    let transfer_loop = central_receiver.await_in().map(gui_c).pause().loop_inf();
    let void = value(());

    // The final transfer process void or the transfer loop.
    let transfer_process =
        value(gui_bool).then_else(transfer_loop, void);

    engine::execute_process_steps(transfer_process.join(process), 8, -1);
}