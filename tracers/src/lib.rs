#![deny(warnings)]

/// The code generated by `tracers-macros` will at runtime require some functionality, both from
/// within this crate but also from third-party crates like `failure`.  It's important that the
/// generated code use _our_ version of these crates, and not be required to add some explicit
/// dependency itself.  So we'll re-export those dependencies here
/// Re-export our two dependencies that are actually used by code in user crates generated by
/// `tracers!` macro.  By re-exporting the crate and not just the types, we ensure the correct
/// version will be used and spare the user having to add these dependencies themselves.  A deeper
/// discussion around this is ongoing right now at:
/// https://github.com/rust-lang-nursery/api-guidelines/issues/176
///
/// The `build.rs` will set a feature to indicate if tracing is enabled at all.  If not then
/// there's no reason to even include this runtime
#[cfg(enabled)]
pub mod runtime {
    pub use tracers_core::failure;
    pub use tracers_core::{wrap, ProbeArgNativeType, ProbeArgType, ProbeArgWrapper};

    #[cfg(dynamic_enabled)]
    pub mod dynamic {
        pub extern crate once_cell;

        pub use tracers_core::dynamic::*;

        // Re-export some types from child crates which callers will need to be able to use.  Ergonomically
        // it makes more sense to a caller to deal with, for example, `tracers::Provider`

        //Alias `SystemTracer` to the appropriate implementation based on the determination made in
        //`build.rs`
        #[cfg(dyn_stap_enabled)]
        pub type SystemTracer = tracers_dyn_stap::StapTracer;

        #[cfg(dyn_noop_enabled)]
        pub type SystemTracer = tracers_noop::NoOpTracer;

        #[cfg(dynamic_enabled)]
        pub type SystemProvider = <SystemTracer as Tracer>::ProviderType;

        #[cfg(dynamic_enabled)]
        pub type SystemProbe = <SystemTracer as Tracer>::ProbeType;
    }
}

#[cfg(test)]
mod test {
    #[cfg(dynamic_enabled)]
    use super::runtime::*;
    #[cfg(dynamic_enabled)]
    use tracers_core::dynamic::Tracer;

    #[test]
    #[cfg(dynamic_enabled)]
    fn verify_expected_dynamic_tracing_impl() {
        //This very simple test checks the TRACERS_EXPECTED_DYNAMIC_IMPL env var, and if set, asserts that
        //the tracing implementation compiled into this library matches the expected one.  In
        //practice this is only used by the CI builds to verify that the compile-time magic always
        //ends up with the expeced implementation on a variety of environments
        if let Ok(expected_impl) = std::env::var("TRACERS_EXPECTED_DYNAMIC_IMPL") {
            assert_eq!(expected_impl, dynamic::SystemTracer::TRACING_IMPLEMENTATION);
        }
    }

    #[test]
    #[cfg(not(dynamic_enabled))]
    fn verify_expected_dynamic_tracing_impl() {
        //This very simple test checks the TRACERS_EXPECTED_DYNAMIC_IMPL env var, and if set, asserts that
        //the tracing implementation compiled into this library matches the expected one.  In
        //practice this is only used by the CI builds to verify that the compile-time magic always
        //ends up with the expeced implementation on a variety of environments
        if let Ok(expected_impl) = std::env::var("TRACERS_EXPECTED_DYNAMIC_IMPL") {
            assert_eq!(expected_impl, "DISABLED",
                       "the crate was compiled with dynamic tracing disabled but apparently the expected implementation was '{}'", expected_impl);
        }
    }
}
