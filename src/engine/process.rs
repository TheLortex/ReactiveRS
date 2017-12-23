use super::Runtime;
use super::continuation::Continuation;
use std::sync::{Arc, Mutex};
use super::signal::*;
use super::signal::signal_runtime::*;
use std::thread;

/// A reactive process.
pub trait Process: 'static + Send {
    /// The value created by the process.
    type Value;

    /// Executes the reactive process in the runtime, calls `next` with the resulting value.
    fn call<C>(self, runtime: &mut Runtime, next: C) where C: Continuation<Self::Value>;

    fn pause(self) -> Pause<Self> where Self: Sized, Self::Value: Send {
        Pause {process: self}
    }

    /// Creates a new process that applies a function to the output value of `Self`.
    fn map<F, V2>(self, map: F) -> Map<Self, F>
        where Self: Sized, F: FnOnce(Self::Value) -> V2 + 'static + Send
    {
        Map { process: self, map }
    }

    /// Creates a new process that executes the process returned by `Self`.
    fn flatten<>(self) -> Flatten<Self>
        where Self: Sized, Self::Value: Process + Send {
        Flatten { process: self }
    }


    /// Creates a new process that executes the first process, applies the given function to the
    /// result, and executes the returned process.
    fn and_then<F, P>(self, function: F) -> AndThen<Self, F>
        where F: FnOnce(Self::Value) -> P + 'static + Send, Self: Sized, P: Process {
        self.map(function).flatten()
    }

    /// Creates a new process that executes the two processes sequentially, and returns the result
    /// of the second process.
    fn then<P>(self, process: P) -> Then<Self, P>
        where Self: Sized, P: Process + Sized {
        Then {process1: self, process2: process}
    }

    /// Creates a new process that executes the two processes in parallel, and returns the couple of
    /// their return values.
    fn join<P>(self, process: P) -> Join<Self, P> where Self: Sized, P: Process + Sized {
        Join {process1: self, process2: process}
    }

    fn multi_join<P>(self, ps: Vec<P>) -> Join<Self, MultiJoin<P>>
        where Self: Sized, P: Process + Sized, P::Value: Send {
        self.join(MultiJoin { ps })
    }

    /// Creates a new process that executes process `q1` if the result of `Self` is true, and `q2`
    /// otherwise.
    fn then_else<Q1, Q2>(self, q1: Q1, q2: Q2) -> ThenElse<Self, Q1, Q2>
        where Self: Process<Value=bool> + Sized, Q1: Process, Q2: Process<Value=Q1::Value> {
        ThenElse { condition: self, q1, q2}
    }

    fn emit<S>(self, s: &S) -> Emit<S, Self>
        where S: SEmit + Sized, Self: Sized, Self: Process<Value=<S::VR as ValueRuntime>::V1>
    {
        s.emit(self)
    }

    fn emit_consume<S>(self, s: S) -> Emit<S, Self>
        where S: SEmitConsume + Sized, Self: Sized, Self: Process<Value=<S::VR as ValueRuntime>::V1>
    {
        s.emit(self)
    }
}


/// A process that can be executed multiple times, modifying its environment each time.
pub trait ProcessMut: Process {
    /// Executes the mutable process in the runtime, then calls `next` with the process and the
    /// process's return value.
    fn call_mut<C>(self, runtime: &mut Runtime, next: C)
        where Self: Sized, C: Continuation<(Self, Self::Value)>;

    /// Creates a process that executes once the ProcessMut, and returns it with the obtained value.
    fn get_mut(self) -> Mut<Self> where Self: Sized {
        Mut { process: self }
    }

    /// Creates a process that executes a ProcessMut with return type LoopStatus until it returns
    /// Exit(v).
    fn loop_while(self) -> While<Self> where Self: Sized {
        While { process: self }
    }

    /// Creates a process that executes a ProcessMut in infinite loop.
    fn loop_inf(self) -> While<Map<Self, fn(()) -> LoopStatus<()>>>
        where Self: Process<Value=()> + Sized
    {
        let c: fn(()) -> LoopStatus<()> = move |_| {
            LoopStatus::Continue
        };
        While { process: self.map(c) }
    }
}


