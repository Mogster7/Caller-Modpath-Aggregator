//! Macros for caller_modpath_aggregator, an overhaul of [repo](https://github.com/Shizcow/caller_modpath)

use proc_macro::TokenStream;
use quote::quote;
use syn::{Block, ItemFn};

// prepend the setup code to the beginning of the input proc_macro
#[proc_macro_attribute]
pub fn expose_caller_modpath(_attr: TokenStream, input: TokenStream) -> TokenStream {
    // for error reporting
    let proc_err = syn::Error::new(
        proc_macro::Span::call_site().into(),
        "#[expose_caller_modpath] can only be used on #[proc_macro_attribute] functions",
    )
        .to_compile_error()
        .into();

    // make sure the format matches
    match syn::parse::<ItemFn>(input) {
        Err(_) => proc_err,
        Ok(input) => {
            // make sure there's #[proc_macro_attribute]
            // This is strictly required due to the rustc meta-call returning early
            if !input
                .attrs
                .clone()
                .into_iter()
                .any(|attr| attr.path.is_ident("proc_macro_attribute"))
            {
                return proc_err;
            }

            // This will be placed at the beginning of the function
            let mut inject = syn::parse2::<Block>(quote! {{
                // Store the name of the function as well
                let item_fn: ItemFn = syn::parse(item.clone()).unwrap();
                let fn_name = &item_fn.sig.ident.to_string();

                caller_modpath_aggregator::append_span(env!("CARGO_CRATE_NAME"), fn_name);

                // Second compilation
                if std::env::var(caller_modpath_aggregator::UUID_ENV_VAR_NAME).is_ok() {
                    return caller_modpath_aggregator::generate_paths();
                }
            }})
                .unwrap();

            // wrap everything back up and return
            let attrs = input.attrs;
            let vis = input.vis;
            let sig = input.sig;
            let mut block = input.block;
            inject.stmts.append(&mut block.stmts);
            (quote! {
                    #(#attrs)*
                    #vis #sig
            #inject
                })
                .into()
        }
    }
}