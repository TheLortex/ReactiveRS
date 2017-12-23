use super::Runtime;

/// A reactive continuation awaiting a value of type `V`. For the sake of simplicity,
/// continuation must be valid on the static lifetime.
pub trait Continuation<V>: 'static + Send {
    /// Calls the continuation.
    fn call(self, runtime: &mut Runtime, value: V);

    /// Calls the continuation. Works even if the continuation is boxed.
    ///
    /// This is necessary because the size of a value must be known to unbox it. It is
    /// thus impossible to take the ownership of a `Box<Continuation>` whitout knowing the
    /// underlying type of the `Continuation`.
    fn call_box(self: Box<Self>, runtime: &mut Runtime, value: V);

    /// Creates a new continuation that applies a function to the input value before
    /// calling `Self`.
    fn map<F, V2>(self, map: F) -> Map<Self, F> where Self: Sized, F: FnOnce(V2) -> V + 'static {
        Map { continuation: self, map }
    }

    /// Creates a new continuation that waits for the next instant to call `Self`.
    fn pause(self) -> Pause<Self> where Self: Sized {
        Pause { continuation: self }
    }
}


impl<V, F> Continuation<V> for F where F: FnOnce(&mut Runtime, V) + 'static + Send {
    fn call(self, runtime: &mut Runtime, value: V)  {
        self(runtime, value);
    }

    fn call_box(self: Box<Self>, runtime: &mut Runtime, value: V) {
        (*self).call(runtime, value);
    }
}


/// A continuation that applies a function before calling another continuation.
pub struct Map<C, F> {
    continuation: C,
    map: F,
}

impl<C, F, V1, V2> Continuation<V1> for Map<C, F>
    where C: Continuation<V2>, F: FnOnce(V1) -> V2 + 'static + Send
{
    fn call(self, runtime: &mut Runtime, value: V1)  {
        let v = (self.map)(value);
        self.continuation.call(runtime, v);
    }

    fn call_box(self: Box<Self>, runtime: &mut Runtime, value: V1) {
        (*self).call(runtime, value);
    }
}


/// A continuation that waits the next instant to call the continuation.
pub struct Pause<C> {
    continuation: C,
}


impl<C, V> Continuation<V> for Pause<C>
    where C: Continuation<V> + 'static + Send, V: 'static + Send
{
    fn call(self, runtime: &mut Runtime, value: V)  {
        runtime.on_next_instant(Box::new(move |r: &mut Runtime, ()| {
            self.continuation.call(r, value);
        }));

    }

    fn call_box(self: Box<Self>, runtime: &mut Runtime, value: V) {
        (*self).call(runtime, value);
    }
}