pub struct Value<V> {
    value: V,
}

impl<V> Value<V> where V: 'static + Send {
    pub fn new(v: V) -> Self {
        Value {value: v}
    }
}

pub fn value<V>(v: V) -> Value<V> where V: 'static {
    Value { value: v }
}

impl<V> Process for Value<V> where V: 'static + Send {
    type Value = V;

    fn call<C>(self, runtime: &mut Runtime, next: C) where C: Continuation<Self::Value> {
        next.call(runtime, self.value);
    }
}

impl<V> ProcessMut for Value<V> where V: Copy + 'static + Send {
    fn call_mut<C>(self, runtime: &mut Runtime, next: C)
        where Self: Sized, C: Continuation<(Self, Self::Value)> {
        let v = self.value;
        next.call(runtime, (self, v));
    }
}


pub struct Pause<P> {
    process: P,
}

impl<P> Process for Pause<P> where P: Process, P::Value: Send {
    type Value = P::Value;

    fn call<C>(self, runtime: &mut Runtime, next: C) where C: Continuation<Self::Value> {
        self.process.call(runtime,
            |runtime: &mut Runtime, value: P::Value| {
                next.pause().call(runtime, value);
            });
    }
}

impl<P> ProcessMut for Pause<P> where P: ProcessMut, P::Value: Send {
    fn call_mut<C>(self, runtime: &mut Runtime, next: C)
        where Self: Sized, C: Continuation<(Self, Self::Value)> {
//        self.process.call_mut(runtime,
//                          |runtime: &mut Runtime, (p, v): (P, P::Value)| {
//                              next.pause().call(runtime, (p.pause(), v));
//                          });
        self.process.get_mut().map(|(p, v): (P, P::Value)| {
            (p.pause(), v)
        }).pause().call(runtime, next);
    }
}


/// A process that applies a function to the output of a Process.
pub struct Map<P, F> {
    process: P,
    map: F,
}

impl<P, F, V2> Process for Map<P, F>
    where P: Process, F: FnOnce(P::Value) -> V2 + 'static + Send
{
    type Value = V2;

    fn call<C>(self, runtime: &mut Runtime, next: C) where C: Continuation<Self::Value> {
        let map = self.map;
        self.process.call(runtime,
                          |r: &mut Runtime, v: P::Value| {
                              next.call(r, map(v));
                          });
    }
}

impl<P, F, V2> ProcessMut for Map<P, F>
    where P: ProcessMut, F: FnMut(P::Value) -> V2 + 'static + Send
{
    fn call_mut<C>(self, runtime: &mut Runtime, next: C)
        where Self: Sized, C: Continuation<(Self, Self::Value)>
    {
        let mut map = self.map;
        self.process.call_mut(runtime,
                          move |r: &mut Runtime, (p, v): (P, P::Value)| {
                              let v2 = map(v);
                              next.call(r, (p.map(map), v2));
                          });
    }
}


/// A process that executes the process returned by a Process.
pub struct Flatten<P> {
    process: P,
}

impl<P> Process for Flatten<P>
    where P: Process, P::Value: Process
{
    type Value = <P::Value as Process>::Value;

    fn call<C>(self, runtime: &mut Runtime, next: C) where C: Continuation<Self::Value> {
        self.process.call(runtime, |r: &mut Runtime, v: P::Value|{
           v.call(r, next);
        });
    }
}

impl<P> ProcessMut for Flatten<P>
    where P: ProcessMut, P::Value: Process
{
    fn call_mut<C>(self, runtime: &mut Runtime, next: C)
        where Self: Sized, C: Continuation<(Self, Self::Value)>
    {
        self.process.call_mut(runtime, |r: &mut Runtime, (p, v): (P, P::Value)|{
            v.call(r, |runtime: &mut Runtime, result: Self::Value| {
                next.call(runtime, (p.flatten(), result));
            });
        });
    }
}

