use super::probe::ProbeGenerator;
use crate::provider::ProviderSpecification;
use crate::ProberResult;
use heck::{ShoutySnakeCase, SnakeCase};
use proc_macro2::TokenStream;
use quote::{quote, quote_spanned};
use syn::parse_quote;
use syn::spanned::Spanned;

pub(super) struct ProviderGenerator<'spec> {
    spec: &'spec ProviderSpecification,
    probes: Vec<ProbeGenerator<'spec>>,
}

impl<'spec> ProviderGenerator<'spec> {
    pub fn new(spec: &'spec ProviderSpecification) -> ProviderGenerator<'spec> {
        let probes: Vec<_> = spec
            .probes()
            .iter()
            .map(|pspec| ProbeGenerator::new(pspec))
            .collect();
        ProviderGenerator { spec, probes }
    }

    pub fn generate(&self) -> ProberResult<TokenStream> {
        // Re-generate this trait as a struct with our probing implementation in it
        let prober_struct = self.generate_prober_struct()?;

        // Generate code for a struct and some `OnceCell` statics to hold the instance of the provider
        // and individual probe wrappers
        let impl_mod = self.generate_impl_mod();

        Ok(quote_spanned! { self.spec.item_trait().span() =>
            #prober_struct

            #impl_mod
        })
    }
    /// A provider is described by the user as a `trait`, with methods corresponding to probes.
    /// However it's actually implemented as a `struct` with no member fields, with static methods
    /// implementing the probes.  Thus, given as input the `trait`, we produce a `struct` of the same
    /// name whose implementation actually performs the firing of the probes.
    fn generate_prober_struct(&self) -> ProberResult<TokenStream> {
        // From the probe specifications, generate the corresponding methods that will be on the probe
        // struct.
        let mut probe_methods: Vec<TokenStream> = Vec::new();
        let mod_name = self.get_provider_impl_mod_name();
        let struct_type_name = self.get_provider_impl_struct_type_name();
        let struct_type_path: syn::Path = parse_quote! { #mod_name::#struct_type_name };
        let provider_name = self.spec.name();
        for probe in self.probes.iter() {
            probe_methods.push(probe.generate_trait_methods(
                &self.spec.item_trait().ident,
                &provider_name,
                &struct_type_path,
            )?);
        }

        // Re-generate the trait method that we took as input, with the modifications to support
        // probing
        // This includes constructing documentation for this trait, using whatever doc strings are already applied by
        // the user, plus a section of our own that has information about the provider and how it
        // translates into the various implementations.
        //
        // Hence, the rather awkward `#[doc...]` bits

        let attrs = &self.spec.item_trait().attrs;
        let span = self.spec.item_trait().span();
        let ident = &self.spec.item_trait().ident;
        let vis = &self.spec.item_trait().vis;

        let mod_name = self.get_provider_impl_mod_name();
        let struct_type_name = self.get_provider_impl_struct_type_name();
        let systemtap_comment = format!(
            "This trait corresponds to a SystemTap/USDT provider named `{}`",
            provider_name
        );

        let result = quote_spanned! { span =>
            #(#attrs)*
            #[doc = "# Probing

This trait is translated at compile-time by `probers` into a platform-specific tracing
provider, which allows very high-performance and low-overhead tracing of the probes it
fires.

The exact details of how to use interact with the probes depends on the underlying
probing implementation.

## SystemTap/USDT (Linux x64)
"]
            #[doc = #systemtap_comment]
            #[doc ="
## Other platforms

TODO: No other platforms supported yet
"]
            #vis struct #ident;

            impl #ident {
                #(#probe_methods)*

                /// **NOTE**: This function was generated by the `probers` macro
                ///
                /// Initializes the provider, if it isn't already initialized, and if initialization
                /// failed returns the error.
                ///
                /// # Usage
                ///
                /// Initializing the provider is not required.  By default, each provider will lazily
                /// initialize the first time a probe is fired.  Explicit initialization can be useful
                /// because it ensures that all of a provider's probes are registered and visible to
                /// the platform-specific tracing tools, like `bpftrace` or `tplist` on Linux.
                ///
                /// It's ok to initialize a provider more than once; init operations are idempotent and
                /// if repeated will not do anything
                ///
                /// # Caution
                ///
                /// Callers should not call this method directly.  Instead use the provided
                /// `init_provider!` macro.  This will correctly elide the call when probing is
                /// compile-time disabled.
                ///
                /// # Example
                ///
                /// ```
                /// use probers::{init_provider, prober, probe};
                ///
                /// #[prober]
                /// trait MyProbes {
                ///     fn probe0();
                /// }
                ///
                /// if let Some(err) = init_provider!(MyProbes) {
                ///     eprintln!("Probe provider failed to initialize: {}", err);
                /// }
                ///
                /// //Note that even if the provider fails to initialize, firing probes will never fail
                /// //or panic...
                ///
                /// println!("Firing anyway...");
                /// probe!(MyProbes::probe0());
                /// ```
                #[allow(dead_code)]
                #vis fn __try_init_provider() -> Option<&'static ::probers::failure::Error> {
                    #mod_name::#struct_type_name::get();
                    #mod_name::#struct_type_name::get_init_error()
                }

                /// **NOTE**: This function was generated by the `probers` macro
                ///
                /// If the provider has been initialized, and if that initialization failed, this
                /// method returns the error information.  If the provider was not initialized, this
                /// method does not initialize it.
                ///
                /// # Usage
                ///
                /// In general callers should prefer to use the `init_provider!` macro which wraps a
                /// call to `__try_init_provider`.  Calls to `get_init_error()` directly are necessary
                /// only when the caller specifically wants to avoid triggering initialization of the
                /// provider, but merely to test if initialization was attempted and failed previously.
                #[allow(dead_code)]
                #vis fn __get_init_error() -> Option<&'static ::probers::failure::Error> {
                    #mod_name::#struct_type_name::get_init_error()
                }
            }
        };

