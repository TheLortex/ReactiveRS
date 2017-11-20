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
    where P: Process, Q:Process
{
    type Value = Q::Value;

    fn call<C>(self, runtime: &mut Runtime, next: C)
        where C: Continuation<Self::Value>, C: Sized {

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


use std::cell::RefCell;
/// A shared pointer to a signal runtime.
pub struct SignalRuntimeRef<V1, V2> where SignalRuntime<V1, V2>: Sized {
    runtime: Rc<SignalRuntime<V1, V2>>,
}

impl<V1, V2> Clone for SignalRuntimeRef<V1, V2> {
    fn clone(&self) -> Self {
        SignalRuntimeRef { runtime: self.runtime.clone() }
    }
}

/// Runtime for pure signals.
pub struct SignalRuntime<V1, V2> {
    present: RefCell<bool>,
    waiting_immediate: RefCell<Vec<Box<Continuation<()>>>>,
    testing_present: RefCell<Vec<Box<Continuation<bool>>>>,
    waiting: RefCell<Vec<Box<Continuation<V2>>>>,
    value: Cell<Option<V2>>,
    default: V2,
    gather: Box<Fn(V1, V2) -> V2>,
}

impl<V1, V2> SignalRuntimeRef<V1, V2> where V1: 'static, V2: 'static + Clone {
    /// Sets the signal as emitted for the current instant, and updates the value of signal with
    /// the new emitted value.
    fn emit(self, runtime: &mut Runtime, value: V1) {
        let mut present = self.runtime.present.borrow_mut();
        if !*present {
            // We first set the signal as emitted.
            *present = true;

            // Then we release all the continuations contained in waiting_immediate.
            let mut waiting_immediate = self.runtime.waiting_immediate.borrow_mut();
            while let Some(c) = waiting_immediate.pop() {
                runtime.on_current_instant(c);
            }

            // Then we release all the continuations contained in testing_present, with true as
            // argument.
            let mut testing_present = self.runtime.testing_present.borrow_mut();
            while let Some(c) = testing_present.pop() {
                runtime.on_current_instant(Box::new(move |r: &mut Runtime, ()| {
                    c.call_box(r, true);
                }));
            }

            // Since the signal was emitted at this cycle, we will have to release all the waiting
            // continuations at the end_of_instant, and reset the signal presence and value.
            let sig_runtime_ref = self.clone();
            let c = move |r: &mut Runtime, ()| {
                *sig_runtime_ref.runtime.present.borrow_mut() = false;

                let mut waiting = sig_runtime_ref.runtime.waiting.borrow_mut();
                let value = sig_runtime_ref.runtime.value.take().unwrap();
                while let Some(cont) = waiting.pop() {
                    // Here, we have to clone the signal value.
                    // TODO: Solve this for SC signals.
                    let v = value.clone();
                    r.on_current_instant(Box::new(move |r: &mut Runtime, ()| {
                        cont.call_box(r, v);
                    }));
                }

                // Finally, we reset the signal value.
                sig_runtime_ref.runtime.value.set(Some(sig_runtime_ref.runtime.default.clone()));
            };
            runtime.on_end_of_instant(Box::new(c));
        }

        // We update the signal value with the new emitted one.
        let v = self.runtime.value.take().unwrap();
        self.runtime.value.set(Some((self.runtime.gather)(value, v)));
    }

    /// Calls `c` at the first cycle where the signal is present.
    fn on_signal<C>(self, runtime: &mut Runtime, c: C) where C: Continuation<()> {
        if *self.runtime.present.borrow() {
            // If the signal is present, we call c.
            c.call(runtime, ());
        } else {
            // Otherwise, we register c to be called when signal is emitted.
            self.runtime.waiting_immediate.borrow_mut().push(Box::new(c));
        }
    }

    /// Calls `c` with the boolean which indicates if the signal is present.
    fn present<C>(self, runtime: &mut Runtime, c: C)
        where C: Continuation<bool>
    {
        if *self.runtime.present.borrow() {
            // If the signal is present, we call c with true.
            c.call(runtime, true);
        } else {
            // Otherwise, we register c to be called with true when the signal is emitted, or to be
            // called with false at the end_of_instant.
            // To do this, if c is the first continuation to test whether the signal is present,
            // we register a continuation c_false that will call all testing_present continuations
            // with false at the end_of_instant.
            let empty = {
                let mut testing_present = self.runtime.testing_present.borrow_mut();
                let b = testing_present.is_empty();
                testing_present.push(Box::new(c));
                b
            };
            if empty {
                let sig_runtime_ref = self.clone();
                let c_false = move |r: &mut Runtime, ()| {
                    let mut testing_present = sig_runtime_ref.runtime.testing_present.borrow_mut();
                    while let Some(cont) = testing_present.pop() {
                        r.on_current_instant(Box::new(|r: &mut Runtime, ()| {
                            cont.call_box(r, false);
                        }));
                    }
                };
                runtime.on_end_of_instant(Box::new(c_false));
            }
        }
    }

    /// Calls `c` at the next cycle after the signal is present, with the value of the signal.
    fn wait<C>(self, runtime: &mut Runtime, c:C)
        where C: Continuation<V2>
    {
        self.runtime.waiting.borrow_mut().push(Box::new(c));
    }
}


/// A reactive signal.
pub trait Signal: 'static {
    type V1;
    type V2: Clone;

    /// Returns a reference to the signal's runtime.
    fn runtime(self) -> SignalRuntimeRef<Self::V1, Self::V2>;

    /// Returns a process that waits for the next emission of the signal, current instant
    /// included.
    fn await_immediate(self) -> AwaitImmediate<Self> where Self: Sized {
        AwaitImmediate { signal: self.runtime() }
    }

    /// Returns a process that calls `p` if the signal is present, and calls `q` at the next instant
    /// if the signal is not present.
    fn present<P, Q, V>(self, p: P, q: Q) -> Present<P, Q, Self>
        where P: Process<Value=V>, Q: Process<Value=V>, Self: Sized
    {
        Present { signal: self.runtime(), process1: p, process2: q }
    }

    // TODO: add other methods if needed.
}


/// A reactive signal which can be emitted
pub trait SEmit: Signal {

    /// Returns a process that executes `p`, and emits its returned value.
    fn emit<P>(self, p: P) -> Emit<Self, P> where P: Process<Value=Self::V1>, Self: Sized {
        Emit { signal: self.runtime(), process: p }
    }
}


/// A reactive signal whose value can be read.
pub trait SAwait: Signal {

    /// Returns a process that waits for the signal, and at next instant returns its value.
    fn await(self) -> Await<Self> where Self: Sized {
        Await { signal: self.runtime() }
    }
}


#[derive(Clone)]
pub struct AwaitImmediate<S> where S: Signal {
    signal: SignalRuntimeRef<S::V1, S::V2>,
}

impl<S> Process for AwaitImmediate<S> where S: Signal + 'static {
    type Value = ();

    fn call<C>(self, runtime: &mut Runtime, next: C) where C: Continuation<Self::Value> {
        self.signal.on_signal(runtime, next);
    }
}

