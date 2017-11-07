mod continuation;
mod process;

use std;
use self::continuation::Continuation;
use self::process::Process;

/// Runtime for executing reactive continuations.
pub struct Runtime {
    cur_instant:    Vec<Box<Continuation<()>>>,
    next_instant:   Vec<Box<Continuation<()>>>,
    end_of_instant: Vec<Box<Continuation<()>>>,
}


impl Runtime {
    /// Creates a new `Runtime`.
    pub fn new() -> Self {
        Runtime { cur_instant: vec!(), next_instant: vec!(), end_of_instant: vec!() }
    }

    /// Executes instants until all work is completed.
    pub fn execute(&mut self) {
        while self.instant() {
            continue;
        }
    }

    /// Executes a single instant to completion. Indicates if more work remains to be done.
    pub fn instant(&mut self) -> bool {
        print!("-");
        // We first execute all the continuations of the instant.
        while let Some(c) = self.cur_instant.pop() {
            c.call_box(self, ());
        }

        // Then, we execute the end of instant continuations
        let mut end_of_instant = vec!();
        std::mem::swap(&mut end_of_instant, &mut self.end_of_instant);
        std::mem::swap(&mut self.cur_instant, &mut self.next_instant);

        while let Some(c) = end_of_instant.pop() {
            c.call_box(self, ());
        }

        !(self.cur_instant.is_empty()
            && self.end_of_instant.is_empty()
            && self.next_instant.is_empty())
    }

    /// Registers a continuation to execute on the current instant.
    fn on_current_instant(&mut self, c: Box<Continuation<()>>) {
        self.cur_instant.push(c);
    }

    /// Registers a continuation to execute at the next instant.
    fn on_next_instant(&mut self, c: Box<Continuation<()>>) {
        self.next_instant.push(c);
    }

    /// Registers a continuation to execute at the end of the instant. Runtime calls for `c`
    /// behave as if they where executed during the next instant.
    fn on_end_of_instant(&mut self, c: Box<Continuation<()>>) {
        self.end_of_instant.push(c);
    }
}

use std::cell::Cell;
use std::rc::Rc;

pub fn execute_process<P>(process: P) -> P::Value where P:Process {
    let mut r = Runtime::new();
    let bonjour: Rc<Cell<Option<P::Value>>> = Rc::new(Cell::new(None));
    let bonjour2 = bonjour.clone();
    process.call(&mut r, move|runtime: &mut Runtime, value: P::Value| {
        bonjour2.set(Some(value));
    });
    r.execute();
    bonjour.take().unwrap()
}


#[cfg(test)]
mod tests {
    use engine::{Runtime, Continuation};
    use engine::process::Process;
    use engine::process;
    use engine;

    #[test]
    fn test_42() {
        println!("==> test_42");

        let continuation_42 = |r: &mut Runtime, v: ()| {
            r.on_next_instant(Box::new(|r: &mut Runtime, v: ()| {
                r.on_next_instant(Box::new(|r: &mut Runtime, v: ()| {
                    println!("42");
                }));
            }));
        };
        let mut r = Runtime::new();
        //    r.on_current_instant(Box::new(continuation_42));
        //    r.execute();

        r.on_current_instant(Box::new(continuation_42));
        r.execute();
        r.instant();
        r.instant();
        r.instant();
        println!("<== test_42");

    }

    #[test]
    fn test_pause() {
        println!("==> test_pause");

        let c = (|r: &mut Runtime, ()| { println!("42") })
            .pause().pause();

        let mut r = Runtime::new();
        r.on_current_instant(Box::new(c));
        r.instant();
        r.instant();
        r.instant();

        println!("<== test_pause");
    }

    #[test]
    fn test_process() {
        println!("==> test_process");

        let p = process::Value::new(42);
        let program = p.pause().pause().map(|x| {println!("{}", x)});
        engine::execute_process(program);

        println!("<== test_process");
    }
}