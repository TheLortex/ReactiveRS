use super::*;
use std::sync::Mutex;


/*
        MPSC Signal
    The MPSC Signal (Multiple Producer, Single Consumer) is a signal which can be emitted many times
    but can only be received once. To guarantee this, the signal init function `mpsc_signal::new`
    returns two different parts:
    - MPSCSignalSender:     implements SEmit.
    - MPSCSignalReceiver:   implements SAwaitInConsume.

    Both parts implement Signal trait, so they both allow all actions on signal status.
*/

/// Value Runtime for MPSC Signals.
pub struct MPSCSignalValueRuntime<V1, V2> {
    waiting_in: Mutex<Option<Box<Continuation<V2>>>>,
    value: Mutex<Option<V2>>,
    gather: Box<(Fn(V1, V2) -> V2) + Send + Sync>,
}

impl<V1, V2> ValueRuntime for MPSCSignalValueRuntime<V1, V2>
    where V1: Send + Sync, V2: Default + 'static + Send + Sync {
    type V1 = V1;
    type V2 = V2;

    fn emit(&self, _runtime: &mut Runtime, v: Self::V1) {
        let mut opt_v2 = self.value.lock().unwrap();
        let v2 = unpack_mutex(&mut opt_v2);
        *opt_v2 = Some((self.gather)(v, v2));
    }

    fn await_in<C>(&self, _runtime: &mut Runtime, c:C) where C: Continuation<Self::V2> {
        *self.waiting_in.lock().unwrap() = Some(Box::new(c));
    }

    fn release_await_in(&self, runtime: &mut Runtime) {
        let mut waiting_in = self.waiting_in.lock().unwrap();
        let mut opt_value = self.value.lock().unwrap();
        let value = unpack_mutex(&mut opt_value);
        let mut empty = None;
        swap(&mut empty, &mut *waiting_in);

        if let Some(c) = empty {
            runtime.on_current_instant(Box::new(move |r: &mut Runtime, ()| {
                c.call_box(r, value);
            }));
        }

        // Finally, we reset the signal value.
        *opt_value = Some(V2::default());
    }

    fn get(&self) -> V1 {
        unreachable!()
    }
}


#[derive(Clone)]
/// Sender part for MPSC, which is Clone.
pub struct MPSCSignalSender<V1, V2>
    where V1: Send + Sync, V2: Default + 'static + Send + Sync
{
    signal: SignalRuntimeRef<MPSCSignalValueRuntime<V1, V2>>,
}

/// Receiver part for MPSC, which is not Clone.
pub struct MPSCSignalReceiver<V1, V2>
    where V1: Send + Sync, V2: Default + 'static + Send + Sync
{
    signal: SignalRuntimeRef<MPSCSignalValueRuntime<V1, V2>>,
}

impl<V1, V2> Signal for MPSCSignalSender<V1, V2>
    where V1: 'static + Send + Sync, V2: 'static + Default + Send + Sync
{
    type VR = MPSCSignalValueRuntime<V1, V2>;

    fn runtime(&self) -> SignalRuntimeRef<Self::VR> {
        self.signal.clone()
    }
}

impl<V1, V2> Signal for MPSCSignalReceiver<V1, V2>
    where V1: 'static + Send + Sync, V2: 'static + Default + Send + Sync
{
    type VR = MPSCSignalValueRuntime<V1, V2>;

    fn runtime(&self) -> SignalRuntimeRef<Self::VR> {
        self.signal.clone()
    }
}

impl<V1, V2> SEmit for MPSCSignalSender<V1, V2>
    where V1: 'static + Send + Sync, V2: 'static + Default + Send + Sync {}

impl<V1, V2> SAwaitInConsume for MPSCSignalReceiver<V1, V2>
    where V1: 'static + Send + Sync, V2: 'static + Default + Send + Sync {}


pub fn new<V1, V2, F>(gather: F) -> (MPSCSignalSender<V1, V2>, MPSCSignalReceiver<V1, V2>)
    where V1: 'static + Send + Sync, V2: Default + Send + Sync,
          F: Fn(V1, V2) -> V2 + 'static, F: Send + Sync
{
    let value_runtime = MPSCSignalValueRuntime {
        waiting_in: Mutex::new(None),
        value: Mutex::new(Some(V2::default())),
        gather: Box::new(gather),
    };
    let runtime_ref = SignalRuntimeRef::new(value_runtime);
    (MPSCSignalSender {signal: runtime_ref.clone() },
     MPSCSignalReceiver { signal : runtime_ref })
}