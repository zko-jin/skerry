use std::collections::HashSet;

use proc_macro::TokenStream;
use proc_macro2::{TokenStream as TokenStream2, TokenTree};
use quote::{ToTokens, format_ident, quote};
use syn::{
    GenericArgument, Ident, Item, ItemFn, ItemMod, Path, PathArguments, ReturnType, Token, Type,
    parse::{Parse, ParseStream},
    parse_macro_input, parse_quote,
    punctuated::Punctuated,
};

/// An attribute macro to automate function-specific error handling.
///
/// Creates a unique error enum for the function named `{FunctionName}Error`.
///
/// # Syntax
/// ```rust
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
    // 1. Resolve Base Path
    let base_path: Path = if attr.is_empty() {
        parse_quote!(crate::errors)
    } else {
        parse_macro_input!(attr as Path)
    };

    let mut input_fn = parse_macro_input!(item as ItemFn);
    let fn_name = &input_fn.sig.ident;

    // 2. Derive Names
    let struct_name_str = fn_name
        .to_string()
        .split('_')
        .map(|s| {
            let mut c = s.chars();
            match c.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            }
        })
        .collect::<String>()
        + "Error";
    let struct_ident = format_ident!("{}", struct_name_str);

    let mut normal_errors = Vec::new();
    let mut passthrough_errors = Vec::new();

    // 3. Token Munching Logic
    if let ReturnType::Type(_, ty) = &input_fn.sig.output {
        if let Type::Path(tp) = ty.as_ref() {
            if let Some(seg) = tp.path.segments.last() {
                if let PathArguments::AngleBracketed(args) = &seg.arguments {
                    if let Some(GenericArgument::Type(error_part)) = args.args.iter().nth(1) {
                        let tokens = error_part.to_token_stream();
                        let mut iter = tokens.into_iter().peekable();

                        if let Some(TokenTree::Group(g)) = iter.next() {
                            if g.delimiter() == proc_macro2::Delimiter::Parenthesis {
                                let mut g_iter = g.stream().into_iter().peekable();
                                while let Some(token) = g_iter.next() {
                                    if let TokenTree::Punct(ref p) = token {
                                        if p.as_char() == '&' {
                                            let mut path_tokens = TokenStream2::new();
                                            while let Some(next) = g_iter.peek() {
                                                if let TokenTree::Punct(p) = next {
                                                    if p.as_char() == ',' {
                                                        break;
                                                    }
                                                }
                                                path_tokens.extend(std::iter::once(
                                                    g_iter.next().unwrap(),
                                                ));
                                            }
                                            passthrough_errors.push(path_tokens);
                                            continue;
                                        }
                                        if p.as_char() == ',' {
                                            continue;
                                        }
                                    }
                                    let mut path_tokens = token.to_token_stream();
                                    while let Some(next) = g_iter.peek() {
                                        if let TokenTree::Punct(p) = next {
                                            if p.as_char() == ',' {
                                                break;
                                            }
                                        }
                                        path_tokens.extend(std::iter::once(g_iter.next().unwrap()));
                                    }
                                    normal_errors.push(path_tokens);
                                }
                            }
                        } else {
                            // Single error case
                            let mut s_iter = error_part.to_token_stream().into_iter().peekable();
                            if let Some(TokenTree::Punct(p)) = s_iter.peek() {
                                if p.as_char() == '&' {
                                    s_iter.next(); // consume &
                                    passthrough_errors.push(s_iter.collect());
                                } else {
                                    normal_errors.push(error_part.to_token_stream());
                                }
                            } else {
                                normal_errors.push(error_part.to_token_stream());
                            }
                        }
                    }
                }
            }
        }
    }

    // 4. Update Function Signature
    if let ReturnType::Type(_, ty) = &mut input_fn.sig.output {
        if let Type::Path(tp) = ty.as_mut() {
            if let Some(seg) = tp.path.segments.last_mut() {
                if let PathArguments::AngleBracketed(args) = &mut seg.arguments {
                    if let Some(arg) = args.args.iter_mut().nth(1) {
                        *arg = parse_quote!(#struct_ident);
                    }
                }
            }
        }
    }

    let expanded = quote! {
        berrors::berrors_internals::create_fn_error!(
            #base_path,
            #struct_ident,
            #fn_name,
            [#(#normal_errors),*],
            [#(#passthrough_errors),*]
        );

        #input_fn
    };

    TokenStream::from(expanded)
}

/// A container macro used to define the boundaries of an error-handling module.
///
/// This macro ensures that all `fn_error` definitions within the module share
/// the same `GlobalErrors` context and `GlobalErrorsIndices` mapping.
///
/// ALL errors returned by fn_error are required to be defined in this module.
///
/// # Syntax
/// ```rust
/// #[error_module]
/// mod errors { // The module itself is stripped by the macro later
///     struct ErrorA;
///     struct ErrorB;
///     struct ErrorC;
/// }
/// ```
#[proc_macro_attribute]
pub fn error_module(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input_mod = parse_macro_input!(item as ItemMod);

    let (_, items) = input_mod
        .content
        .expect("error_module must be used on a module with a body");

    let mut struct_names = Vec::new();
    let mut out_items = Vec::new();

    for item in items {
        if let Item::Struct(s) = &item {
            struct_names.push(s.ident.clone());
        }
        out_items.push(item);
    }

    let expanded = quote! {
        #(#out_items)*

        pub enum Result<T, E> {
            Ok(T),
            Err(E),
        }
        pub use Result::*;

        #[derive(PartialEq, Eq, Clone, Copy)]
        #[repr(u32)]
        pub enum GlobalErrorsIndices {
            #(
                #struct_names,
            )*
        }

        impl GlobalErrorsIndices {
            pub const fn compare(self, other: Self) -> bool {
                self as u32 == other as u32
            }
        }

        impl berrors::berrors_internals::Indexable for GlobalErrorsIndices {}

        #(
            impl berrors::berrors_internals::ErrCode<GlobalErrorsIndices> for #struct_names {
                const CODE: GlobalErrorsIndices = GlobalErrorsIndices::#struct_names;
            }
        )*

        pub enum GlobalErrors<E: berrors::berrors_internals::ComparableError<GlobalErrorsIndices>> {
            #(
                #struct_names(#struct_names),
            )*
            _PHANTOM(std::marker::PhantomData<E>)
        }

        impl<E: berrors::berrors_internals::ComparableError<GlobalErrorsIndices>> GlobalErrors<E> {
            pub const fn code_to_str(code: GlobalErrorsIndices) -> &'static str {
                match code {
                    #(
                        GlobalErrorsIndices::#struct_names => stringify!(#struct_names),
                    )*
                }
            }
        }
    };

    TokenStream::from(expanded)
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
    base_path: Path,
    ty: Ident,
    errors: Vec<Path>,
}

