use std::cell::{Cell, RefCell};
use super::Runtime;
use super::continuation::Continuation;
use super::process::{Process, ProcessMut};
use std::mem::swap;
use std::rc::Rc;

pub trait ValueRuntime {
    type V1;
    type V2;

    fn emit(&self, runtime: &mut Runtime, v: Self::V1);
    fn await_in<C>(&self, runtime: &mut Runtime, c:C) where C: Continuation<Self::V2>;
    fn release_await_in(&self, runtime: &mut Runtime);
    fn get(&self) -> Self::V2;
}

pub struct SignalRuntime<VR> where VR: ValueRuntime {
    present: RefCell<bool>,
    waiting_immediate: RefCell<Vec<Box<Continuation<()>>>>,
    waiting_one_immediate: RefCell<Vec<Box<Continuation<VR::V2>>>>,
    testing_present: RefCell<Vec<Box<Continuation<bool>>>>,
    waiting: RefCell<Vec<Box<Continuation<()>>>>,
    value_runtime: VR,
}

/// A shared pointer to a signal runtime.
pub struct SignalRuntimeRef<VR> where VR: ValueRuntime {
    runtime: Rc<SignalRuntime<VR>>,
}

impl<VR> Clone for SignalRuntimeRef<VR> where VR: ValueRuntime {
    fn clone(&self) -> Self {
        SignalRuntimeRef { runtime: self.runtime.clone() }
    }
}


impl<VR> SignalRuntimeRef<VR> where VR: ValueRuntime + 'static {

    /// Calls `c` at the first cycle where the signal is present.
    fn on_signal<C>(&self, runtime: &mut Runtime, c: C) where C: Continuation<()> {
        if *self.runtime.present.borrow() {
            // If the signal is present, we call c.
            c.call(runtime, ());
        } else {
            // Otherwise, we register c to be called when signal is emitted.
            self.runtime.waiting_immediate.borrow_mut().push(Box::new(c));
        }
    }

    /// Calls `c` with the boolean which indicates if the signal is present.
    fn present<C>(&self, runtime: &mut Runtime, c: C)
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

    /// Calls `c` at the next cycle after the signal is present.
    fn await<C>(&self, runtime: &mut Runtime, c:C) where C: Continuation<()>
    {
        self.on_signal(runtime, |r: &mut Runtime, ()| {
            r.on_next_instant(Box::new(c));
        })
    }

    /// Sets the signal as emitted for the current instant, and updates the value of signal with
    /// the new emitted value.
    fn emit(&self, runtime: &mut Runtime, value: VR::V1) {
        // We update the signal value with the new emitted one.
        self.runtime.value_runtime.emit(runtime, value);

        let mut present = self.runtime.present.borrow_mut();
        if !*present {
            // We first set the signal as emitted.
            *present = true;

            // Then we release all the continuations contained in waiting_immediate.
            let mut waiting_immediate = self.runtime.waiting_immediate.borrow_mut();
            while let Some(c) = waiting_immediate.pop() {
                runtime.on_current_instant(c);
            }

            // Then we release all the continuations contained in waiting_one_immediate,
            // with the current value of the signal.
            let mut waiting_one_immediate = self.runtime.waiting_one_immediate.borrow_mut();
            while let Some(c) = waiting_one_immediate.pop() {
                let v = self.runtime.value_runtime.get();
                runtime.on_current_instant(Box::new(move |r: &mut Runtime, ()| {
                    c.call_box(r, v);
                }));
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
            // continuations at the end_of_instant, and reset the signal presence.
            let sig_runtime_ref = self.clone();
            let c = move |r: &mut Runtime, ()| {
                *sig_runtime_ref.runtime.present.borrow_mut() = false;

                let mut waiting = sig_runtime_ref.runtime.waiting.borrow_mut();
                while let Some(cont) = waiting.pop() {
                    r.on_current_instant(cont);
                }

                sig_runtime_ref.runtime.value_runtime.release_await_in(r);
            };
            runtime.on_end_of_instant(Box::new(c));
        }
    }

    /// Calls `c` at the next cycle after the signal is present, with the value of the signal.
    fn wait<C>(&self, runtime: &mut Runtime, c:C)
        where C: Continuation<VR::V2>
    {
        self.runtime.value_runtime.await_in(runtime, c);
    }

    /// Calls `c` at the first cycle where the signal is present, with its current value.
    fn await_one_immediate<C>(&self, runtime: &mut Runtime, c: C) where C: Continuation<VR::V2> {
        if *self.runtime.present.borrow() {
            // If the signal is present, we call c.
            c.call(runtime, self.runtime.value_runtime.get());
        } else {
            // Otherwise, we register c to be called when signal is emitted.
            self.runtime.waiting_one_immediate.borrow_mut().push(Box::new(c));
        }
    }
}

