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
    fn flatten<>(self) -> Flatten<Self>
        where Self: Sized, Self::Value: Process {
        Flatten { process: self }
    }


    /// Creates a new process that executes the first process, applies the given function to the
    /// result, and executes the returned process.
    fn and_then<F, P>(self, function: F) -> AndThen<Self, F>
        where F: FnOnce(Self::Value) -> P + 'static, Self: Sized, P: Process {
        self.map(function).flatten()
    }

    /// Creates a new process that executes the two processes in parallel, and returns the couple of
    /// their return values.
    fn join<P>(self, process: P) -> Join<Self, P> where Self: Sized, P: Sized {
        Join {process1: self, process2: process}
    }
}


pub struct Value<V> {
    value: V,
}

impl<V> Process for Value<V> where V: 'static {
    type Value = V;

    fn call<C>(self, runtime: &mut Runtime, next: C) where C: Continuation<Self::Value> {
        next.call(runtime, self.value);
    }
}

impl<V> Value<V> where V: 'static{
    pub fn new(v: V) -> Self {
        Value {value: v}
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


type AndThen<P, F> = Flatten<Map<P, F>>;


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