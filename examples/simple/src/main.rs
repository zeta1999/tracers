//! This is a simple binary which declares and fires some simple probes
//!
//! It's the "hello world" equivalent for tracing
#![deny(warnings)]

use tracers_macros::{probe, tracer};

#[tracer]
trait SimpleProbes {
    fn hello(who: &str);
    fn greeting(greeting: &str, name: &str);
    fn optional_greeting(greeting: &str, name: &Option<&str>);
}

fn main() {
    loop {
        probe!(SimpleProbes::hello("world"));
        probe!(SimpleProbes::greeting("hello", "world"));
        let name = Some("world");
        probe!(SimpleProbes::optional_greeting("hello", &name));
        let name: Option<&str> = None;
        probe!(SimpleProbes::optional_greeting("hello", &name));
    }
}
