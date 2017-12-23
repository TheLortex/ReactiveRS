//! Base implementation of SignalRuntime.

use super::*;
use std::sync::{Arc, Mutex};


/// ValueRuntime: part of the SignalRuntime which manipulates the values of the signal.
// It is a trait, since it will be different for each type of signal (Pure signal, MPMC, ...).
pub trait ValueRuntime: Send + Sync {
    /// Input type of the signal (type of emitted values).
    type V1: Send + Sync;

    /// Output type of the signal (type of received values).
    type V2: Send + Sync;

    /// Updates the runtime value with the emission of the value `v`.
    /// Only needs to be implemented if the signal implements the trait SEmit.
    fn emit(&self, runtime: &mut Runtime, v: Self::V1);

    /// Registers the continuation `c` as waiting for the value of the signal.
    /// Only needs to be implemented if the signal implements the trait SAwaitIn.
    fn await_in<C>(&self, runtime: &mut Runtime, c:C) where C: Continuation<Self::V2>;

    /// Calls all the registered continuations waiting for the value of the signal, with the
    /// current value of the signal. They will be executed in the next instant.
    /// Only needs to be implemented if the signal implements the trait SAwaitIn.
    fn release_await_in(&self, runtime: &mut Runtime);

    /// Gets one of the emitted values of the signal, to pass it to continuations which called
    /// `await_one_immediate`.
    /// Only needs to be implemented if the signal implements the trait SAwaitOneImmediate.
    fn get(&self) -> Self::V1;
}


/// Signal Runtime: contains all the information concerning the signal status, and the continuations
/// interacting with this status.
/// Contains a `ValueRuntime`, to handle the value of the signal and the continuations
/// waiting for this value.
pub struct SignalRuntime<VR> where VR: ValueRuntime {
    present: Mutex<bool>,
    waiting_immediate: Mutex<Vec<Box<Continuation<()>>>>,
    waiting_one_immediate: Mutex<Vec<Box<Continuation<VR::V1>>>>,
    testing_present: Mutex<Vec<Box<Continuation<bool>>>>,
    waiting: Mutex<Vec<Box<Continuation<()>>>>,
    value_runtime: VR,
}

impl<VR> SignalRuntime<VR> where VR: ValueRuntime {
    /// Creates a new `SignalRuntime` from `value_runtime`.
    pub fn new(value_runtime: VR) -> Self {
        SignalRuntime {
            present: Mutex::new(false),
            waiting_immediate: Mutex::new(vec!()),
            testing_present: Mutex::new(vec!()),
            waiting: Mutex::new(vec!()),
            waiting_one_immediate: Mutex::new(vec!()),
            value_runtime
        }
    }
}

/// A shared pointer to a signal runtime.
pub struct SignalRuntimeRef<VR> where VR: ValueRuntime {
    pub runtime: Arc<SignalRuntime<VR>>,
}

impl<VR> Clone for SignalRuntimeRef<VR> where VR: ValueRuntime {
    fn clone(&self) -> Self {
        SignalRuntimeRef { runtime: self.runtime.clone() }
    }
}


