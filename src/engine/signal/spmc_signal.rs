use super::*;
use std::sync::Mutex;


/*
        SPMC Signal
    The SPMC Signal (Single Producer, Multiple Consumer) is a signal which can be emitted only once
    but can be received many times. To guarantee this, the signal init function `spmc_signal::new`
    returns two different parts:
    - SPMCSignalSender:     implements SEmitConsume.
    - SPMCSignalReceiver:   implements SAwaitIn.

    Both parts implement Signal trait, so they both allow all actions on signal status.

    NB:
    If it is placed in some immediate loop, each additional emission removes previous emitted value
    of the signal, setting it to the new emitted value.
    In order to prevent this case from happening, we could use some `RepeatableIfNotImmediate`
    marker, but it is not done yet.
*/

/// A runtime for SPMC signals.
pub struct SPMCSignalValueRuntime<V> {
    waiting_in: Mutex<Vec<Box<Continuation<V>>>>,
    value: Mutex<Option<V>>,
}


impl<V> ValueRuntime for SPMCSignalValueRuntime<V> where V: Clone + 'static + Send + Sync {
    type V1 = V;
    type V2 = V;

    fn emit(&self, _runtime: &mut Runtime, v: Self::V1) {
        *(self.value.lock().unwrap()) = Some(v);
    }

    fn await_in<C>(&self, _runtime: &mut Runtime, c:C) where C: Continuation<Self::V2> {
        self.waiting_in.lock().unwrap().push(Box::new(c));
    }

    fn release_await_in(&self, runtime: &mut Runtime) {
        let mut waiting_in = self.waiting_in.lock().unwrap();
        let mut opt_value = self.value.lock().unwrap();
        // This also resets the value of the signal.
        let value = unpack_mutex(&mut opt_value);

        while let Some(cont) = waiting_in.pop() {
            let v = value.clone();
            runtime.on_current_instant(Box::new(move |r: &mut Runtime, ()| {
                cont.call_box(r, v);
            }));
        }
    }

    fn get(&self) -> Self::V1 {
        (self.value.lock().unwrap()).clone().unwrap()
    }
}

#[derive(Clone)]
pub struct SPMCSignalReceiver<V> where V: Clone + 'static + Send + Sync {
    signal: SignalRuntimeRef<SPMCSignalValueRuntime<V>>,
}

pub struct SPMCSignalSender<V> where V: Clone + 'static + Send + Sync {
    signal: SignalRuntimeRef<SPMCSignalValueRuntime<V>>,
}

impl<V> Signal for SPMCSignalReceiver<V> where V: 'static + Clone + Send + Sync {
    type VR = SPMCSignalValueRuntime<V>;

    fn runtime(&self) -> SignalRuntimeRef<Self::VR> {
        self.signal.clone()
    }
}

impl<V> Signal for SPMCSignalSender<V> where V: 'static + Clone + Send + Sync {
    type VR = SPMCSignalValueRuntime<V>;

    fn runtime(&self) -> SignalRuntimeRef<Self::VR> {
        self.signal.clone()
    }
}

impl<V> SAwaitIn for SPMCSignalReceiver<V> where V: 'static + Clone + Send + Sync {}
impl<V> SAwaitOneImmediate for SPMCSignalReceiver<V> where V: 'static + Clone + Send + Sync {}

impl<V> SEmitConsume for SPMCSignalSender<V> where V: 'static + Clone + Send + Sync {}


pub fn new<V>() -> (SPMCSignalSender<V>, SPMCSignalReceiver<V>) where V: Clone + Send + Sync
{
    let value_runtime = SPMCSignalValueRuntime {
        waiting_in: Mutex::new(vec!()),
        value: Mutex::new(None),
    };
    let runtime_ref = SignalRuntimeRef::new(value_runtime);
    (SPMCSignalSender { signal : runtime_ref.clone() },
     SPMCSignalReceiver { signal: runtime_ref })
}
