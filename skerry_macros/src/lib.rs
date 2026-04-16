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
    pub mod error_module;
    pub mod fn_error;
    pub mod impl_missing_converts;
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
/// #[error_module]
/// mod errors {
///     struct ErrorA;
///     struct ErrorB;
///     struct LocalError;
/// }
/// #[fn_error]
/// pub fn passthrough() -> Result<(), (ErrorA, ErrorB)> { ... }
/// #[fn_error]
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
pub fn fn_error(attr: TokenStream, item: TokenStream) -> TokenStream {
    crate::internal::fn_error::fn_error(attr, item)
}

/// A container macro used to define the boundaries of an error-handling module.
///
/// This macro ensures that all `fn_error` definitions within the module share
/// the same `GlobalErrors` context and `GlobalErrorsIndices` mapping.
///
/// ALL errors returned by fn_error are required to be defined in this module.
///
/// # Syntax
/// ```rust,ignore
/// #[error_module]
/// mod errors { // The module itself is stripped by the macro later
///     struct ErrorA;
///     struct ErrorB;
///     struct ErrorC;
/// }
/// ```
#[proc_macro_attribute]
pub fn error_module(attr: TokenStream, item: TokenStream) -> TokenStream {
    crate::internal::error_module::error_module(attr, item)
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

        impl Into<GlobalErrors<#ty>> for #ty {
            fn into(self) -> GlobalErrors<#ty> {
                match self {
                    #(
                        #ty::#variants(v) => GlobalErrors::#variants(v),
                    )*
                }
            }
        }

        #(
            impl From<#deduped> for #ty {
                fn from(value: #deduped) -> Self {
                    Self::#variants(value)
                }
            }

            // impl<T> From<Result<T, #deduped>> for Result<T, #ty> {
            //     fn from(value: Result<T, #deduped>) -> Self {
            //         match value {
            //             Ok(t) => Ok(t),
            //             Err(e) => Err(e.into()),
            //         }
            //     }
            // }

            // TODO Recheck this later
            // impl<T, I: Into<#variants>> std::ops::FromResidual<I> for #base_path::Result<T, #ty>
            // {
            //     fn from_residual(residual: I) -> #base_path::Result<T, #ty> {
            //         Result::Err(#ty::#variants(residual.into()))
            //     }
            // }
        )*

        impl skerry::skerry_internals::ComparableError for #ty {
            const NAME: &'static str = stringify!(#ty);

            const CODES: &'static [&'static str] = &[
                #(
                    <#deduped as skerry::skerry_internals::ErrCode>::CODE,
                )*
            ];
        }

        // impl #ty {
        //     #[track_caller]
        //     const fn check<E: skerry::skerry_internals::ComparableError>() {
        //         use skerry::skerry_internals::ComparableError;
        //         let mut not_found_types = skerry::skerry_internals::ConstantStrVec::new();

        //         let mut index = 0;
        //         let len = #ty::CODES.len();
        //         let mut current_needle = 0;
        //         let needle_len = E::CODES.len();

        //         while current_needle < needle_len {
        //             if E::CODES[current_needle].compare(#ty::CODES[index]) {
        //                 current_needle += 1;
        //                 index = 0;
        //                 continue;
        //             }

        //             index += 1;
        //             if index == len {
        //                 let error_type: &'static str = E::CODES[current_needle];
        //                 not_found_types.push(error_type);
        //                 current_needle += 1;
        //                 index = 0;
        //             }
        //         }

        //         if !not_found_types.is_empty() {
        //             const_panic::concat_panic! {
        //                 "Cannot convert `",
        //                 display: stringify!(#ty),
        //                 "` from `",
        //                 display: E::NAME,
        //                 "`, missing ",
        //                 not_found_types.get_slice(),
        //             };
        //         }
        //     }
        // }

        impl<T> std::ops::Try for Result<T, #ty> {
            type Output = T;
            type Residual = GlobalErrors<#ty>;

            fn from_output(output: Self::Output) -> Self {
                Result::Ok(output)
            }

            fn branch(self) -> std::ops::ControlFlow<Self::Residual, Self::Output> {
                match self {
                    Result::Ok(t) => std::ops::ControlFlow::Continue(t),
                    Result::Err(e) => std::ops::ControlFlow::Break(e.into()),
                }
            }
        }

    };

    TokenStream::from(expanded)
}
