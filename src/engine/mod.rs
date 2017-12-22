mod continuation;
pub mod process;
pub mod signal;

extern crate coco;
extern crate itertools;

use self::continuation::Continuation;
use self::process::Process;

use self::coco::deque::{self, Worker, Stealer};

use std::sync::{Arc, Barrier};
use std::sync::atomic::{AtomicIsize, Ordering};
use std::thread;
use std::mem;

type JobStealer = Stealer<Box<Continuation<()>>>;

/// Parallel runtime structure
pub struct ParallelRuntime {
    /// Shared data between workers
    shared_data: Arc<SharedData>,
    /// List of workers
    runtimes: Vec<Runtime>,
}

/// Shared data structure
pub struct SharedData {
    /// Stealing end of the work-stealing queue for each worker.
    runtimes_jobs: Vec<JobStealer>,
    /// Number of workers that are currently working in the instant.
    n_local_working: AtomicIsize,
    /// Number of workers that, at end of instant, will have work to do on next instant.
    n_global_working: AtomicIsize,
     /// Synchronization barrier between workers.
    sync_barrier: Barrier,
}

impl ParallelRuntime {
    /// Creates a new `ParallelRuntime` by creating `n_workers` workers.
    pub fn new(n_workers: usize) -> Self {
        // Create work stealing queue for jobs.
        // The worker end can push and pop.
        // The stealer end can steal.
        let (mut worker_job_cur_instant, stealer_job_cur_instant): (Vec<_>, Vec<_>) =
            (0..n_workers).map(|_| deque::new()).unzip();

        // Shared data structure between workers.
        let shared_data = SharedData {
            runtimes_jobs: stealer_job_cur_instant,
            n_local_working: AtomicIsize::new(n_workers as isize),
            n_global_working: AtomicIsize::new(0),
            sync_barrier: Barrier::new(n_workers),
        };

        // Instantiation of ParallelRuntime.
        let mut r = ParallelRuntime {
            shared_data: Arc::new(shared_data),
            runtimes: vec!(),
        };

        // Creation of workers.
        while let Some(cur_instant_worker) = worker_job_cur_instant.pop() {
            let mut runtime = Runtime::new(r.shared_data.clone(), cur_instant_worker);
            r.runtimes.push(runtime);
        };

        r
    }

    /// Start the runtime with a given job.
    /// `max_iters` is the maximum number of iterations that should be done. If it's -1 then there's
    /// no limit.
    pub fn execute(&mut self, job: Box<Continuation<()>>, max_iters: i32) {
        // Give the job to an arbitrarily chosen worker.
        self.runtimes[0].on_current_instant(job);

        let mut join_handles = vec!();

        // Start workers.
        while let Some(mut runtime) = self.runtimes.pop() {
            let mut b = thread::Builder::new();
            b = b.name("RRS Worker".to_string());

            let worker_continuation = move || {
                // Thread main loop.
                runtime.work(max_iters);
                runtime
            };
            let handle = b.spawn(worker_continuation).unwrap();
            join_handles.push(handle);
        }

        // Wait for work to be done.
        while let Some(x) = join_handles.pop() {
            self.runtimes.push(x.join().unwrap());
        };
    }
}

/// Runtime for executing reactive continuations.
pub struct Runtime {
    /// Continuations that have to be done on current instant.
    /// Worker end of the work stealing queue, as the runtime can be part of a parallel runtime.
    cur_instant:    Worker<Box<Continuation<()>>>,
    /// Continuations that have to be done on next instant.
    next_instant:   Vec<Box<Continuation<()>>>,
    /// Continuations that have to be done on end of instant.
    end_of_instant: Vec<Box<Continuation<()>>>,
    /// Pointer to the shared data between workers.
    manager:        Arc<SharedData>,
}

