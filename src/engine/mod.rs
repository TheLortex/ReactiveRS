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
    let result: Rc<Cell<Option<P::Value>>> = Rc::new(Cell::new(None));
    let result2 = result.clone();
    process.call(&mut r, move|runtime: &mut Runtime, value: P::Value| {
        result2.set(Some(value));
    });
    r.execute();
    result.take().unwrap()
}


#[cfg(test)]
mod tests {
    use engine::{Runtime, Continuation};
    use engine::process::{Process, value, LoopStatus, ProcessMut, Signal, SEmit, PureSignal};
    use engine::process;
    use engine;

    use std::rc::Rc;
    use std::cell::Cell;

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
        let program = p.pause().pause().map(|x| {println!("{}", x); x+4 });
        assert_eq!(engine::execute_process(program), 46);

        println!("<== test_process");
    }

    #[test]
    fn test_flatten() {
        let p = process::Value::new(42);
        let p2 = process::Value::new(p);
        assert_eq!(engine::execute_process(p2.flatten()), 42);
    }

    #[test]
    fn test_and_then() {
        let p = process::Value::new(42).pause();
        let f = |x| {
            process::Value::new(x + 42)
        };
        assert_eq!(engine::execute_process(p.and_then(f)), 84);
    }

    #[test]
    fn test_then() {
        let container = Rc::new(Cell::new(Some(0)));
        let container2 = container.clone();
        let container3 = container.clone();

        let plus_3 = move |_| {
            let v = container.take().unwrap();
            container.set(Some(v+3))
        };

        let times_2 = move |_| {
            let v = container2.take().unwrap();
            container2.set(Some(v*2))
        };

        let p_plus_3 = process::Value::new(()).map(plus_3);
        let p_times_2 = process::Value::new(()).map(times_2);

        engine::execute_process(p_plus_3.then(p_times_2));
        assert_eq!(6, container3.take().unwrap());
    }

    #[test]
    fn test_join() {
        let reward = Rc::new(Cell::new(Some(42)));
        let reward2 = reward.clone();

        let p = process::Value::new(reward).pause().pause().pause().pause()
            .map(|v| {
                let v = v.take();
                println!("Process p {:?}", v);
                v
            });
        let q = process::Value::new(reward2).pause().pause().pause()
            .map(|v| {
                let v = v.take();
                println!("Process q {:?}", v);
                v
            });

        assert_eq!((None, Some(42)), engine::execute_process(p.join(q)));
    }

    #[test]
    fn test_loop_while() {
        println!("==> test_loop_while");
        let mut x = 10;
        let c = move |_| {
            x -= 1;
            if x == 0 {
                LoopStatus::Exit(42)
            } else {
                LoopStatus::Continue
            }
        };
        let p = process::Value::new(()).map(c);
        assert_eq!(42, engine::execute_process(p.pause().loop_while()));

        let n = 10;
        let reward = Rc::new(Cell::new(Some(n)));
        let reward2 = reward.clone();

        let mut tot1 = 0;
        let mut tot2 = 0;
        let c1 = move |_| {
            let v = reward.take().unwrap();
            reward.set(Some(v-1));
            if v <= 0 {
                LoopStatus::Exit(tot1)
            } else {
                tot1 += v;
                LoopStatus::Continue
            }
        };
        let c2 = move |_| {
            let v = reward2.take().unwrap();
            reward2.set(Some(v-1));
            if v <= 0 {
                process::Value::new(LoopStatus::Exit(tot2))
            } else {
                tot2 += v;
                process::Value::new(LoopStatus::Continue)
            }
        };

        let p = process::Value::new(()).pause().pause()
            .map(c1);
        let q = process::Value::new(()).pause().pause()
            .and_then(c2);

        let pbis = p.loop_while();
        let qloop = q.loop_while();
        let qbis = process::Value::new(()).pause().then(qloop);

        let m = n / 2;
        assert_eq!((m * (m + 1), m * m), engine::execute_process(pbis.join(qbis)));
        println!("<== test_loop_while");
    }

    #[test]
    #[ignore]
    fn test_pure_signal() {
        let s = PureSignal::new();
        let s2 = s.clone();
        let s3 = s.clone();
        let c1: fn(()) -> LoopStatus<()> = |_| {
            println!("s sent");
            LoopStatus::Continue
        };
        let p1 = s.emit(Value::new(())).pause().pause().pause()
            .map(c1).loop_while();
        let c21 = |_| {
            println!("present");
            ()
        };
        let p21 = process::Value::new(()).map(c21).pause();
        let c22 = |_| {
            println!("not present");
            ()
        };
        let p22 = process::Value::new(()).map(c22);
        let c2: fn(()) -> LoopStatus<()> = |_| { LoopStatus::Continue };
        let p2 = s2.present(p21, p22)
            .map(c2).loop_while();
        let c3: fn(()) -> LoopStatus<()> = |_| {
            println!("s received");
            LoopStatus::Continue
        };
        let p3 = s3.await_immediate().map(c3).pause().loop_while();

        let p = p1.join(p2.join(p3));

        engine::execute_process(p);
    }

    use engine::process::{MCSignal, SAwait, Value};
    #[test]
    #[ignore]
    fn test_mc_signal() {
        let s = MCSignal::new(0, |v1, v2| {
            println!("{} + {} = {}", v1, v2, v1 + v2);
            v1 + v2
        });
        let p1 = s.clone().emit(value(1)).pause().loop_inf();
        let print_v = |v| {
            println!("{}", v);
            v
        };
        let p2 = s.clone().emit(s.clone().await().map(print_v)).loop_inf();
        let p = p1.join(p2);
        engine::execute_process(p);
    }
}