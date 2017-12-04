mod continuation;
mod process;

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
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::thread::JoinHandle;



pub struct Jobs {
    cur_instant: Stealer<Box<Continuation<()>>>,
    next_instant: Stealer<Box<Continuation<()>>>,
    end_of_instant: Stealer<Box<Continuation<()>>>,
}

pub struct ParallelRuntime {
    runtimes_jobs: Vec<Jobs>,
    work_reader: Vec<Arc<AtomicBool>>,
}

impl ParallelRuntime {
    pub fn new(n_workers: usize, first_job: Box<Continuation<()>>) -> Arc<Self> {
        // Create dequeue for jobs.
        let (worker_job_cur_instant, stealer_job_cur_instant): (Vec<_>, Vec<_>) =
            (0..n_workers).map(|_| deque::new()).unzip();

        let (worker_job_next_instant, stealer_job_next_instant): (Vec<_>, Vec<_>) =
            (0..n_workers).map(|_| deque::new()).unzip();

        let (worker_job_end_of_instant, stealer_job_end_of_instant): (Vec<_>, Vec<_>) =
            (0..n_workers).map(|_| deque::new()).unzip();

        let jobs_as_tuples = multizip((stealer_job_cur_instant, stealer_job_next_instant, stealer_job_end_of_instant));

        let jobs_iter = jobs_as_tuples.map(
            |(cur_instant, next_instant, end_of_instant)| {
                Jobs {cur_instant, next_instant, end_of_instant}
            }
        );

        let barrier = Arc::new(Barrier::new(n_workers));

        let mut still_work_to_do= vec!() ;
        let mut still_work_to_do_ref = vec!();
        for _ in 0..n_workers {
            let var = Arc::new(AtomicBool::new(true));
            still_work_to_do_ref.push(var.clone());
            still_work_to_do.push(var);
        };

        let mut jobs_vec = vec!();
        jobs_vec.extend(jobs_iter);

        let r = ParallelRuntime {
            runtimes_jobs: jobs_vec,
            work_reader: still_work_to_do_ref,
        };
        let me = Arc::new(r);

        let workers_as_tuples = itertools::multizip((worker_job_cur_instant, worker_job_next_instant, worker_job_end_of_instant));

        let mut runtimes = vec!();

        for (i, (cur_instant, next_instant, end_of_instant)) in workers_as_tuples.enumerate() {
            // spawn threads.
            let mut runtime = Runtime::new(me.clone(), still_work_to_do.pop().unwrap(), cur_instant, next_instant, end_of_instant);
            runtimes.push(runtime);
        }

        runtimes[0].on_current_instant(first_job);

        let mut join_handles = vec!();

        while let Some(mut runtime) = runtimes.pop() {
            let mut b = thread::Builder::new();
            b = b.name("RRS Worker".to_string());

            let c = barrier.clone();
            let worker_continuation = move || {
                // Thread main loop.
                runtime.work(c);
            };
            let handle = b.spawn(worker_continuation).unwrap();
            join_handles.push(handle);
        }

        while let Some(x) = join_handles.pop() {
            x.join();
        };

        me.clone()
    }
}

/// Runtime for executing reactive continuations.
pub struct Runtime {
    cur_instant:    Worker<Box<Continuation<()>>>,
    next_instant:   Worker<Box<Continuation<()>>>,
    end_of_instant: Worker<Box<Continuation<()>>>,
    manager:        Arc<ParallelRuntime>,
    working_bool:   Arc<AtomicBool>,
}


impl Runtime {
    /// Creates a new `Runtime`.
    pub fn new(manager: Arc<ParallelRuntime>,
               working_bool: Arc<AtomicBool>,
               cur_instant: Worker<Box<Continuation<()>>>,
               next_instant: Worker<Box<Continuation<()>>>,
               end_of_instant: Worker<Box<Continuation<()>>>) -> Self {
        Runtime {
            cur_instant,
            next_instant,
            end_of_instant,
            manager,
            working_bool,
        }
    }

    pub fn work(&mut self, synchro_barrier: Arc<Barrier>) {
        loop {
            println!("iter");
            // Step 1.

            // Do all the local work.
            while let Some(c) = self.cur_instant.pop() {
                c.call_box(self, ());
            }

            // Try to steal work and unroll all local work then.
            while let Some(c) = self.manager.runtimes_jobs.iter().filter_map(|job| {
                job.cur_instant.steal()
            }).next() {
                c.call_box(self, ());

                while let Some(c) = self.cur_instant.pop() {
                    c.call_box(self, ());
                }
            }

            // TODO: Sleep and try again here because some worker might add a lot of work to steal later.
            synchro_barrier.wait();

            // Step 2.
            let mut end_of_instant = vec!();
            while let Some(c) = self.end_of_instant.pop() {
                end_of_instant.push(c)
            }

            while let Some(c) = self.next_instant.pop() {
                self.cur_instant.push(c);
            }

            synchro_barrier.wait();

            // Do all the local work.
            while let Some(c) = end_of_instant.pop() {
                c.call_box(self, ());
            }

            // TODO: Steal end_of_instant of other processes.

            let local_work_to_do = self.end_of_instant.len() > 0 || self.next_instant.len() > 0 || self.cur_instant.len() > 0;
            self.working_bool.store(local_work_to_do, Ordering::Relaxed);
            synchro_barrier.wait();
            let mut other_work_to_do = false;


            for t in self.manager.work_reader.iter()  {
                other_work_to_do |= t.load(Ordering::Relaxed);
            }

            if !other_work_to_do {
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

    let mut r = ParallelRuntime::new(4, Box::new(move |mut runtime: &mut Runtime, ()| {
        process.call(&mut runtime, move|runtime: &mut Runtime, value: P::Value| {
            *result2.lock().unwrap() = Some(value);
        });
    }));

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
        };/*
        let mut rt = ParallelRuntime::new();
        let mut r = Runtime::new(rt);
        //    r.on_current_instant(Box::new(continuation_42));
        //    r.execute();

        r.on_current_instant(Box::new(continuation_42));
        r.execute();
        r.instant();
        r.instant();
        r.instant();*/
        println!("<== test_42");
    }

    #[test]
    fn test_parallel_42() {
        println!("==>");

        let a = value(()).pause().map(|_| {
            for i in 0..100000 {};
            println!("42");
        });

        let b = value(()).pause().map(|_| {
            for i in 0..100000 {};
            println!("43");
        });

        let c = value(()).pause().map(|_| {
            for i in 0..100000 {};
            println!("44");
        });

        engine::execute_process(a.join(b).join(c));
    }

    #[test]
    fn test_pause() {
        println!("==> test_pause");

    /*    let c = (|r: &mut Runtime, ()| { println!("42") })
            .pause().pause();

        let mut rt = ParallelRuntime::new();
        let mut r = Runtime::new(rt);
        r.on_current_instant(Box::new(c));
        r.instant();
        r.instant();
        r.instant();
*/
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
       /* let container = Rc::new(Cell::new(Some(0)));
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
        assert_eq!(6, container3.take().unwrap());*/
    }

    #[test]
    fn test_join() {
    /*    let reward = Rc::new(Cell::new(Some(42)));
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

        assert_eq!((None, Some(42)), engine::execute_process(p.join(q)));*/
    }

    #[test]
    fn test_loop_while() {
     /*   println!("==> test_loop_while");
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
        println!("<== test_loop_while");*/
    }

    #[test]
    #[ignore]
    fn test_pure_signal() {
        /*let s = PureSignal::new();
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
        engine::execute_process(p);*/
    }
}