type AndThen<P, F> = Flatten<Map<P, F>>;

/// A process that executes two processes sequentially, and return the value of the last process.

pub struct Then<P, Q> {
    process1: P,
    process2: Q,
}

impl <P, Q> Process for Then<P, Q>
    where P: Process, Q: Process
{
    type Value = Q::Value;

    fn call<C>(self, runtime: &mut Runtime, next: C)
        where C: Continuation<Self::Value> + Sized {

        let p2 = self.process2;

        let c = move |runtime: &mut Runtime, _: P::Value| {
            p2.call(runtime, next);
        };

        self.process1.call(runtime, c);
    }
}


impl<P, Q> ProcessMut for Then<P, Q>
    where P: ProcessMut, Q:ProcessMut
{
    fn call_mut<C>(self, runtime: &mut Runtime, next: C)
        where Self: Sized, C: Continuation<(Self, Self::Value)>
    {
        let p2 = self.process2;

        let c = move |runtime: &mut Runtime, v: (P, P::Value)| {
            let (p1, _) = v;

            let c2 = next.map(move |v: (Q, Q::Value)| {
                let (p2, v2) = v;
                (p1.then(p2), v2)
            });
            p2.call_mut(runtime, c2);
        };

        self.process1.call_mut(runtime, c);
    }
}


/// A process that executes two processes in parallel, and returns both values.
pub struct Join<P, Q> {
    process1: P,
    process2: Q,
}

pub struct JoinPoint<V1, V2, C> where C: Continuation<(V1, V2)>, V1: Send, V2: Send {
    v1: Mutex<Option<V1>>,
    v2: Mutex<Option<V2>>,
    continuation: Mutex<Option<C>>,
}

/// Parallel execution of two processes.
impl<P, Q> Process for Join<P, Q>
    where P: Process, Q: Process, P::Value: Send, Q::Value: Send
{
    type Value = (P::Value, Q::Value);

    fn call<C>(self, runtime: &mut Runtime, next: C) where C: Continuation<Self::Value>, C: Sized {
        let join_point = Arc::new(JoinPoint {
            v1: Mutex::new(None), v2: Mutex::new(None), continuation: Mutex::new(Some(next)),
        });
        let join_point2 = join_point.clone();
        let c1 = move |runtime: &mut Runtime, v1: P::Value| {
            let ok;
            {
                let v2 = join_point.v2.lock().unwrap();
                if let Some(_) = *v2 {
                    ok = true;
                }
                else {
                    ok = false;
                }
            }

            if ok {
                let join_point = match Arc::try_unwrap(join_point) {
                    Ok(val) => val,
                    _ => unreachable!("Process join failed"),
                };

                let val = join_point.v2.into_inner().unwrap().unwrap();
                let continuation = join_point.continuation.into_inner().unwrap().unwrap();
                continuation.call(runtime, (v1, val));
            } else {
                *join_point.v1.lock().unwrap() = Some(v1);
            }
        };
        let c2 = move |runtime: &mut Runtime, v2: Q::Value| {
            let ok;
            {
                let v1 = join_point2.v1.lock().unwrap();
                if let Some(_) = *v1 {
                    ok = true;
                } else {
                    ok = false;
                }
            }

            if ok {
                let join_point2 = match Arc::try_unwrap(join_point2) {
                    Ok(val) => val,
                    _ => unreachable!("Process join failed."),
                };

                let val = join_point2.v1.into_inner().unwrap().unwrap();
                let continuation = join_point2.continuation.into_inner().unwrap().unwrap();
                continuation.call(runtime, (val, v2));
            } else {
                *join_point2.v2.lock().unwrap() = Some(v2);
            }
        };
        self.process1.call(runtime, c1);
        self.process2.call(runtime, c2);
    }
}

