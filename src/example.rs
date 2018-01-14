#![feature(conservative_impl_trait)]
extern crate reactivers;

use reactivers::engine::signal::*;
use reactivers::engine::process::*;
use reactivers::engine;

fn main() {
    let p = value(()).map(| _ | {
        println!("Hello world");
    });

    engine::execute_process(p);

    let s = puresignal::new();
    let sender = value(()).pause().pause().emit(&s);
    let receiver = s.await().map(| _ | {
        println!("Received");
    });

    engine::execute_process(sender.join(receiver));
}