use super::Runtime;
use super::continuation::Continuation;
use std::cell::Cell;
use std::rc::Rc;


/// A reactive process.
pub trait Process: 'static {
    /// The value created by the process.
    type Value;

    /// Executes the reactive process in the runtime, calls `next` with the resulting value.
    fn call<C>(self, runtime: &mut Runtime, next: C) where C: Continuation<Self::Value>;

    fn pause(self) -> Pause<Self> where Self: Sized {
        Pause {process: self}
    }

    /// Creates a new process that applies a function to the output value of `Self`.
    fn map<F, V2>(self, map: F) -> Map<Self, F>
        where Self: Sized, F: FnOnce(Self::Value) -> V2 + 'static
    {
        Map { process: self, map }
    }

    /// Creates a new process that executes the process returned by `Self`.
    fn flatten(self) -> Flatten<Self>
        where Self: Sized, Self::Value: Process {
        Flatten { process: self }
    }


    /// Creates a new process that executes the first process, applies the given function to the
    /// result, and executes the returned process.
    fn and_then<F, P>(self, function: F) -> AndThen<Self, F>
        where F: FnOnce(Self::Value) -> P + 'static, Self: Sized, P: Process {
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
    fn join<P>(self, process: P) -> Join<Self, P> where Self: Sized, P: Sized {
        Join {process1: self, process2: process}
    }

    /// Creates a new process that executes process `q1` if the result of `Self` is true, and `q2`
    /// otherwise.
    fn then_else<Q1, Q2>(self, q1: Q1, q2: Q2) -> ThenElse<Self, Q1, Q2>
        where Self: Process<Value=bool> + Sized, Q1: Process, Q2: Process<Value=Q1::Value> {
        ThenElse { condition: self, q1, q2}
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

impl<V> Value<V> where V: 'static {
    pub fn new(v: V) -> Self {
        Value {value: v}
    }
}

pub fn value<V>(v: V) -> Value<V> where V: 'static {
    Value { value: v }
}

impl<V> Process for Value<V> where V: 'static {
    type Value = V;

    fn call<C>(self, runtime: &mut Runtime, next: C) where C: Continuation<Self::Value> {
        next.call(runtime, self.value);
    }
}

impl<V> ProcessMut for Value<V> where V: Copy + 'static {
    fn call_mut<C>(self, runtime: &mut Runtime, next: C)
        where Self: Sized, C: Continuation<(Self, Self::Value)> {
        let v = self.value;
        next.call(runtime, (self, v));
    }
}


pub struct Pause<P> {
    process: P,
}

impl<P> Process for Pause<P> where P: Process {
    type Value = P::Value;

    fn call<C>(self, runtime: &mut Runtime, next: C) where C: Continuation<Self::Value> {
        self.process.call(runtime,
            |runtime: &mut Runtime, value: P::Value| {
                next.pause().call(runtime, value);
            });
    }
}

impl<P> ProcessMut for Pause<P> where P: ProcessMut {
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
    where P: Process, F: FnOnce(P::Value) -> V2 + 'static
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
    where P: ProcessMut, F: FnMut(P::Value) -> V2 + 'static
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

        let c = move |runtime: &mut Runtime, v1: P::Value| {
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
            let (p1, v1) = v;

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

pub struct JoinPoint<V1, V2, C> where C: Continuation<(V1, V2)> {
    v1: Cell<Option<V1>>,
    v2: Cell<Option<V2>>,
    continuation: Cell<Option<C>>,
}

impl<P, Q> Process for Join<P, Q>
    where P: Process, Q: Process
{
    type Value = (P::Value, Q::Value);

    fn call<C>(self, runtime: &mut Runtime, next: C) where C: Continuation<Self::Value>, C: Sized {
        let join_point = Rc::new(JoinPoint {
            v1: Cell::new(None), v2: Cell::new(None), continuation: Cell::new(Some(next)),
        });
        let join_point2 = join_point.clone();
        let c1 = move |runtime: &mut Runtime, v1: P::Value| {
            let v2 = join_point.v2.take();

            if let Some(v2) = v2 {
                join_point.continuation.take().unwrap().call(runtime, (v1, v2));
            }
            else {
                join_point.v1.set(Some(v1));
            }
        };
        let c2 = move |runtime: &mut Runtime, v2: Q::Value| {
            let v1 = join_point2.v1.take();

            if let Some(v1) = v1 {
                join_point2.continuation.take().unwrap().call(runtime, (v1, v2));
            }
                else {
                    join_point2.v2.set(Some(v2));
                }
        };
        self.process1.call(runtime, c1);
        self.process2.call(runtime, c2);
    }
}

impl<P, Q> ProcessMut for Join<P, Q> where P: ProcessMut, Q:ProcessMut {
    fn call_mut<C>(self, runtime: &mut Runtime, next: C)
        where Self: Sized, C: Continuation<(Self, Self::Value)>
    {
        self.process1.get_mut().join(self.process2.get_mut())
            .map(|((p1, v1), (p2, v2))| {
                (p1.join(p2), (v1, v2))
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
pub enum LoopStatus<V> {
    Continue, Exit(V)
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