impl<S> ProcessMut for AwaitImmediate<S> where S: Signal + 'static {
    fn call_mut<C>(self, runtime: &mut Runtime, next: C)
        where Self: Sized, C: Continuation<(Self, Self::Value)>
    {
        let signal = self.signal.clone();
        self.signal.on_signal(runtime, move |r: &mut Runtime, ()| {
            next.call(r, (AwaitImmediate { signal }, ()))
        });
    }
}

pub struct Emit<S, P> where S: Signal {
    signal: SignalRuntimeRef<S::V1, S::V2>,
    process: P,
}

impl<S, P> Process for Emit<S, P> where S: Signal, P: Process<Value=S::V1> {
    type Value = ();

    fn call<C>(self, runtime: &mut Runtime, next: C) where C: Continuation<Self::Value> {
        let signal = self.signal;
        self.process.call(runtime, |r: &mut Runtime, v: S::V1| {
            signal.emit(r, v);
            next.call(r, ());
        });
    }
}

impl<S, P> ProcessMut for Emit<S, P>
    where S: Signal, P: Process<Value=S::V1>, P: ProcessMut
{
    fn call_mut<C>(self, runtime: &mut Runtime, next: C)
        where Self: Sized, C: Continuation<(Self, Self::Value)>
    {
        let signal_copy = self.signal.clone();
        let signal = self.signal;
        self.process.call_mut(runtime, |r: &mut Runtime, (p, v): (P, S::V1)| {
            signal.emit(r, v);
            next.call(r, (Emit { signal: signal_copy, process: p}, ()));
        });
    }
}

pub struct Present<P, Q, S> where S: Signal {
    signal: SignalRuntimeRef<S::V1, S::V2>,
    process1: P,
    process2: Q,
}