impl<P, Q> ProcessMut for Join<P, Q>
    where P: ProcessMut, Q: ProcessMut, P::Value: Send, Q::Value: Send {
    fn call_mut<C>(self, runtime: &mut Runtime, next: C)
        where Self: Sized, C: Continuation<(Self, Self::Value)>
    {
        self.process1.get_mut().join(self.process2.get_mut())
            .map(|((p1, v1), (p2, v2))| {
                (p1.join(p2), (v1, v2))
            }).call(runtime, next);
    }
}

pub struct MultiJoin<P> {
    ps: Vec<P>,
}

pub struct MultiJoinPoint<V, C> where C: Continuation<Vec<V>> {
    remaining: Mutex<usize>,
    value: Mutex<Vec<Option<V>>>,
    continuation: Mutex<Option<C>>,
}

pub fn multi_join<P>(ps: Vec<P>) -> MultiJoin<P> {
    MultiJoin { ps }
}

use std::time;

/// Parallel execution of a list of processes.
impl<P> Process for MultiJoin<P>
    where P: Process, P::Value: Send
{
    type Value = Vec<P::Value>;

    /// Launch execution of processes, then calling the `next` continuation when every process has finished.
    fn call<C>(self, runtime: &mut Runtime, next: C) where C: Continuation<Self::Value>, C: Sized {
        // Shared data structure containing worker data.
        let join_point_original = Arc::new(MultiJoinPoint {
            remaining: Mutex::new(self.ps.len()+1),
            value: Mutex::new((0..self.ps.len()).map(|_| { None }).collect()),
            continuation: Mutex::new(Some(next)),
        });

        // List processes to run.
        for (i, p) in self.ps.into_iter().enumerate() {
            // Clone shared data pointer.
            let join_point = join_point_original.clone();
            // Create end of process continuation.
            let c = move |runtime: &mut Runtime, v: P::Value| {
                // Check if someone is still working.
                let ok;
                {
                    let mut remaining = join_point.remaining.lock().unwrap();
                    if *remaining == 1 {
                        ok = true;
                    } else {
                        ok = false;
                    }
                    *remaining -= 1;

                    if !ok { // Free the reference as soon as possible.
                        (*join_point.value.lock().unwrap())[i] = Some(v);
                        return;
                    }
                }
                // Here this is executed when the last process has finished.

                // Wait for remaining references to be free.
                while Arc::strong_count(&join_point) > 1 {
                    thread::sleep(time::Duration::from_millis(10));
                }

                // Get ownership of `join_point`.
                let join_point = match Arc::try_unwrap(join_point) {
                    Ok(val) => val,
                    _ => unreachable!("Process join failed."),
                };

                // Get ownership of processes values and next continuation.
                let mut value = join_point.value.into_inner().unwrap();
                let continuation = join_point.continuation.into_inner().unwrap().unwrap();
                value[i] = Some(v);

                // Call next continuation.
                continuation.call(runtime, value.into_iter().map(|v| { v.unwrap() }).collect());

            };
            runtime.on_current_instant(Box::new(move |runtime: &mut Runtime, _| {
                p.call(runtime, c);
            }));
        };

        // Maybe everything has been done so quickly that `join_point_original` is the last reference to the join point structure.
        // Then we have to do the same work as before by getting ownership of the data and callling the next continuation.

        let ok;
        {
            let mut remaining = join_point_original.remaining.lock().unwrap();
            if *remaining == 1 {
                ok = true;
            } else {
                ok = false;
            }
            *remaining -= 1;
        }

        if ok {
            // Wait for remaining references to be free.
            while Arc::strong_count(&join_point_original) > 1 {
                thread::sleep(time::Duration::from_millis(10));
            }


            let join_point_original = match Arc::try_unwrap(join_point_original) {
                Ok(val) => val,
                _ => unreachable! ("Process join failed."),
            };

            let value = join_point_original.value.into_inner().unwrap();
            let continuation = join_point_original.continuation.into_inner().unwrap().unwrap();
            continuation.call(runtime, value.into_iter().map(| v | { v.unwrap() }).collect());
        }
    }
}

