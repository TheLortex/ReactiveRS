use super::*;
use std::sync::Mutex;

/*
        Value Signal
    The Value Signal is a basic MPMC (Multiple Producer, Multiple Consumer) signal with some value.
    It implements all actions on value: SEmit, SAwaitIn, SAwaitOneImmediate.

    Since there may be multiple consumers, the output type is required to implement Clone trait.
    Since othe emitted values may be read, the input type is required to implement Clone trait.
*/

/// Value Runtime for ValueSignal.
pub struct MCSignalValueRuntime<V1, V2> {
    waiting_in: Mutex<Vec<Box<Continuation<V2>>>>,
    value: Mutex<Option<V2>>,   // We wrap the value in an Option to avoid to clone the value at
                                // each update.
    default: V2,
    last_emitted: Mutex<Option<V1>>,
    gather: Box<(Fn(V1, V2) -> V2) + Send + Sync>,
}

impl<V1, V2> ValueRuntime for MCSignalValueRuntime<V1, V2>
    where V1: Clone + 'static + Send + Sync, V2: Clone + 'static + Send + Sync
{
    type V1 = V1;
    type V2 = V2;

    fn emit(&self, _runtime: &mut Runtime, v: Self::V1) {
        let mut opt_v2 = self.value.lock().unwrap();
        let v2 = unpack_mutex(&mut opt_v2);
        *opt_v2 = Some((self.gather)(v, v2));
    }

    fn await_in<C>(&self, _runtime: &mut Runtime, c:C) where C: Continuation<Self::V2> {
        self.waiting_in.lock().unwrap().push(Box::new(c));
    }

    fn release_await_in(&self, runtime: &mut Runtime) {
        let mut waiting_in = self.waiting_in.lock().unwrap();
        let mut opt_value = self.value.lock().unwrap();
        let value = unpack_mutex(&mut opt_value);
        while let Some(cont) = waiting_in.pop() {
            // Here, we have to clone the signal value to move it each to continuation.
            let v = value.clone();
            runtime.on_current_instant(Box::new(move |r: &mut Runtime, ()| {
                cont.call_box(r, v);
            }));
        }

        // Finally, we reset the signal value.
        *opt_value = Some(self.default.clone());
    }

    fn get(&self) -> Self::V1 {
        let opt_v = self.last_emitted.lock().unwrap();
        opt_v.clone().unwrap()
    }
}


#[derive(Clone)]
/// Basic MPMC signal with a value. Output type must implement Clone.
pub struct MCSignal<V1, V2> where V1: 'static + Clone + Send + Sync, V2: Clone + 'static + Send + Sync {
    signal: SignalRuntimeRef<MCSignalValueRuntime<V1, V2>>,
}

impl<V1, V2> MCSignal<V1, V2> where V1: 'static + Clone + Send + Sync, V2: Clone + Send + Sync {

    /// Creates a new Value Signal from a default value and a combination function `gather`.
    pub fn new<F>(default: V2, gather: F) -> Self
        where F: Fn(V1, V2) -> V2 + 'static, F: Send + Sync
    {
        let value_runtime = MCSignalValueRuntime {
            waiting_in: Mutex::new(vec!()),
            value: Mutex::new(Some(default.clone())),
            default,
            last_emitted: Mutex::new(None),
            gather: Box::new(gather),
        };

        MCSignal { signal: SignalRuntimeRef::new(value_runtime) }
    }
}

impl<V1, V2> Signal for MCSignal<V1, V2> where V1: 'static + Clone + Send + Sync, V2: 'static + Clone + Send + Sync {
    type VR = MCSignalValueRuntime<V1, V2>;

    fn runtime(&self) -> SignalRuntimeRef<Self::VR> {
        self.signal.clone()
    }
}

impl<V1, V2> SEmit for MCSignal<V1, V2> where V1: 'static + Clone + Send + Sync, V2: 'static + Clone + Send + Sync {}
impl<V1, V2> SAwaitIn for MCSignal<V1, V2> where V1: 'static + Clone + Send + Sync, V2: 'static + Clone + Send + Sync {}
impl<V1, V2> SAwaitOneImmediate for MCSignal<V1, V2> where V1: 'static + Clone + Send + Sync, V2: 'static + Clone + Send + Sync {}