use std::time;

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

    /// Worker loop that executes at most `max_iter` instants.
    /// If `max_iter` is -1 there is no limit.
    pub fn work(&mut self, max_iter: i32) {
        let mut n_iter = 0;

        loop {
            // Execution count check.
            n_iter += 1;
            if max_iter != -1 && n_iter > max_iter {
                break;
            }

            // Step 1.
            // Do all the local work.
            while let Some(c) = self.cur_instant.pop() {
                c.call_box(self, ());
            }
            // Decrement the number of working threads when work is done.
            self.manager.n_local_working.fetch_add(-1, Ordering::Relaxed);

            // While someone is working (and might add something on his queue)
            while self.manager.n_local_working.load(Ordering::Relaxed) > 0 {
                let mut stolen = false;

                // Try to steal work and unroll all local work then.
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

                // Nothing was stolen but someone is still working, try to steal later on.
                if !stolen {
                    thread::sleep(time::Duration::from_millis(10));
                }
            }

            // Synchronization barrier, and reset global working threads counter.
            if self.manager.sync_barrier.wait().is_leader() {
                self.manager.n_global_working.store(0, Ordering::Relaxed);
            }

            // Step 2.
            let mut end_of_instant = vec!();
            mem::swap(&mut self.end_of_instant, &mut end_of_instant);

            while let Some(c) = self.next_instant.pop() {
                self.cur_instant.push(c);
            }

            // Do all the local work.
            while let Some(c) = end_of_instant.pop() {
                c.call_box(self, ());
            }

            self.manager.sync_barrier.wait();

            // Check if the worker will have work to do later;
            let local_work_to_do = self.end_of_instant.len() > 0 || self.next_instant.len() > 0 || self.cur_instant.len() > 0;

            // Here n_global_working should be equal to zero.
            if local_work_to_do {
                self.manager.n_global_working.fetch_add(1, Ordering::Relaxed);
            }
            self.manager.n_local_working.fetch_add(1, Ordering::Relaxed);
            self.manager.sync_barrier.wait();

            let work_to_do = self.manager.n_global_working.load(Ordering::Relaxed) > 0;

            if !work_to_do {
                break;
            }

        };
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

use std::sync::{Mutex};


pub fn execute_process<P>(process: P) -> P::Value where P:Process, P::Value: Send {
    execute_process_steps(process, 6, -1)
}

pub fn execute_process_steps<P>(process: P, n_workers: usize, max_iters: i32) -> P::Value where P:Process, P::Value: Send {
    // let mut r = Runtime::new(1);
    let result: Arc<Mutex<Option<P::Value>>> = Arc::new(Mutex::new(None));
    let result2 = result.clone();

    let mut r = ParallelRuntime::new(n_workers);

    let todo = Box::new(move |mut runtime: &mut Runtime, ()| {
        process.call(&mut runtime, move |_: &mut Runtime, value: P::Value| {
            *result2.lock().unwrap() = Some(value);
        });
    });

    r.execute(todo, max_iters);

    let res = match Arc::try_unwrap(result) {
        Ok(x) => x,
        _ => panic!("Failed unwrap in execute_process. It may be a Deadlock."),
    };
    res.into_inner().unwrap().unwrap()
}


#[cfg(test)]
mod tests {
    extern crate test;
    extern crate cpuprofiler;
    use self::cpuprofiler::PROFILER;

    use engine::process::{Process, value, LoopStatus, ProcessMut};
    use engine::process;
    use engine;
    use engine::signal::*;

    use std::sync::{Arc, Mutex};
    use self::test::Bencher;
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
            for _ in 0..1000000 {};
            println!("42");
        });

        let b = value(()).pause().map(|_| {
            for _ in 0..1000000 {};
            println!("43");
        });

        let c = value(()).pause().map(|_| {
            for _ in 0..1000000 {};
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
            *opt_v = Some(v+3);
        };

        let times_2 = move |_| {
            let mut opt_v = container2.lock().unwrap();
            let v = opt_v.unwrap();
            *opt_v = Some(v*2);
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
            *opt_r1 = Some(v-1);
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
            *opt_r2 = Some(v-1);
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
        let mut x = 1000;
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

    #[bench]
    fn bench_while_perf(b: &mut Bencher) {
        b.iter(|| test_while_perf());
    }

    #[test]
    #[ignore]
    fn profile_while_perf() {
        PROFILER.lock().unwrap().start("./my-prof.profile").unwrap();

        for _ in 0..100 {
            test_while_perf();
        }

        PROFILER.lock().unwrap().stop().unwrap();
    }

    #[test]
    #[ignore]
    fn test_pure_signal() {
        let s = puresignal::new();
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

    #[test]
    #[ignore]
    fn test_mc_signal() {
        let s = value_signal::new(0, |v1, v2| {
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

    #[test]
    fn test_mpsc_signal() {
        pub struct TestStruct {
            content: i32,
        }

        let (s1, r1) = mpsc_signal::new(|v1: TestStruct, _| {
           Some(v1)
        });

        let (s2, r2) = mpsc_signal::new(|v1: TestStruct, _| {
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

    #[test]
    fn test_spmc_signal() {

        let (sender, receiver) = spmc_signal::new();
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

        let p2 = receiver.await_in().map(loop2).loop_while();
        let p3 = receiver.await_one_immediate().map(loop3).pause().loop_while();
        let p = p1.join(p2.join(p3));
        assert_eq!(engine::execute_process(p), (10, (20, 20)));
    }
}