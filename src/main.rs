#![feature(conservative_impl_trait)]

extern crate reactivers;
extern crate rand;

mod trafficsim;

use trafficsim::network::*;
use trafficsim::car::*;

pub fn main() {
    let mut network = Network::new(3, 3);
    network.add_crossroad(0, 0);
    network.add_crossroad(0, 2);
    network.add_crossroad(2, 0);
    network.add_crossroad(2, 2);
    network.add_crossroad(1, 1);

    network.add_all_roads((0, 0), (0, 2));
    network.add_all_roads((0, 2), (2, 2));
    network.add_all_roads((2, 2), (2, 0));
    network.add_all_roads((2, 0), (0, 0));

    network.simplify();

    for &c in &network.crossroads {
        println!("{}: {:?}", c, network.crossroad(c).nodes);
    }

    let car_count = 16;
    let cars: Vec<Car> = (0..car_count).map(|_| {
        println!("{}", network);
        network.create_car()
    }).collect();

    println!("{}", network);
    println!("{}", network.clone_graph());

    trafficsim::run_simulation(network, cars);
}