/// A reactive signal.
pub trait Signal where {
    type VR: ValueRuntime;

    /// Returns a reference to the signal's runtime.
    fn runtime(&self) -> SignalRuntimeRef<Self::VR>;

    /// Returns a process that waits for the next emission of the signal, current instant
    /// included.
    fn await_immediate(&self) -> AwaitImmediate<Self> where Self: Sized {
        AwaitImmediate { signal: self.runtime() }
    }

    /// Returns a process that calls `p` if the signal is present, and calls `q` at the next instant
    /// if the signal is not present.
    fn present<P, Q, V>(&self, p: P, q: Q) -> Present<P, Q, Self>
        where P: Process<Value=V>, Q: Process<Value=V>, Self: Sized
    {
        Present { signal: self.runtime(), process1: p, process2: q }
    }

    // TODO: add other methods if needed.
}


/// A reactive signal which can be emitted
pub trait SEmit: Signal {

    /// Returns a process that executes `p`, and emits its returned value.
    fn emit<P>(&self, p: P) -> Emit<Self, P> where P: Process<Value=<Self::VR as ValueRuntime>::V1>, Self: Sized {
        Emit { signal: self.runtime(), process: p }
    }
}

/// A reactive signal which can only be sent at one location in the code.
/// If it is placed in some immediate loop, each additional emission removes previous emitted value
/// of the signal, setting it to the new emitted value.
/// In order to prevent this case from happening, we could use some `RepeatableIfNotImmediate`
/// marker.
pub trait SEmitConsume: Signal {

    /// Returns a process that executes `p`, and emits its returned value.
    fn emit<P>(self, p: P) -> Emit<Self, P> where P: Process<Value=<Self::VR as ValueRuntime>::V1>, Self: Sized {
        Emit { signal: self.runtime(), process: p }
    }
}


/// A reactive signal whose value can be read.
pub trait SAwaitIn: Signal {

    /// Returns a process that waits for the signal, and at next instant returns its value.
    fn await_in(&self) -> AwaitIn<Self> where Self: Sized {
        AwaitIn { signal: self.runtime() }
    }
}

/// A reactive signal whose value can be read only at one location in the code.
pub trait SAwaitInConsume: Signal {

    /// Returns a process that waits for the signal, and at next instant returns its value.
    fn await_in(self) -> AwaitIn<Self> where Self: Sized {
        AwaitIn { signal: self.runtime() }
    }
}

/// A reactive signal whose value can be read immediately.
pub trait SAwaitOneImmediate: Signal {

    /// Returns a process that waits for the signal, and at next instant returns its value.
    fn await_one_immediate(&self) -> AwaitOneImmediate<Self> where Self: Sized {
        AwaitOneImmediate { signal: self.runtime() }
    }
}

#[derive(Clone)]
pub struct AwaitImmediate<S> where S: Signal {
    signal: SignalRuntimeRef<S::VR>,
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
    signal: SignalRuntimeRef<S::VR>,
    process: P,
}

impl<S, P> Process for Emit<S, P> where S: Signal + 'static, P: Process<Value=<S::VR as ValueRuntime>::V1> {
    type Value = ();

    fn call<C>(self, runtime: &mut Runtime, next: C) where C: Continuation<Self::Value> {
        let signal = self.signal;
        self.process.call(runtime, move |r: &mut Runtime, v: <S::VR as ValueRuntime>::V1| {
            signal.emit(r, v);
            next.call(r, ());
        });
    }
}

impl<S, P> ProcessMut for Emit<S, P>
    where S: Signal + 'static, P: Process<Value=<S::VR as ValueRuntime>::V1>, P: ProcessMut
{
    fn call_mut<C>(self, runtime: &mut Runtime, next: C)
        where Self: Sized, C: Continuation<(Self, Self::Value)>
    {
        let signal_copy = self.signal.clone();
        let signal = self.signal;
        self.process.call_mut(runtime, move|r: &mut Runtime, (p, v): (P, <S::VR as ValueRuntime>::V1)| {
            signal.emit(r, v);
            next.call(r, (Emit { signal: signal_copy, process: p}, ()));
        });
    }
}

pub struct Present<P, Q, S> where S: Signal {
    signal: SignalRuntimeRef<S::VR>,
    process1: P,
    process2: Q,
}