impl Parse for Input {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let base_path: Path;
        let ty: Ident;
        let fn_name: Ident;

        base_path = input.parse()?;
        input.parse::<Token![,]>()?;
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
            base_path,
            ty,
            errors,
        })
    }
}

#[proc_macro]
pub fn create_fn_error_step(input: TokenStream) -> TokenStream {
    let Input {
        fn_name,
        base_path,
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
            (@callback target: [$type:ident], base: [$base:path, $fn_name:ident], accum: [$($acc:path),*], remaining: [$($rem:ident),*]) => {
                berrors::berrors_internals::expand_starred_lists! {
                    @step
                    target: [$type],
                    base: [$base, $fn_name],
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

        impl Into<#base_path::GlobalErrors<#ty>> for #ty {
            fn into(self) -> #base_path::GlobalErrors<#ty> {
                match self {
                    #(
                        #ty::#variants(v) => #base_path::GlobalErrors::#variants(v),
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

            impl<T, I: Into<#variants>> std::ops::FromResidual<I> for #base_path::Result<T, #ty>
            {
                fn from_residual(residual: I) -> #base_path::Result<T, #ty> {
                    Result::Err(#ty::#variants(residual.into()))
                }
            }
        )*

        impl berrors::berrors_internals::ComparableError<#base_path::GlobalErrorsIndices> for #ty {
            const NAME: &'static str = stringify!(#ty);

            const CODES: &'static [#base_path::GlobalErrorsIndices] = &[
                #(
                    <#deduped as berrors::berrors_internals::ErrCode<#base_path::GlobalErrorsIndices>>::CODE,
                )*
            ];
        }

        impl #ty {
            #[track_caller]
            const fn check<E: berrors::berrors_internals::ComparableError<#base_path::GlobalErrorsIndices>>() {
                use berrors::berrors_internals::ComparableError;
                let mut not_found_types = berrors::berrors_internals::ConstantStrVec::new();

                let mut index = 0;
                let len = #ty::CODES.len();
                let mut current_needle = 0;
                let needle_len = E::CODES.len();

                while current_needle < needle_len {
                    if E::CODES[current_needle].compare(#ty::CODES[index]) {
                        current_needle += 1;
                        index = 0;
                        continue;
                    }

                    index += 1;
                    if index == len {
                        let error_type: &'static str =
                            #base_path::GlobalErrors::<E>::code_to_str(E::CODES[current_needle]);
                        not_found_types.push(error_type);
                        current_needle += 1;
                        index = 0;
                    }
                }

                if !not_found_types.is_empty() {
                    const_panic::concat_panic! {
                        "Cannot convert `",
                        display: stringify!(#ty),
                        "` from `",
                        display: E::NAME,
                        "`, missing ",
                        not_found_types.get_slice(),
                    };
                }
            }
        }

        impl<T> std::ops::Try for #base_path::Result<T, #ty> {
            type Output = T;
            type Residual = #base_path::GlobalErrors<#ty>;

            fn from_output(output: Self::Output) -> Self {
                #base_path::Result::Ok(output)
            }

            fn branch(self) -> std::ops::ControlFlow<Self::Residual, Self::Output> {
                match self {
                    #base_path::Result::Ok(t) => std::ops::ControlFlow::Continue(t),
                    #base_path::Result::Err(e) => std::ops::ControlFlow::Break(e.into()),
                }
            }
        }

        impl<T, E: berrors::berrors_internals::ComparableError<#base_path::GlobalErrorsIndices>>
            std::ops::FromResidual<#base_path::GlobalErrors<E>> for #base_path::Result<T, #ty>
        {
            fn from_residual(residual: #base_path::GlobalErrors<E>) -> #base_path::Result<T, #ty> {
                let _ = const { #ty::check::<E>() };
                match residual {
                    #(
                        #base_path::GlobalErrors::#variants(v) => {
                            #base_path::Result::Err(#ty::#variants(v))
                        }
                    )*
                    _ => unreachable!(),
                }
            }
        }
    };

    TokenStream::from(expanded)
}