/// Implements all the possible actions on a Signal Runtime via a SignalRuntimeRef.
impl<VR> SignalRuntimeRef<VR> where VR: ValueRuntime + 'static {

    /// Creates a new `SignalRuntimeRef` from `value_runtime`.
    pub fn new(value_runtime: VR) -> Self {
        SignalRuntimeRef { runtime: Arc::new(SignalRuntime::new(value_runtime)) }
    }

    /// Calls `c` at the first cycle where the signal is present.
    pub fn on_signal<C>(&self, runtime: &mut Runtime, c: C) where C: Continuation<()> {
        if *self.runtime.present.lock().unwrap() {
            // If the signal is present, we call c.
            c.call(runtime, ());
        } else {
            // Otherwise, we register c to be called when signal is emitted.
            self.runtime.waiting_immediate.lock().unwrap().push(Box::new(c));
        }
    }

    /// Calls `c` with the boolean which indicates if the signal is present.
    pub fn present<C>(&self, runtime: &mut Runtime, c: C)
        where C: Continuation<bool>
    {
        if *self.runtime.present.lock().unwrap() {
            // If the signal is present, we call c with true.
            c.call(runtime, true);
        } else {
            // We register c to be called with true when the signal is emitted, or to be
            // called with false at the end_of_instant.

            // First determines if testing_present is empty, and adds the new c to it.
            let empty = {
                let mut testing_present = self.runtime.testing_present.lock().unwrap();
                let b = testing_present.is_empty();
                testing_present.push(Box::new(c));
                b
            };

            // If it was empty, c is the first continuation to be added, so it adds the continuation
            // which will call at the end of instant all the remaining continuations in
            // testing_present with false.
            if empty {
                let sig_runtime_ref = self.clone();
                let c_false = move |r: &mut Runtime, ()| {
                    let mut testing_present = sig_runtime_ref.runtime.testing_present.lock().unwrap();
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
    pub fn await<C>(&self, runtime: &mut Runtime, c:C) where C: Continuation<()>
    {
        self.on_signal(runtime, |r: &mut Runtime, ()| {
            r.on_next_instant(Box::new(c));
        })
    }

    /// Sets the signal as emitted for the current instant, and updates the value of signal with
    /// the new emitted value.
    pub fn emit(&self, runtime: &mut Runtime, value: VR::V1) {
        // We update the signal value with the new emitted one, through the value runtime.
        self.runtime.value_runtime.emit(runtime, value);

        // We update the status of the signal.
        let mut present = self.runtime.present.lock().unwrap();
        if !*present {
            // The signal status changes to true.
            // We first set the signal as emitted.
            *present = true;

            // Then we release all the continuations contained in waiting_immediate.
            let mut waiting_immediate = self.runtime.waiting_immediate.lock().unwrap();
            while let Some(c) = waiting_immediate.pop() {
                runtime.on_current_instant(c);
            }

            // Then we release all the continuations contained in waiting_one_immediate,
            // with the current value of the signal.
            let mut waiting_one_immediate = self.runtime.waiting_one_immediate.lock().unwrap();
            while let Some(c) = waiting_one_immediate.pop() {
                let v = self.runtime.value_runtime.get();
                runtime.on_current_instant(Box::new(move |r: &mut Runtime, ()| {
                    c.call_box(r, v);
                }));
            }

            // Then we release all the continuations contained in testing_present, with true as
            // argument.
            let mut testing_present = self.runtime.testing_present.lock().unwrap();
            while let Some(c) = testing_present.pop() {
                runtime.on_current_instant(Box::new(move |r: &mut Runtime, ()| {
                    c.call_box(r, true);
                }));
            }

            // Since the signal was emitted at this cycle, we will have to release all the waiting
            // continuations at the end_of_instant, and reset the signal presence.
            let sig_runtime_ref = self.clone();
            let end_update = move |r: &mut Runtime, ()| {
                // Resets signal status.
                *sig_runtime_ref.runtime.present.lock().unwrap() = false;

                // Releases all waiting continuations.
                let mut waiting = sig_runtime_ref.runtime.waiting.lock().unwrap();
                while let Some(cont) = waiting.pop() {
                    r.on_current_instant(cont);
                }

                // Also releases all the continuations waiting for the value, through value runtime.
                sig_runtime_ref.runtime.value_runtime.release_await_in(r);
            };

            // Registers this continuation to be called at the end of instant.
            runtime.on_end_of_instant(Box::new(end_update));
        }
    }

    /// Calls `c` at the next cycle after the signal is present, with the value of the signal.
    pub fn await_in<C>(&self, runtime: &mut Runtime, c:C)
        where C: Continuation<VR::V2>
    {
        // Just forwards the action to the value runtime.
        self.runtime.value_runtime.await_in(runtime, c);
    }

    /// Calls `c` at the first cycle where the signal is present, with its current value.
    pub fn await_one_immediate<C>(&self, runtime: &mut Runtime, c: C) where C: Continuation<VR::V1>
    {
        if *self.runtime.present.lock().unwrap() {
            // If the signal is present, we call c we the current value of the signal, that we can
            // get through the value runtime.
            c.call(runtime, self.runtime.value_runtime.get());
        } else {
            // Otherwise, we register c to be called when signal is emitted.
            self.runtime.waiting_one_immediate.lock().unwrap().push(Box::new(c));
        }
    }
}