impl<P, Q, S, V> Process for Present<P, Q, S>
    where P: Process<Value=V>, Q: Process<Value=V>, S: Signal + 'static
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
pub struct AwaitIn<S> where S: Signal {
    signal: SignalRuntimeRef<S::VR>,
}

impl<S> Process for AwaitIn<S> where S: Signal + 'static {
    type Value = <S::VR as ValueRuntime>::V2;

    fn call<C>(self, runtime: &mut Runtime, next: C) where C: Continuation<Self::Value> {
        self.signal.wait(runtime, next);
    }
}

impl<S> ProcessMut for AwaitIn<S> where S: Signal + 'static {
    fn call_mut<C>(self, runtime: &mut Runtime, next: C)
        where Self: Sized, C: Continuation<(Self, Self::Value)>
    {
        let signal = self.signal.clone();
        self.signal.wait(runtime, move |r: &mut Runtime, v| {
            next.call(r, (AwaitIn { signal }, v))
        });
    }
}

#[derive(Clone)]
pub struct AwaitOneImmediate<S> where S: Signal {
    signal: SignalRuntimeRef<S::VR>,
}

impl<S> Process for AwaitOneImmediate<S> where S: Signal + 'static {
    type Value = <S::VR as ValueRuntime>::V2;

    fn call<C>(self, runtime: &mut Runtime, next: C) where C: Continuation<Self::Value> {
        self.signal.await_one_immediate(runtime, next);
    }
}

impl<S> ProcessMut for AwaitOneImmediate<S> where S: Signal + 'static {
    fn call_mut<C>(self, runtime: &mut Runtime, next: C)
        where Self: Sized, C: Continuation<(Self, Self::Value)>
    {
        let signal = self.signal.clone();
        self.signal.await_one_immediate(runtime, move |r: &mut Runtime, v| {
            next.call(r, (AwaitOneImmediate { signal }, v))
        });
    }
}

pub struct PureSignalValueRuntime {}

impl ValueRuntime for PureSignalValueRuntime {
    type V1 = ();
    type V2 = ();

    fn emit(&self, runtime: &mut Runtime, v: Self::V1) {
        return;
    }

    fn await_in<C>(&self, runtime: &mut Runtime, c:C) where C: Continuation<Self::V2> {
        return;
    }

    fn release_await_in(&self, runtime: &mut Runtime) {
        return;
    }

    fn get(&self) -> Self::V2 {
        unimplemented!()
    }
}

#[derive(Clone)]
pub struct PureSignal {
    signal: SignalRuntimeRef<PureSignalValueRuntime>,
}

impl PureSignal {
    pub fn new() -> PureSignal {
        let runtime = SignalRuntime {
            present: RefCell::new(false),
            waiting_immediate: RefCell::new(vec!()),
            testing_present: RefCell::new(vec!()),
            waiting: RefCell::new(vec!()),
            waiting_one_immediate: RefCell::new(vec!()),
            value_runtime: PureSignalValueRuntime {},
        };
        PureSignal {signal: SignalRuntimeRef { runtime: Rc::new(runtime) }}
    }
}

impl Signal for PureSignal {
    type VR = PureSignalValueRuntime;

    fn runtime(&self) -> SignalRuntimeRef<Self::VR> {
        self.signal.clone()
    }
}

impl SEmit for PureSignal {}


/// Value Runtime for MC Signals.
pub struct MCSignalValueRuntime<V1, V2> {
    waiting_in: RefCell<Vec<Box<Continuation<V2>>>>,
    value: Cell<Option<V2>>,
    default: V2,
    gather: Box<Fn(V1, V2) -> V2>,
}

impl<V1, V2> ValueRuntime for MCSignalValueRuntime<V1, V2> where V2: Clone + 'static {
    type V1 = V1;
    type V2 = V2;

    fn emit(&self, runtime: &mut Runtime, v: Self::V1) {
        let v2 = self.value.take().unwrap();
        self.value.set(Some((self.gather)(v, v2)));
    }

    fn await_in<C>(&self, runtime: &mut Runtime, c:C) where C: Continuation<Self::V2> {
        self.waiting_in.borrow_mut().push(Box::new(c));
    }

    fn release_await_in(&self, runtime: &mut Runtime) {
        let mut waiting_in = self.waiting_in.borrow_mut();
        let value = self.value.take().unwrap();
        while let Some(cont) = waiting_in.pop() {
            // Here, we have to clone the signal value.
            // TODO: Solve this for SC signals.
            let v = value.clone();
            runtime.on_current_instant(Box::new(move |r: &mut Runtime, ()| {
                cont.call_box(r, v);
            }));
        }

        // Finally, we reset the signal value.
        self.value.set(Some(self.default.clone()));
    }

    fn get(&self) -> Self::V2 {
        let v = self.value.take().unwrap();
        self.value.set(Some(v.clone()));
        v
    }
}