impl<P> ProcessMut for MultiJoin<P>
    where P: ProcessMut, P::Value: Send {
    fn call_mut<C>(self, runtime: &mut Runtime, next: C)
        where Self: Sized, C: Continuation<(Self, Self::Value)> + Sized
    {
        let ps_mut: Vec<Mut<P>> = self.ps.into_iter().map(|p| { p.get_mut() }).collect();
        (MultiJoin { ps: ps_mut }).map(|v: Vec<(P, P::Value)>| {
            let mut ps = vec!();
            let mut res = vec!();
            for (p, value) in v {
                ps.push(p);
                res.push(value);
            }
            (MultiJoin { ps }, res)
        }).call(runtime, next);
    }
}

/// A process that executes `q1` or `q2` depending on `condition` result.
pub struct ThenElse<P, Q1, Q2> {
    condition: P,
    q1: Q1,
    q2: Q2,
}

impl<P, Q1, Q2> Process for ThenElse<P, Q1, Q2>
    where P: Process<Value=bool>, Q1: Process, Q2: Process<Value=Q1::Value>
{
    type Value = Q1::Value;

    fn call<C>(self, runtime: &mut Runtime, next: C) where C: Continuation<Self::Value> {
        let q1 = self.q1;
        let q2 = self.q2;
        self.condition.call(runtime, move |r: &mut Runtime, v: bool| {
            if v {
                q1.call(r, next);
            } else {
                q2.call(r, next);
            }
        });
    }
}

impl<P, Q1, Q2> ProcessMut for ThenElse<P, Q1, Q2>
    where P: ProcessMut<Value=bool>, Q1: ProcessMut, Q2: ProcessMut<Value=Q1::Value>
{
    fn call_mut<C>(self, runtime: &mut Runtime, next: C) where Self: Sized, C: Continuation<(Self, Self::Value)> {
        let q1 = self.q1;
        let q2 = self.q2;
        self.condition.call_mut(runtime, move |r: &mut Runtime, (p, v): (P, bool)| {
            if v {
                q1.call_mut(r, move |r: &mut Runtime, (q1, v)| {
                    next.call(r, (p.then_else(q1, q2), v));
                });
            } else {
                q2.call_mut(r, move |r: &mut Runtime, (q2, v)| {
                    next.call(r,(p.then_else(q1, q2), v));
                });
            }
        });
    }
}

/// A process that executes a ProcessMut once and returns it with the obtained value.
pub struct Mut<P> {
    process: P,
}

impl<P> Process for Mut<P> where P: ProcessMut {
    type Value = (P, P::Value);

    fn call<C>(self, runtime: &mut Runtime, next: C) where C: Continuation<Self::Value> {
        self.process.call_mut(runtime, next);
    }
}


/// Indicates if a loop is finished.

#[derive(Clone)]
pub enum LoopStatus<V> {
    Continue, Exit(V)
}

impl<V> Copy for LoopStatus<V> where V: Copy {

}


pub struct While<P> {
    process: P,
}

impl<P, V> Process for While<P> where P: ProcessMut, P: Process<Value=LoopStatus<V>> {
    type Value = V;

    fn call<C>(self, runtime: &mut Runtime, next: C) where C: Continuation<Self::Value> {
        self.process.call_mut(runtime, |runtime: &mut Runtime, (p, v): (P, P::Value)| {
            match v {
                LoopStatus::Continue => p.loop_while().call(runtime, next),
                LoopStatus::Exit(v) => next.call(runtime, v),
            }
        });
    }
}

impl<P, V> ProcessMut for While<P> where P: ProcessMut, P: Process<Value=LoopStatus<V>> {

    fn call_mut<C>(self, runtime: &mut Runtime, next: C)
        where Self: Sized, C: Continuation<(Self, Self::Value)>
    {
        self.process.call_mut(runtime, |runtime: &mut Runtime, (p, v): (P, P::Value)| {
            match v {
                LoopStatus::Continue => p.loop_while().call_mut(runtime, next),
                LoopStatus::Exit(v) => next.call(runtime, (p.loop_while(), v)),
            }
        });
    }
}
