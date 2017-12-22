use super::*;

/*
        Pure Signal
    The Pure Signal is a basic unit signal. Its input value is ().
    It only implements SEmit trait, because all other interactions with the signal value are
    equivalent to some interaction with signal status.

    Therefore, the PureSignalValueRuntime implementation is trivial.
*/

/// Value Runtime for PureSignal.
pub struct PureSignalValueRuntime {}

impl ValueRuntime for PureSignalValueRuntime {
    type V1 = ();
    type V2 = ();

    fn emit(&self, _runtime: &mut Runtime, _v: Self::V1) {
        return;
    }

    fn await_in<C>(&self, _runtime: &mut Runtime, _c:C) where C: Continuation<Self::V2> {
        unreachable!()
    }

    fn release_await_in(&self, _runtime: &mut Runtime) {
        unreachable!()
    }

    fn get(&self) -> Self::V2 {
        unreachable!()
    }
}

#[derive(Clone)]
/// Basic unit signal.
pub struct PureSignal {
    signal: SignalRuntimeRef<PureSignalValueRuntime>,
}

impl PureSignal {
    pub fn new() -> PureSignal {
        PureSignal { signal: SignalRuntimeRef::new(PureSignalValueRuntime {}) }
    }
}

impl Signal for PureSignal {
    type VR = PureSignalValueRuntime;

    fn runtime(&self) -> SignalRuntimeRef<Self::VR> {
        self.signal.clone()
    }
}

impl SEmit for PureSignal {}