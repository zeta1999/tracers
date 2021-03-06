//! This is the main module for all code generators, both the build-time generators invoked from
//! `build.rs` and the generators used by the proc macros.  There are multiple implementations of
//! these generators for the various tracing implementations, though only one can be active at
//! compile time, via conditonal compilation
use crate::build_rs::BuildInfo;
use crate::error::TracersResult;
use crate::spec::{ProbeCallSpecification, ProviderInitSpecification, ProviderSpecification};
use crate::TracingType;
use proc_macro2::TokenStream;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};

//mod c;
pub(crate) mod common;
pub(crate) mod dynamic;
pub(crate) mod r#static;

/// The native code generation step produces one or more libraries that the tracing application
/// must link to in order to function properly.
///
/// These can be different kinds of libraries which must be handled differently.
///
/// If it's a library that contains C wrapper functions around the native tracing API, that is a
/// static library that is linked via the `#[link]` attribute on the `extern` declaration produced
/// by the code generator.
///
/// However the native tracing API sometimes requires supporting libraries, either static or
/// dynamic.  These must be linked explicitly by printing specially formated links to stdout in the
/// `build.rs` script so Cargo knows where to find the libs.
#[derive(Serialize, Deserialize, Debug, PartialEq, PartialOrd)]
pub(crate) enum NativeLib {
    /// The name (minus `lib` prefix and `.a` or `.lib` suffix) of the generated static wrapper
    /// lib.  This should be linked by the `#[tracer]` macro so it will not be passed to cargo to
    /// link
    StaticWrapperLib(String),

    /// A path where the static wrapper lib should be looked for.  This will be passed to cargo as
    /// one of the native library search paths
    StaticWrapperLibPath(PathBuf),

    /// The name of a dynamically-linked support lib.  This will be explicitly linked to all
    /// targets of the package
    DynamicSupportLib(String),

    /// The name of a statically-linked support lib.  This will be explicitly linked to all
    /// targets of the package
    StaticSupportLib(String),

    /// A path where support libs can be found.  This will be passed to cargo as
    /// one of the native library search paths
    SupportLibPath(PathBuf),
}

/// Each probing implementation must implement this trait, which has components which are called at
/// build-time from `build.rs` and also components invoked by the macros at compile time.  Though
/// invoked in very different contexts, there is much overlap and thus it makes sense to provide
/// them all in one trait implementation.
pub(crate) trait CodeGenerator {
    /// Invoked by the `tracer` attribute macro to process a probing provider declaration and
    /// generate whatever code is required there.
    fn handle_provider_trait(&self, provider: ProviderSpecification) -> TracersResult<TokenStream>;

    /// Invoked by the `probe!` macro to (conditionally) fire a probe.
    fn handle_probe_call(&self, call: ProbeCallSpecification) -> TracersResult<TokenStream>;

    /// Invoked by the `init_provider!` macro to (optionally) initialize the provider, although one
    /// requirement of all implementations is that explicit initialization is not required and will
    /// be done lazily on first use.
    fn handle_init_provider(&self, init: ProviderInitSpecification) -> TracersResult<TokenStream>;

    /// This is invoked from within `build.rs` of the crate which is dependent upon `tracers`.  It
    /// doesn't take much arguments because it interacts directly with cargo via environment
    /// variables and stdout/stderr.
    ///
    /// It is designed not to panic; if there is a hard stop that should cause the dependent crate
    /// to fail, then it returns an error.  Most errors won't be hard stops, but merely warnings
    /// that cause the probing system to switch to a no-nop implementation
    fn generate_native_code(
        &self,
        stdout: &mut dyn Write,
        manifest_dir: &Path,
        out_dir: &Path,
        package_name: &str,
        targets: Vec<PathBuf>,
    ) -> Vec<NativeLib>;
}

/// Loads the `BuildInfo` and based on its contents creates and returns the applicable
/// `CodeGenerator` implementation
pub(crate) fn code_generator() -> TracersResult<Box<dyn CodeGenerator>> {
    let bi = BuildInfo::load()?;

    Ok(match bi.implementation.tracing_type() {
        //There are two implementations: one for static tracing (`disabled` is a special case of
        //`static`), and one for dynamic
        TracingType::Disabled | TracingType::Static => Box::new(r#static::StaticGenerator::new(bi)),
        TracingType::Dynamic => Box::new(dynamic::DynamicGenerator::new(bi)),
    })
}