#[derive(Clone)]
pub struct MCSignal<V1, V2> where V2: Clone + 'static {
    signal: SignalRuntimeRef<MCSignalValueRuntime<V1, V2>>,
}

impl<V1, V2> MCSignal<V1, V2> where V2: Clone {
    pub fn new<F>(default: V2, gather: F) -> Self
        where F: Fn(V1, V2) -> V2 + 'static
    {
        let value_runtime = MCSignalValueRuntime {
            waiting_in: RefCell::new(vec!()),
            value: Cell::new(Some(default.clone())),
            default,
            gather: Box::new(gather),
        };
        let runtime = SignalRuntime {
            present: RefCell::new(false),
            waiting_immediate: RefCell::new(vec!()),
            testing_present: RefCell::new(vec!()),
            waiting: RefCell::new(vec!()),
            waiting_one_immediate: RefCell::new(vec!()),
            value_runtime
        };
        MCSignal {signal: SignalRuntimeRef { runtime: Rc::new(runtime) }}
    }
}

impl<V1, V2> Signal for MCSignal<V1, V2> where V1: 'static, V2: 'static + Clone {
    type VR = MCSignalValueRuntime<V1, V2>;

    fn runtime(&self) -> SignalRuntimeRef<Self::VR> {
        self.signal.clone()
    }
}

impl<V1, V2> SEmit for MCSignal<V1, V2> where V1: 'static, V2: 'static + Clone {}
impl<V1, V2> SAwaitIn for MCSignal<V1, V2> where V1: 'static, V2: 'static + Clone {}
impl<V1, V2> SAwaitOneImmediate for MCSignal<V1, V2> where V1: 'static, V2: 'static + Clone {}


/// Value Runtime for MPSC Signals.
pub struct MPSCSignalValueRuntime<V1, V2> {
    waiting_in: RefCell<Option<Box<Continuation<V2>>>>,
    value: Cell<Option<V2>>,
    gather: Box<Fn(V1, V2) -> V2>,
}

impl<V1, V2> ValueRuntime for MPSCSignalValueRuntime<V1, V2> where V2: Default + 'static {
    type V1 = V1;
    type V2 = V2;

    fn emit(&self, runtime: &mut Runtime, v: Self::V1) {
        let v2 = self.value.take().unwrap();
        self.value.set(Some((self.gather)(v, v2)));
    }

    fn await_in<C>(&self, runtime: &mut Runtime, c:C) where C: Continuation<Self::V2> {
        *self.waiting_in.borrow_mut() = Some(Box::new(c));
    }

    fn release_await_in(&self, runtime: &mut Runtime) {
        let mut waiting_in = self.waiting_in.borrow_mut();
        let value = self.value.take().unwrap();
        let mut empty = None;
        swap(&mut empty, &mut *waiting_in);

        if let Some(c) = empty {
            runtime.on_current_instant(Box::new(move |r: &mut Runtime, ()| {
                c.call_box(r, value);
            }));
        }

        // Finally, we reset the signal value.
        self.value.set(Some(V2::default()));
    }

    fn get(&self) -> V2 {
        unimplemented!()
    }
}


#[derive(Clone)]
pub struct MPSCSignal<V1, V2> where V2: Default + 'static {
    signal: SignalRuntimeRef<MPSCSignalValueRuntime<V1, V2>>,
}

pub struct MPSCSignalReceiver<V1, V2> where V2: Default + 'static {
    signal: SignalRuntimeRef<MPSCSignalValueRuntime<V1, V2>>,
}

impl<V1, V2> MPSCSignal<V1, V2> where V2: Default {
    pub fn new<F>(gather: F) -> (Self, MPSCSignalReceiver<V1, V2>)
        where F: Fn(V1, V2) -> V2 + 'static
    {
        let value_runtime = MPSCSignalValueRuntime {
            waiting_in: RefCell::new(None),
            value: Cell::new(Some(V2::default())),
            gather: Box::new(gather),
        };
        let runtime = SignalRuntime {
            present: RefCell::new(false),
            waiting_immediate: RefCell::new(vec!()),
            testing_present: RefCell::new(vec!()),
            waiting: RefCell::new(vec!()),
            waiting_one_immediate: RefCell::new(vec!()),
            value_runtime
        };
        let runtime_ref = SignalRuntimeRef { runtime: Rc::new(runtime) };
        (MPSCSignal {signal: runtime_ref.clone() },
         MPSCSignalReceiver { signal : runtime_ref })
    }
}

