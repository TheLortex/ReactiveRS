pub mod signal_runtime; // Contains the definition of SignalRuntime and ValueRuntime.

pub mod puresignal;     // Defines the unit signal: PureSignal.
pub mod value_signal;   // Defines a basic value signal (MPMC): ValueSignal.
pub mod mpsc_signal;    // Defines a MPSC signal.
pub mod spmc_signal;    // Defines a SPMC signal.

use super::Runtime;
use super::continuation::Continuation;
use super::process::{Process, ProcessMut};
use self::signal_runtime::*;
use std::mem::swap;
use std::sync::MutexGuard;


/*
 This file contains all the definitions of the interface with signals.
 It defines the Signal trait, and all derived traits that allow specific actions on signals.
 Each trait provides some methods to convert a signal to a specific type, which implements the
 process trait.

 Thus, it also contains the implementation of trait Process and ProcessMut for each of all these
 various types.
*/

/// Allows to get the value contained in a Mutex, if this is a Option. Replaces the value with None.
/// Used by some specific signals.
pub fn unpack_mutex<V>(x: &mut MutexGuard<Option<V>>) -> V {
    let mut temp = None;
    swap(&mut temp, &mut *x);
    temp.unwrap()
}




/*
    Traits definition
*/

/// A reactive signal.
/// This trait only provides basic actions on signal status, that are allowed on all the signals.
/// It does not provide any action on signal value.
/// More restrictive actions can be allowed through other derived traits (`SEmit`, `SAwaitIn`, ...).
pub trait Signal where {
    type VR: ValueRuntime;

    /// Returns a reference to the signal's runtime.
    fn runtime(&self) -> SignalRuntimeRef<Self::VR>;

    /// Returns a process that waits for the next emission of the signal, current instant included.
    fn await_immediate(&self) -> AwaitImmediate<Self> where Self: Sized {
        AwaitImmediate { signal: self.runtime() }
    }

    /// Returns a process that waits for the instant following the next emission of the signal.
    fn await(&self) -> Await<Self> where Self: Sized {
        Await { signal: self.runtime() }
    }

    /// Returns a process that calls `p` if the signal is present, and calls `q` at the next instant
    /// if the signal is not present.
    fn present<P, Q, V>(&self, p: P, q: Q) -> Present<P, Q, Self>
        where P: Process<Value=V>, Q: Process<Value=V>, Self: Sized
    {
        Present { signal: self.runtime(), process1: p, process2: q }
    }
}


/// A reactive signal which can be emitted.
pub trait SEmit: Signal {

    /// Returns a process that executes `p`, and emits its returned value.
    fn emit<P>(&self, p: P) -> Emit<Self, P> where P: Process<Value=<Self::VR as ValueRuntime>::V1>, Self: Sized {
        Emit { signal: self.runtime(), process: p }
    }
}


/// A reactive signal which can be emitted, but whose emission consumes the signal object.
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


/// A reactive signal whose value can be read, but whose reading consumes the signal object.
pub trait SAwaitInConsume: Signal {

    /// Returns a process that waits for the signal, and at next instant returns its value.
    fn await_in(self) -> AwaitIn<Self> where Self: Sized {
        AwaitIn { signal: self.runtime() }
    }
}


/// A reactive signal whose emissions can be read immediately.
pub trait SAwaitOneImmediate: Signal {

    /// Returns a process that waits for an emission and returns the emitted value.
    fn await_one_immediate(&self) -> AwaitOneImmediate<Self> where Self: Sized {
        AwaitOneImmediate { signal: self.runtime() }
    }
}




/*
    Process and ProcessMut implementations for the aboved used return types.
*/

/*
    AwaitImmediate
*/
/// A process that waits for the next emission of the signal, current instant included.
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


/*
    Await
*/
/// A process that waits for the instant following the next emission of the signal.
pub struct Await<S> where S: Signal {
    signal: SignalRuntimeRef<S::VR>,
}

impl<S> Process for Await<S> where S: Signal + 'static {
    type Value = ();

    fn call<C>(self, runtime: &mut Runtime, next: C) where C: Continuation<Self::Value> {
        self.signal.await(runtime, next);
    }
}

impl<S> ProcessMut for Await<S> where S: Signal + 'static {
    fn call_mut<C>(self, runtime: &mut Runtime, next: C)
        where Self: Sized, C: Continuation<(Self, Self::Value)>
    {
        let signal = self.signal.clone();
        self.signal.await(runtime, move |r: &mut Runtime, ()| {
            next.call(r, (Await { signal }, ()))
        });
    }
}


/*
    Present
*/
/// A process that calls a process if the signal is present, or calls another at the next instant
/// if the signal is not present.
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
            } else {
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
            } else {
                let p = self.process1;
                self.process2.get_mut().map(move |(q, v)| {
                    (Present { signal: signal2, process1: p, process2: q }, v)
                }).call(r, next);
            }
        });
    }
}


/*
    Emit
*/
/// A process that emits the returned value of a process.
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


/*
    AwaitIn
*/
/// A process that waits for the signal, and at next instant returns its value.
pub struct AwaitIn<S> where S: Signal {
    signal: SignalRuntimeRef<S::VR>,
}

impl<S> Process for AwaitIn<S> where S: Signal + 'static {
    type Value = <S::VR as ValueRuntime>::V2;

    fn call<C>(self, runtime: &mut Runtime, next: C) where C: Continuation<Self::Value> {
        self.signal.await_in(runtime, next);
    }
}

impl<S> ProcessMut for AwaitIn<S> where S: Signal + 'static {
    fn call_mut<C>(self, runtime: &mut Runtime, next: C)
        where Self: Sized, C: Continuation<(Self, Self::Value)>
    {
        let signal = self.signal.clone();
        self.signal.await_in(runtime, move |r: &mut Runtime, v| {
            next.call(r, (AwaitIn { signal }, v))
        });
    }
}


/*
    AwaitOneImmediate
*/
/// A process that waits for an emission of the signal, and returns the emitted value.
pub struct AwaitOneImmediate<S> where S: Signal {
    signal: SignalRuntimeRef<S::VR>,
}

impl<S> Process for AwaitOneImmediate<S> where S: Signal + 'static {
    type Value = <S::VR as ValueRuntime>::V1;

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
