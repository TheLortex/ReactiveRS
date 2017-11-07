use super::Runtime;
use super::continuation::Continuation;

/// A reactive process.
pub trait Process: 'static {
    /// The value created by the process.
    type Value;

    /// Executes the reactive process in the runtime, calls `next` with the resulting value.
    fn call<C>(self, runtime: &mut Runtime, next: C) where C: Continuation<Self::Value>;

    fn pause(self) -> Pause<Self> where Self: Sized {
        Pause {process: self}
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
    fn new(v: V) -> Self {
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