impl<P, Q, S, V> Process for Present<P, Q, S>
    where P: Process<Value=V>, Q: Process<Value=V>, S: Signal + 'static, S::V2: Clone
{
    type Value = V;

    fn call<C>(self, runtime: &mut Runtime, next: C) where C: Continuation<Self::Value> {
        let signal = self.signal.clone();
        signal.present(runtime, move |r: &mut Runtime, present: bool| {
            if present {
                self.process1.call(r, next);
            }
            else {
                self.process2.call(r, next);
            }
        });
    }
}

impl<P, Q, S, V> ProcessMut for Present<P, Q, S>
    where P: Process<Value=V> + ProcessMut, Q: Process<Value=V> + ProcessMut, S: Signal + 'static
{
    fn call_mut<C>(self, runtime: &mut Runtime, next: C)
        where Self: Sized, C: Continuation<(Self, Self::Value)>
    {
        let signal = self.signal.clone();
        let signal2 = self.signal.clone();
        signal.present(runtime, move |r: &mut Runtime, present: bool| {
            if present {
                let q = self.process2;
                self.process1.get_mut().map(move |(p, v)| {
                    (Present { signal: signal2, process1: p, process2: q }, v)
                }).call(r, next);
            }
            else {
                let p = self.process1;
                self.process2.get_mut().map(move |(q, v)| {
                    (Present { signal: signal2, process1: p, process2: q }, v)
                }).call(r, next);
            }
        });
    }
}


#[derive(Clone)]
pub struct Await<S> where S: Signal {
    signal: SignalRuntimeRef<S::V1, S::V2>,
}

impl<S> Process for Await<S> where S: Signal + 'static {
    type Value = S::V2;

    fn call<C>(self, runtime: &mut Runtime, next: C) where C: Continuation<Self::Value> {
        self.signal.wait(runtime, next);
    }
}

impl<S> ProcessMut for Await<S> where S: Signal + 'static {
    fn call_mut<C>(self, runtime: &mut Runtime, next: C)
        where Self: Sized, C: Continuation<(Self, Self::Value)>
    {
        let signal = self.signal.clone();
        self.signal.wait(runtime, move |r: &mut Runtime, v| {
            next.call(r, (Await { signal }, v))
        });
    }
}


#[derive(Clone)]
pub struct PureSignal {
    signal: SignalRuntimeRef<(), ()>,
}

impl PureSignal {
    pub fn new() -> PureSignal {
        let runtime = SignalRuntime {
            present: RefCell::new(false),
            waiting_immediate: RefCell::new(vec!()),
            testing_present: RefCell::new(vec!()),
            waiting: RefCell::new(vec!()),
            value: Cell::new(Some(())),
            default: (),
            gather: Box::new(|(), ()| { () }),
        };
        PureSignal {signal: SignalRuntimeRef { runtime: Rc::new(runtime) }}
    }
}

impl Signal for PureSignal {
    type V1 = ();
    type V2 = ();

    fn runtime(self) -> SignalRuntimeRef<Self::V1, Self::V2> {
        self.signal.clone()
    }
}

impl SEmit for PureSignal {}

#[derive(Clone)]
pub struct MCSignal<V1, V2> {
    signal: SignalRuntimeRef<V1, V2>,
}

impl<V1, V2> MCSignal<V1, V2> where V2: Clone {
    pub fn new<F>(default: V2, gather: F) -> Self
        where F: Fn(V1, V2) -> V2 + 'static
    {
        let runtime = SignalRuntime {
            present: RefCell::new(false),
            waiting_immediate: RefCell::new(vec!()),
            testing_present: RefCell::new(vec!()),
            waiting: RefCell::new(vec!()),
            value: Cell::new(Some(default.clone())),
            default,
            gather: Box::new(gather),
        };
        MCSignal {signal: SignalRuntimeRef { runtime: Rc::new(runtime) }}
    }
}

impl<V1, V2> Signal for MCSignal<V1, V2> where V1: 'static, V2: 'static + Clone {
    type V1 = V1;
    type V2 = V2;

    fn runtime(self) -> SignalRuntimeRef<Self::V1, Self::V2> {
        self.signal.clone()
    }
}

impl<V1, V2> SEmit for MCSignal<V1, V2> where V1: 'static, V2: 'static + Clone {}
impl<V1, V2> SAwait for MCSignal<V1, V2> where V1: 'static, V2: 'static + Clone {}