        Ok(result)
    }

    /// The implementation of the probing logic is complex enough that it involves the declaration of a
    /// few variables and one new struct type.  All of this is contained within a module, to avoid the
    /// possibility of collissions with other code.  This method generates that module and all its
    /// contents.
    ///
    /// The contents are, briefly:
    /// * The module itself, named after the trait
    /// * A declaration of a `struct` which will hold references to all of the probes
    /// * Multiple static `OnceCell` variables which hold the underlying provider instance as well as
    /// the instance of the `struct` which holds references to all of the probes
    fn generate_impl_mod(&self) -> TokenStream {
        let mod_name = self.get_provider_impl_mod_name();
        let struct_type_name = self.get_provider_impl_struct_type_name();
        let struct_var_name = self.get_provider_impl_struct_var_name();
        let struct_type_params = self.generate_provider_struct_type_params();
        let instance_var_name = self.get_provider_instance_var_name();
        let define_provider_call = self.generate_define_provider_call();
        let provider_var_name = syn::Ident::new("p", self.spec.item_trait().span());
        let struct_members: Vec<_> = self
            .probes
            .iter()
            .map(|probe| probe.generate_struct_member_declaration())
            .collect();

        let struct_initializers: Vec<_> = self
            .probes
            .iter()
            .map(|probe| probe.generate_struct_member_initialization(&provider_var_name))
            .collect();

        quote_spanned! { self.spec.item_trait().span() =>
            mod #mod_name {
                use ::probers::failure::{bail, Fallible};
                use ::probers::{SystemTracer,SystemProvider,Provider};
                use ::probers::{ProviderBuilder,Tracer};
                use ::probers::once_cell::sync::OnceCell;

                #[allow(dead_code)]
                pub(super) struct #struct_type_name<#struct_type_params> {
                    #(pub #struct_members),*
                }

                unsafe impl<#struct_type_params> Send for #struct_type_name<#struct_type_params> {}
                unsafe impl<#struct_type_params> Sync for #struct_type_name <#struct_type_params>{}

                static #instance_var_name: OnceCell<Fallible<SystemProvider>> = OnceCell::INIT;
                static #struct_var_name: OnceCell<Fallible<#struct_type_name>> = OnceCell::INIT;
                static IMPL_OPT: OnceCell<Option<&'static #struct_type_name>> = OnceCell::INIT;

                impl<#struct_type_params> #struct_type_name<#struct_type_params> {
                   pub(super) fn get_init_error() -> Option<&'static failure::Error> {
                        //Don't do a whole re-init cycle again, but if the initialization has happened,
                        //check for failure
                        #struct_var_name.get().and_then(|fallible|  fallible.as_ref().err() )
                   }

                   #[allow(dead_code)]
                   pub(super) fn get() -> Option<&'static #struct_type_name<#struct_type_params>> {
                       let imp: &'static Option<&'static #struct_type_name> = IMPL_OPT.get_or_init(|| {
                           // The reason for this seemingly-excessive nesting is that it's possible for
                           // both the creation of `SystemProvider` or the subsequent initialization of
                           // #struct_type_name to fail with different and also relevant errors.  By
                           // separting them this way we're able to preserve the details about any init
                           // failures that happen, while at runtime when firing probes it's a simple
                           // call of a method on an `Option<T>`.  I don't have any data to back this
                           // up but I suspect that allows for better optimizations, since we know an
                           // `Option<&T>` is implemented as a simple pointer where `None` is `NULL`.
                           let imp = #struct_var_name.get_or_init(|| {
                               // Initialzie the `SystemProvider`, capturing any initialization errors
                               let #provider_var_name: &Fallible<SystemProvider> = #instance_var_name.get_or_init(|| {
                                    #define_provider_call
                               });

                               // Transform this #provider_var_name into an owned `Fallible` containing
                               // references to `T` or `E`, since there's not much useful you can do
                               // with just a `&Result`.
                               match #provider_var_name.as_ref() {
                                   Err(e) => bail!("Provider initialization failed: {}", e),
                                   Ok(#provider_var_name) => {
                                       // Proceed to create the struct containing each of the probes'
                                       // `ProviderProbe` instances
                                       Ok(
                                           #struct_type_name{
                                               #(#struct_initializers,)*
                                           }
                                       )
                                   }
                               }
                           });

                           //Convert this &Fallible<..> into an Option<&T>
                           imp.as_ref().ok()
                       });

                       //Copy this `&Option<&T>` to a new `Option<&T>`.  Since that should be
                       //implemented as just a pointer, this should be effectively free
                       *imp
                   }
                }
            }
        }
    }

    /// A `Provider` is built by calling `define_provider` on a `Tracer` implementation.
    /// `define_provider` takes a closure and passes a `ProviderBuilder` parameter to that closure.
    /// This method generates the call to `SystemTracer::define_provider`, and includes code to add
    /// each of the probes to the provider
    fn generate_define_provider_call(&self) -> TokenStream {
        let builder = syn::Ident::new("builder", self.spec.item_trait().ident.span());
        let add_probe_calls: Vec<TokenStream> = self
            .probes
            .iter()
            .map(|probe| probe.generate_add_probe_call(&builder))
            .collect();
        let provider_name = self.spec.name();

        quote_spanned! { self.spec.item_trait().span() =>
            // The provider name must be chosen carefully.  As of this writing (2019-04) the `bpftrace`
            // and `bcc` tools have, shall we say, "evolving" support for USDT.  As of now, with the
            // latest git version of `bpftrace`, the provider name can't have dots or colons.  For now,
            // then, the provider name is just the name of the provider trait, converted into
            // snake_case for consistency with USDT naming conventions.  If two modules in the same
            // process have the same provider name, they will conflict and some unspecified `bad
            // things` will happen.
            let provider_name = #provider_name;

            SystemTracer::define_provider(&provider_name, |mut #builder| {
                #(#add_probe_calls)*

                Ok(builder)
            })
        }
    }

    /// The provider struct we declare to hold the probe objects needs to take a lot of type
    /// parameters.  One type, 'a, which corresponds to the lifetime parameter of the underling
    /// `ProviderProbe`s, and also one lifetime parameter for every reference argument of every probe
    /// method.
    ///
    /// The return value of this is a token stream consisting of all of the types, but not including
    /// the angle brackets.
    fn generate_provider_struct_type_params(&self) -> TokenStream {
        // Make a list of all of the reference param lifetimes of all the probes
        let probe_lifetimes: Vec<syn::Lifetime> = self
            .probes
            .iter()
            .map(|p| p.args_lifetime_parameters())
            .flatten()
            .collect();

        //The struct simply takes all of these lifetimes plus 'a
        quote! {
            'a, #(#probe_lifetimes),*
        }
    }

    /// Returns the name of the module in which most of the implementation code for this trait will be
    /// located.
    fn get_provider_impl_mod_name(&self) -> syn::Ident {
        let snake_case_name = format!("{}Provider", self.spec.item_trait().ident).to_snake_case();

        syn::Ident::new(
            &format!("__{}", snake_case_name),
            self.spec.item_trait().ident.span(),
        )
    }

    /// The name of the struct type within the impl module which represents the provider, eg `MyProbesProviderImpl`.
    /// Note that this is not the same as the struct which we generate which has the same name as the
    /// trait and implements its methods.
    fn get_provider_impl_struct_type_name(&self) -> syn::Ident {
        crate::syn_helpers::add_suffix_to_ident(&self.spec.item_trait().ident, "ProviderImpl")
    }

    /// The name of the static variable which contains the singleton instance of the provider struct,
    /// eg MYPROBESPROVIDERIMPL
    fn get_provider_impl_struct_var_name(&self) -> syn::Ident {
        syn::Ident::new(
            &format!("{}ProviderImpl", self.spec.item_trait().ident).to_shouty_snake_case(),
            self.spec.item_trait().span(),
        )
    }

    /// The name of the static variable which contains the singleton instance of the underlying tracing
    /// system's `Provider` instance, eg MYPROBESPROVIDER
    fn get_provider_instance_var_name(&self) -> syn::Ident {
        syn::Ident::new(
            &format!("{}Provider", self.spec.item_trait().ident).to_shouty_snake_case(),
            self.spec.item_trait().span(),
        )
    }
}