mod continuation;
mod process;
mod signal;

extern crate coco;
extern crate itertools;

use std;
use self::continuation::Continuation;
use self::process::Process;

/// TODO: Check if legal to use (compare with proposed method)
use self::coco::deque::{self, Worker, Stealer};
use self::itertools::multizip;
use self::itertools::Zip;

use std::sync::{Arc, Barrier};
use std::sync::atomic::{AtomicIsize, Ordering};
use std::thread;
use std::thread::JoinHandle;
use std::borrow::BorrowMut;

type JobStealer = Stealer<Box<Continuation<()>>>;

pub struct ParallelRuntime {
    shared_data: Arc<SharedData>,
    runtimes: Vec<Runtime>,
}

pub struct SharedData {
    runtimes_jobs: Vec<JobStealer>,
    n_local_working: AtomicIsize,
    n_global_working: AtomicIsize,
    sync_barrier: Barrier,
}

impl ParallelRuntime {
    pub fn new(n_workers: usize) -> Self {
        /// Create dequeue for jobs.
        let (mut worker_job_cur_instant, stealer_job_cur_instant): (Vec<_>, Vec<_>) =
            (0..n_workers).map(|_| deque::new()).unzip();

        /// Shared data structure between workers.
        let shared_data = SharedData {
            runtimes_jobs: stealer_job_cur_instant,
            n_local_working: AtomicIsize::new(0),
            n_global_working: AtomicIsize::new(0),
            sync_barrier: Barrier::new(n_workers),
        };

        /// Instantiation of ParallelRuntime.
        let mut r = ParallelRuntime {
            shared_data: Arc::new(shared_data),
            runtimes: vec!(),
        };

        /// Creation of workers.
        while let Some(cur_instant_worker) = worker_job_cur_instant.pop() {
            // spawn threads.
            let mut runtime = Runtime::new(r.shared_data.clone(), cur_instant_worker);
            r.runtimes.push(runtime);
        };

        r
    }

    pub fn execute(&mut self, job: Box<Continuation<()>>) {
        self.runtimes[0].on_current_instant(job);

        let mut join_handles = vec!();

        /// Start workers.
        while let Some(mut runtime) = self.runtimes.pop() {
            let mut b = thread::Builder::new();
            b = b.name("RRS Worker".to_string());

            let worker_continuation = move || {
                // Thread main loop.
                runtime.work();
                runtime
            };
            let handle = b.spawn(worker_continuation).unwrap();
            join_handles.push(handle);
        }

        /// Wait for work to be done.
        while let Some(x) = join_handles.pop() {
            self.runtimes.push(x.join().unwrap());
        };
    }
}

/// Runtime for executing reactive continuations.
pub struct Runtime {
    cur_instant:    Worker<Box<Continuation<()>>>,
    next_instant:   Vec<Box<Continuation<()>>>,
    end_of_instant: Vec<Box<Continuation<()>>>,
    manager:        Arc<SharedData>,
}


impl Runtime {
    /// Creates a new `Runtime`.
    pub fn new(manager: Arc<SharedData>,
               cur_instant: Worker<Box<Continuation<()>>>) -> Self {
        Runtime {
            cur_instant,
            next_instant: vec!(),
            end_of_instant: vec!(),
            manager,
        }
    }

