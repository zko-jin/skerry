use std::collections::HashSet;

use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{
    Ident, Path, Token,
    parse::{Parse, ParseStream},
    parse_macro_input,
    punctuated::Punctuated,
};

mod internal {
    pub mod impl_missing_converts;
    pub mod skerry_fn;
    pub mod skerry_impl;
    pub mod skerry_mod;
}

#[proc_macro]
pub fn impl_missing_converts(input: TokenStream) -> TokenStream {
    crate::internal::impl_missing_converts::impl_missing_converts(input)
}

/// An attribute macro to automate function-specific error handling.
///
/// Creates a unique error enum for the function named `{FunctionName}Error`.
///
/// # Syntax
/// ```rust,ignore
/// # use skerry::*;
/// #[skerry_mod]
/// mod errors {
///     struct ErrorA;
///     struct ErrorB;
///     struct LocalError;
/// }
/// #[skerry_fn]
/// pub fn passthrough() -> Result<(), (ErrorA, ErrorB)> { ... }
/// #[skerry_fn]
/// pub fn perform_task() -> Result<(), (LocalError, &PassthroughErr)> { ... }
/// ```
///
/// This will generate a PassthroughErr and a PerformTaskErr.
///
/// # Error Types
/// * **Normal Path**: (e.g., `LocalError`) Generates a variant in the local
///   enum.
/// * **Ampersand Path (`&`)**: Marks an error as "passthrough." It is verified
///   at compile-time and allowed to bubble up via `?`, but does not become a
///   variant in the generated local enum. For example PerformTask will have the
///   variants `ErrorA`, `ErrorB` and `LocalErr`
///
/// # Macro Deduplication
/// The final generated enum contains no duplicate error variants, feel free to
/// add them for semantic reasons (e.g., &PassThroughErr Contains LocalErr
/// already but you added it again).
#[proc_macro_attribute]
pub fn skerry_fn(attr: TokenStream, item: TokenStream) -> TokenStream {
    match crate::internal::skerry_fn::skerry_fn(attr, item, None) {
        Ok((def, input)) => quote! {
            #def

            #input
        }
        .into(),
        Err(e) => e,
    }
}

#[proc_macro_attribute]
pub fn skerry_impl(attr: TokenStream, item: TokenStream) -> TokenStream {
    crate::internal::skerry_impl::skerry_impl(attr, item)
}

/// A container macro used to define the boundaries of an error-handling module.
///
/// This macro ensures that all `skerry_fn` definitions within the module share
/// the same `GlobalErrors` context and `GlobalErrorsIndices` mapping.
///
/// ALL errors returned by `skerry_fn` are required to be defined in this module.
///
/// # Syntax
/// ```rust,ignore
/// #[skerry_mod]
/// mod errors { // The module itself is stripped by the macro later
///     struct ErrorA;
///     struct ErrorB;
///     struct ErrorC;
/// }
/// ```
#[proc_macro_attribute]
pub fn skerry_mod(attr: TokenStream, item: TokenStream) -> TokenStream {
    crate::internal::skerry_mod::skerry_mod(attr, item)
}

#[proc_macro]
pub fn dedup(input: TokenStream) -> TokenStream {
    let args: Punctuated<Ident, Token![,]> =
        parse_macro_input!(input with Punctuated::parse_terminated);

    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();

    for ident in args {
        if seen.insert(ident.to_string()) {
            out.push(ident);
        }
    }

    TokenStream::from(quote! {
        #(#out),*
    })
}

struct Input {
    fn_name: Ident,
    ty: Ident,
    errors: Vec<Path>,
}

impl Parse for Input {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let ty: Ident;
        let fn_name: Ident;

        ty = input.parse()?;
        input.parse::<Token![,]>()?;
        fn_name = input.parse()?;

        let mut errors = Vec::new();
        while !input.is_empty() {
            if input.peek(Token![,]) {
                input.parse::<Token![,]>()?;
            } else if input.is_empty() {
                break;
            }
            errors.push(input.parse()?);
        }

        Ok(Input {
            fn_name,
            ty,
            errors,
        })
    }
}

#[proc_macro]
pub fn create_fn_error_step(input: TokenStream) -> TokenStream {
    let Input {
        fn_name,
        ty,
        errors,
    } = parse_macro_input!(input as Input);

    let mut seen = HashSet::new();
    let mut deduped = Vec::new();

    for path in errors {
        let key = quote!(#path).to_string();
        if seen.insert(key) {
            deduped.push(path);
        }
    }

    let variants: Vec<Ident> = deduped
        .iter()
        .map(|p| p.segments.last().unwrap().ident.clone())
        .collect();

    let macro_ident = format_ident!("{}_error_list", fn_name);

    let expanded = quote! {
        #[macro_export]
        macro_rules! #macro_ident {
            (@callback target: [$type:ident], base: [$fn_name:ident], accum: [$($acc:path),*], remaining: [$($rem:ident),*]) => {
                skerry::skerry_internals::expand_starred_lists! {
                    @step
                    target: [$type],
                    base: [$fn_name],
                    accum: [
                        $($acc),*
                        ,
                        #( #deduped ),*
                    ],
                    remaining: [$($rem),*]
                }
            };
        }

        pub enum #ty {
            #(
                #variants(#deduped),
            )*
        }

        skerry_impl_missing_errors!(#ty, [#(#variants),*]);

        impl From<#ty> for GlobalErrors<#ty> {
            fn from(val: #ty) -> GlobalErrors<#ty> {
                match val {
                    #(
                        #ty::#variants(v) => GlobalErrors::#variants(v),
                    )*
                }
            }
        }

        impl skerry::skerry_internals::SkerryError for #ty {}
    };

    TokenStream::from(expanded)
}
