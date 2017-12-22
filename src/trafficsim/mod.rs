extern crate reactivers;
use reactivers::engine::signal::*;
use reactivers::engine::process::*;
use reactivers::engine;

pub mod car;
pub mod graph;
pub mod road;
pub mod network;

use self::network::*;
use self::car::*;

use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

pub fn run_simulation(network: Network, cars: Vec<Car>, data: Option<(f64,Arc<Mutex<Option<Vec<Move>>>>)>)
{
    let (central_sender, central_receiver) = spmc_signal::new();
    let (pos_signal_sender, pos_signal_receiver) =
        mpsc_signal::new(|(id, (action, speed)): (CarId, (Action, Speed)),
                         (mut v, mut s): (Vec<Action>, Vec<Speed>)|
            {
                for _ in (v.len())..(id+1) {
                    v.push(Action::VANISH);
                    s.push(0);
                }
                v[id] = action;
                s[id] = speed;
                (v, s)
            });

    let network_process = network.process(central_sender, pos_signal_receiver);
    let car_processes = cars.into_iter().map(|c| {
        c.process(
            central_receiver.clone(),
            pos_signal_sender.clone()
        )
    }).collect();

    let process = network_process.multi_join(car_processes);

    let gui_bool = data.is_some();
    let (duration, data) = {
        if gui_bool {
            data.unwrap()
        }
        else {
            (1., Arc::new(Mutex::new(None)))
        }
    };
    let mut step = 0;
    let gui_c = move | infos: Arc<GlobalInfos> | {
        {
            let mut data = data.lock().unwrap();
            *data = Some(infos.moves.clone());
        }
        step += 1;
        thread::sleep(Duration::from_millis((duration * 1000.) as u64));
    };
    let q1 = central_receiver.await_in().map(gui_c).pause().loop_inf();
    let q2 = value(());
    let gui_p = value(gui_bool).then_else(q1, q2);

    engine::execute_process_steps(gui_p.join(process), 8, -1);
}