impl<V1, V2> Signal for MPSCSignal<V1, V2> where V1: 'static, V2: 'static + Default {
    type VR = MPSCSignalValueRuntime<V1, V2>;

    fn runtime(&self) -> SignalRuntimeRef<Self::VR> {
        self.signal.clone()
    }
}

impl<V1, V2> Signal for MPSCSignalReceiver<V1, V2> where V1: 'static, V2: 'static + Default {
    type VR = MPSCSignalValueRuntime<V1, V2>;

    fn runtime(&self) -> SignalRuntimeRef<Self::VR> {
        self.signal.clone()
    }
}

impl<V1, V2> SEmit for MPSCSignal<V1, V2> where V1: 'static, V2: 'static + Default {}

impl<V1, V2> SEmit for MPSCSignalReceiver<V1, V2> where V1: 'static, V2: 'static + Default {}
impl<V1, V2> SAwaitInConsume for MPSCSignalReceiver<V1, V2> where V1: 'static, V2: 'static + Default {}


/// A runtime for SPMC signals.
pub struct SPMCSignalValueRuntime<V> {
    waiting_in: RefCell<Vec<Box<Continuation<V>>>>,
    value: Cell<Option<V>>,
}

impl<V> ValueRuntime for SPMCSignalValueRuntime<V> where V: Clone + 'static {
    type V1 = V;
    type V2 = V;

    fn emit(&self, runtime: &mut Runtime, v: Self::V1) {
        self.value.set(Some(v));
    }

    fn await_in<C>(&self, runtime: &mut Runtime, c:C) where C: Continuation<Self::V2> {
        self.waiting_in.borrow_mut().push(Box::new(c));
    }

    fn release_await_in(&self, runtime: &mut Runtime) {
        let mut waiting_in = self.waiting_in.borrow_mut();
        let value = self.value.take().unwrap();
        while let Some(cont) = waiting_in.pop() {
            let v = value.clone();
            runtime.on_current_instant(Box::new(move |r: &mut Runtime, ()| {
                cont.call_box(r, v);
            }));
        }

        // Finally, we reset the signal value.
        self.value.set(None);
    }

    fn get(&self) -> Self::V2 {
        let v = self.value.take().unwrap();
        self.value.set(Some(v.clone()));
        v
    }
}

#[derive(Clone)]
pub struct SPMCSignal<V> where V: Clone + 'static {
    signal: SignalRuntimeRef<SPMCSignalValueRuntime<V>>,
}

pub struct SPMCSignalSender<V> where V: Clone + 'static {
    signal: SignalRuntimeRef<SPMCSignalValueRuntime<V>>,
}

impl<V> SPMCSignal<V> where V: Clone {
    pub fn new() -> (Self, SPMCSignalSender<V>)
    {
        let value_runtime = SPMCSignalValueRuntime {
            waiting_in: RefCell::new(vec!()),
            value: Cell::new(None),
        };
        let runtime = SignalRuntime {
            present: RefCell::new(false),
            waiting_immediate: RefCell::new(vec!()),
            testing_present: RefCell::new(vec!()),
            waiting: RefCell::new(vec!()),
            waiting_one_immediate: RefCell::new(vec!()),
            value_runtime
        };
        let runtime_ref = SignalRuntimeRef { runtime: Rc::new(runtime) };
        (SPMCSignal {signal: runtime_ref.clone() },
         SPMCSignalSender { signal : runtime_ref })
    }
}

impl<V> Signal for SPMCSignal<V> where V: 'static + Clone {
    type VR = SPMCSignalValueRuntime<V>;

    fn runtime(&self) -> SignalRuntimeRef<Self::VR> {
        self.signal.clone()
    }
}

impl<V> Signal for SPMCSignalSender<V> where V: 'static + Clone {
    type VR = SPMCSignalValueRuntime<V>;

    fn runtime(&self) -> SignalRuntimeRef<Self::VR> {
        self.signal.clone()
    }
}

impl<V> SAwaitIn for SPMCSignal<V> where V: 'static + Clone {}
impl<V> SAwaitOneImmediate for SPMCSignal<V> where V: 'static + Clone {}

impl<V> SAwaitIn for SPMCSignalSender<V> where V: 'static + Clone {}
impl<V> SAwaitOneImmediate for SPMCSignalSender<V> where V: 'static + Clone {}

impl<V> SEmitConsume for SPMCSignalSender<V> where V: 'static + Clone {}