    pub fn work(&mut self) {
        loop {
            println!("iter");
            // Step 1.

            /// Do all the local work.
            self.manager.n_local_working.fetch_add(1, Ordering::Relaxed);
            while let Some(c) = self.cur_instant.pop() {
                c.call_box(self, ());
            }
            self.manager.n_local_working.fetch_add(-1, Ordering::Relaxed);

            /// While someone is working (and might add something on his queue)
            while self.manager.n_local_working.load(Ordering::Relaxed) > 0 {
                let mut stolen = false;

                /// Try to steal work and unroll all local work then.
                while let Some(c) = self.manager.runtimes_jobs.iter().filter_map(|job| {
                    job.steal()
                }).next() {
                    stolen = true;
                    self.manager.n_local_working.fetch_add(1, Ordering::Relaxed);
                    c.call_box(self, ());

                    while let Some(c) = self.cur_instant.pop() {
                        c.call_box(self, ());
                    }
                    self.manager.n_local_working.fetch_add(-1, Ordering::Relaxed);
                }

                if !stolen {
                    thread::sleep_ms(10);
                }
            }

            if self.manager.sync_barrier.wait().is_leader() {
                self.manager.n_global_working.store(0, Ordering::Relaxed);
            }

            // Step 2.
            let mut end_of_instant = vec!();
            while let Some(c) = self.end_of_instant.pop() {
                end_of_instant.push(c)
            }

            while let Some(c) = self.next_instant.pop() {
                self.cur_instant.push(c);
            }

            self.manager.sync_barrier.wait();

            // Do all the local work.
            while let Some(c) = end_of_instant.pop() {
                c.call_box(self, ());
            }

            let local_work_to_do = self.end_of_instant.len() > 0 || self.next_instant.len() > 0 || self.cur_instant.len() > 0;

            // Here n_global_working should be equal to zero.
            if local_work_to_do {
                self.manager.n_global_working.fetch_add(1, Ordering::Relaxed);
            }

            self.manager.sync_barrier.wait();

            let work_to_do = self.manager.n_global_working.load(Ordering::Relaxed) > 0;

            if !work_to_do {
                break;
            }

        };
        println!("j'me tire");
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
use std::sync::{Mutex};

pub fn execute_process<P>(process: P) -> P::Value where P:Process, P::Value: Send {
    // let mut r = Runtime::new(1);
    let result: Arc<Mutex<Option<P::Value>>> = Arc::new(Mutex::new(None));
    let result2 = result.clone();

    let mut r = ParallelRuntime::new(4);

    let todo = Box::new(move |mut runtime: &mut Runtime, ()| {
        process.call(&mut runtime, move|runtime: &mut Runtime, value: P::Value| {
            *result2.lock().unwrap() = Some(value);
        });
    });

    r.execute(todo);

    let res = match Arc::try_unwrap(result) {
        Ok(x) => x,
        _ => panic!("Failed unwrap in execute_process"),
    };
    res.into_inner().unwrap().unwrap()
}


#[cfg(test)]
mod tests {
    use engine::{Runtime, Continuation};
    use engine::process::{Process, value, LoopStatus, ProcessMut};
    use engine::signal::{Signal, SEmit, PureSignal};
    use engine::process;
    use engine;

    use std::sync::{Arc, Mutex};
//    #[test]
//    fn test_42() {
//        println!("==> test_42");
//
//        let continuation_42 = |r: &mut Runtime, v: ()| {
//            r.on_next_instant(Box::new(|r: &mut Runtime, v: ()| {
//                r.on_next_instant(Box::new(|r: &mut Runtime, v: ()| {
//                    println!("42");
//                }));
//            }));
//        };
//        let mut r = Runtime::new();
//        //    r.on_current_instant(Box::new(continuation_42));
//        //    r.execute();
//
//        r.on_current_instant(Box::new(continuation_42));
//        r.execute();
//        r.instant();
//        r.instant();
//        r.instant();
//        println!("<== test_42");
//    }

    #[test]
    fn test_parallel_42() {
        println!("==>");

        let a = value(()).pause().map(|_| {
            for i in 0..1000000 {};
            println!("42");
        });

        let b = value(()).pause().map(|_| {
            for i in 0..1000000 {};
            println!("43");
        });

        let c = value(()).pause().map(|_| {
            for i in 0..1000000 {};
            println!("44");
        });

        engine::execute_process(a.join(b).join(c));
    }

//    #[test]
//    fn test_pause() {
//        println!("==> test_pause");
//
//        let c = (|r: &mut Runtime, ()| { println!("42") })
//            .pause().pause();
//
//        let mut r = Runtime::new();
//        r.on_current_instant(Box::new(c));
//        r.instant();
//        r.instant();
//        r.instant();
//
//        println!("<== test_pause");
//    }

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
        let container = Arc::new(Mutex::new(Some(0)));
        let container2 = container.clone();
        let container3 = container.clone();

        let plus_3 = move |_| {
            let mut opt_v = container.lock().unwrap();
            let v = opt_v.unwrap();
            *opt_v = (Some(v+3));
        };

        let times_2 = move |_| {
            let mut opt_v = container2.lock().unwrap();
            let v = opt_v.unwrap();
            *opt_v = (Some(v*2));
        };

        let p_plus_3 = process::Value::new(()).map(plus_3);
        let p_times_2 = process::Value::new(()).map(times_2);

        engine::execute_process(p_plus_3.then(p_times_2.pause()));
        assert_eq!(6, container3.lock().unwrap().unwrap());
    }

    #[test]
    fn test_then_else() {
        let p = value(false).then_else(value(42), value(44));
        assert_eq!(engine::execute_process(p), 44);

        let q = value(true).then_else(value(44), value(42));
        assert_eq!(engine::execute_process(q), 44);
    }

    #[test]
    fn test_join() {
        let reward = Arc::new(Mutex::new(Some(42)));
        let reward2 = reward.clone();

        let p = process::Value::new(reward).pause().pause().pause().pause()
            .map(|v| {
                let mut v = v.lock().unwrap();
                println!("Process p {:?}", *v);
                let x = *v;
                *v = None;
                x
            });
        let q = process::Value::new(reward2).pause().pause().pause()
            .map(|v| {
                let mut v = v.lock().unwrap();
                println!("Process q {:?}", *v);
                let x = *v;
                *v = None;
                x
            });

        assert_eq!((None, Some(42)), engine::execute_process(p.join(q)));
    }

    #[test]
    fn test_loop_while() {
        println!("==> test_loop_while");
        let n = 10;
        let reward = Arc::new(Mutex::new(Some(n)));
        let reward2 = reward.clone();

        let mut tot1 = 0;
        let mut tot2 = 0;
        let c1 = move |_| {
            let mut opt_r1 = reward.lock().unwrap();
            let v = opt_r1.unwrap();
            *opt_r1 = (Some(v-1));
            if v <= 0 {
                LoopStatus::Exit(tot1)
            } else {
                tot1 += v;
                LoopStatus::Continue
            }
        };
        let c2 = move |_| {
            let mut opt_r2 = reward2.lock().unwrap();
            let v = opt_r2.unwrap();
            *opt_r2 = (Some(v-1));
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
    fn test_while_perf() {
        let mut x = 100000;
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
    }

    #[test]
    #[ignore]
    fn test_pure_signal() {
        let s = PureSignal::new();
        let c1: fn(()) -> LoopStatus<()> = |_| {
            println!("s sent");
            LoopStatus::Continue
        };
        let p1 = s.emit(value(())).pause().pause().pause()
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
        let p2 = s.present(p21, p22)
            .map(c2).loop_while();
        let c3: fn(()) -> LoopStatus<()> = |_| {
            println!("s received");
            LoopStatus::Continue
        };
        let p3 = s.await_immediate().map(c3).pause().loop_while();

        let p = p1.join(p2.join(p3));

        engine::execute_process(p);
    }

    use engine::signal::{MCSignal, SAwaitIn};
    #[test]
    #[ignore]
    fn test_mc_signal() {
        let s = MCSignal::new(0, |v1, v2| {
            println!("{} + {} = {}", v1, v2, v1 + v2);
            v1 + v2
        });
        let p1 = s.emit(value(1)).pause().loop_inf();
        let print_v = |v| {
            println!("{}", v);
            v
        };
        let p2 = s.emit(s.await_in().map(print_v)).loop_inf();
        let p = p1.join(p2);
        engine::execute_process(p);
    }

    use engine::signal::{MPSCSignal, SAwaitInConsume};
    #[test]
    fn test_mpsc_signal() {
        pub struct TestStruct {
            content: i32,
        }

        let (s1, r1) = MPSCSignal::new(|v1: TestStruct, v2| {
           Some(v1)
        });

        let (s2, r2) = MPSCSignal::new(|v1: TestStruct, v2| {
            Some(v1)
        });

        let pre_loop1 = s1.emit(value(TestStruct { content: 0 }));
        let loop1 = move | v: Option<TestStruct> | {
            let mut v = v.unwrap();
//            println!("Value seen: {}", v.content);
            let x = v.content;
            v.content += 1;
            let condition = value(x >= 10);
            let p_true = value(LoopStatus::Exit(x));
            let p_false = s1.emit(value(v)).then(value(LoopStatus::Continue));

            condition.then_else(p_true, p_false)
        };

        let loop2 = move | v: Option<TestStruct> | {
            let mut v = v.unwrap();
            println!("Value seen: {}", v.content);
            let x = v.content;
            v.content += 1;
            let condition = value(x >= 10);
            let p = {
                if x >= 10 {
                    LoopStatus::Exit(x)
                } else {
                    LoopStatus::Continue
                }
            };

            s2.emit(value(v)).then(value(p))
        };

        let p1 = pre_loop1.then(r2.await_in().map(loop1).flatten().loop_while());
        let p2 = r1.await_in().map(loop2).flatten().loop_while();

        let p = p1.join(p2);
        assert_eq!(engine::execute_process(p), (11, 10));
    }

    use engine::signal::{SPMCSignal, SPMCSignalSender, SAwaitOneImmediate, SEmitConsume};
    #[test]
    fn test_spmc_signal() {

        let (s, sender) = SPMCSignal::new();
        let mut count = 0;
        let loop1 = move | () | {
            count += 1;
            if count >= 10 {
                LoopStatus::Exit((10))
            } else {
                LoopStatus::Continue
            }
        };

        let mut signal_value = 0;
        let increment = move | () | {
            signal_value += 2;
            signal_value
        };

        let p1 = sender.emit(value(()).map(increment)).map(loop1).pause().loop_while();

        let loop2 = move | v: i32 | {
            println!("Value seen: {}", v);
            if v >= 19 {
                LoopStatus::Exit(v)
            } else {
                LoopStatus::Continue
            }
        };
        let loop3 = move | v: i32 | {
            println!("Value seen: {}", v);
            if v >= 19 {
                LoopStatus::Exit(v)
            } else {
                LoopStatus::Continue
            }
        };

        let p2 = s.await_in().map(loop2).loop_while();
        let p3 = s.await_one_immediate().map(loop3).pause().loop_while();
        let p = p1.join(p2.join(p3));
        assert_eq!(engine::execute_process(p), (10, (20, 20)));
    }
}