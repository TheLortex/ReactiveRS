#![feature(conservative_impl_trait)]

extern crate reactivers;
mod trafficsim;

use trafficsim::car::Car;
use trafficsim::gui::Gui;
use trafficsim::network::Network;

pub fn main() {
    // We load the network from a file.
    let mut network = Network::new(0, 0);
    network.load_file("map1");

    // We remove the unused roads.
    network.simplify();
    println!("{}", network);

    // We define the parameters of the simulation.
    let (duration, car_count) = (0.5, 2000);

    // We create the cars on the network.
    let cars: Vec<Car> = (0..car_count).map(|_| {
        network.create_car()
    }).collect();

    // With the Gui
    let mut gui = Gui::new(&network, duration);
    gui.run(network, cars);

    // Without the Gui
//    trafficsim::run_simulation(network, cars, None);
}