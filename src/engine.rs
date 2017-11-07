use std;

/// A reactive process.
pub trait Process: 'static {
    /// The value created by the process.
    type Value;

    /// Executes the reactive process in the runtime, calls `next` with the resulting value.
    fn call<C>(self, runtime: &mut Runtime, next: C) where C: Continuation<Self::Value>;


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



/// A reactive continuation awaiting a value of type `V`. For the sake of simplicity,
/// continuation must be valid on the static lifetime.
pub trait Continuation<V>: 'static {
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


impl<V, F> Continuation<V> for F where F: FnOnce(&mut Runtime, V) + 'static {
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
    where C: Continuation<V2>, F: FnOnce(V1) -> V2 + 'static
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
    where C: Continuation<V> + 'static, V: 'static
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


/// Runtime for executing reactive continuations.
pub struct Runtime {
    cur_instant:    Vec<Box<Continuation<()>>>,
    next_instant:   Vec<Box<Continuation<()>>>,
    end_of_instant: Vec<Box<Continuation<()>>>,
}


impl Runtime {
    /// Creates a new `Runtime`.
    pub fn new() -> Self {
        Runtime { cur_instant: vec!(), next_instant: vec!(), end_of_instant: vec!() }
    }

    /// Executes instants until all work is completed.
    pub fn execute(&mut self) {
        while self.instant() {
            continue;
        }
    }

    /// Executes a single instant to completion. Indicates if more work remains to be done.
    pub fn instant(&mut self) -> bool {
        // We first execute all the continuations of the instant.
        while let Some(c) = self.cur_instant.pop() {
            c.call_box(self, ());
        }

        // Then, we execute the end of instant continuations
        let mut end_of_instant = vec!();
        std::mem::swap(&mut end_of_instant, &mut self.end_of_instant);
        std::mem::swap(&mut self.cur_instant, &mut self.next_instant);

        while let Some(c) = end_of_instant.pop() {
            c.call_box(self, ());
        }

        !(self.cur_instant.is_empty()
            && self.end_of_instant.is_empty()
            && self.next_instant.is_empty())
    }

    /// Registers a continuation to execute on the current instant.
    fn on_current_instant(&mut self, c: Box<Continuation<()>>) {
        self.cur_instant.push(c);
    }

    /// Registers a continuation to execute at the next instant.
    fn on_next_instant(&mut self, c: Box<Continuation<()>>) {
        self.next_instant.push(c);
    }

    /// Registers a continuation to execute at the end of the instant. Runtime calls for `c`
    /// behave as if they where executed during the next instant.
    fn on_end_of_instant(&mut self, c: Box<Continuation<()>>) {
        self.end_of_instant.push(c);
    }
}



#[cfg(test)]
mod tests {
    use engine::{Runtime, Continuation};

    #[test]
    fn test_42() {
        println!("Hello, world!");

        let continuation_42 = |r: &mut Runtime, v: ()| {
            r.on_next_instant(Box::new(|r: &mut Runtime, v: ()| {
                r.on_next_instant(Box::new(|r: &mut Runtime, v: ()| {
                    println!("42");
                }));
            }));
        };
        let mut r = Runtime::new();
        //    r.on_current_instant(Box::new(continuation_42));
        //    r.execute();

        r.on_current_instant(Box::new(continuation_42));
        r.execute();
        println!("Starting");
        r.instant();
        println!("end of instant 1");
        r.instant();
        println!("end of instant 2");
        r.instant();
        println!("end of instant 3");
    }

    #[test]
    fn test_pause() {
        let c = (|r: &mut Runtime, ()| { println!("42") })
            .pause().pause();

        let mut r = Runtime::new();
        r.on_current_instant(Box::new(c));
        println!("Starting");
        r.instant();
        println!("end of instant 1");
        r.instant();
        println!("end of instant 2");
        r.instant();
        println!("end of instant 3");
    }
}