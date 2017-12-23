# ReactiveRS

Rust implementation of Reactive ML extension.

All the sources can be found in `src/` folder.
This package contains the reactive library in `src/engine` folder,
and 2 applications using the library.

## `reactivers`
This library implements basic constructions of Reactive ML.
It does not requires additional installs (except for
the dependencies expressed in `Cargo.toml`).
Here are basic commands to use this library, assuming you
are using `cargo` command. They must be executed in current
directory:
- `cargo build`: builds the library.
- `cargo test`: launches the tests.
- `cargo doc`: regenerates the documentation of the library,
in `target/debug/doc/reactivers/` folder.

The library has already been generated and is directly
available in `doc/` folder. It can be opened by opening
`doc/reactivers/index.html` with a browser.

Only the parallel implementation is given.
The addition of many `Mutex` and synchronization points
brings considerably slows the reactive engine.

## Traffic Simulation
The first application is a Traffic Simulation.
Cars move in a road network. They compute their optimal path
to their destination, emit their wanted actions on a signal, and a central
unit gathers all these actions, computes the allowed moves
and emits the updated information to all the cars.

This application requires the SDL2 graphics library to be installed.
On Linux, this can be done by installing package `libsdl2-dev`
with `apt`: `sudo apt install libsdl2-dev`.

If it does not work, the application can be launched without 
the GUI (it runs but shows nothing), see `traffic_sim.rs` for
further details.

To run the application, use the following command:
`cargo run --bin trafficsim --features trafficsim`.
(`release` parameter can be added).

A documentation has also been generated in `doc/` folder.
(This has be done by changing the binary 
to become a part of the library).

The resulting application is very slow. We did not have time
to precisely investigate the reasons of the slow down.

NB:
The car computations are independent and thus fully parallel,
but the central unit computations are still a big sequential
computation. To use more of reactive constructions, this would
be interesting to distribute these computations among the roads
and use more signals to exchange values.

## Game of life

This is a simple reactive implementation of the game of life.
Details on the game are available 
[here](https://en.wikipedia.org/wiki/Conway%27s_Game_of_Life).

The display requires `ncurses` to be installed.
This can be done with following command:
`sudo apt install libncurses5-dev`.

To run the application, use the following command:
`cargo run --bin gameoflife --features gameoflife`.
(`release` parameter can be added).

NB: The compilation can take some time (about 30 seconds).
