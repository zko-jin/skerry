use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{
    format_ident,
    quote,
};
use syn::{
    GenericArgument,
    Ident,
    Item,
    ItemEnum,
    Path,
    PathArguments,
    ReturnType,
    Signature,
    Token,
    Type,
    Visibility,
    parse::{
        Parse,
        ParseStream,
    },
    parse_macro_input,
    parse_quote,
    punctuated::Punctuated,
    spanned::Spanned as _,
};

use crate::internal::skerry_fn::{
    format_camel_case,
    process_inner_errors,
    quote_error_gen,
};

mod internal {
    pub mod skerry_fn;
    pub mod skerry_impl;
    pub mod skerry_mod;
    pub mod skerry_trait;
}
struct SkerryErrorList {
    simple: Vec<Ident>,
    composite: Vec<Ident>,
}

impl Parse for SkerryErrorList {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let items: Punctuated<ErrorItem, Token![,]> =
            input.parse_terminated(ErrorItem::parse, Token![,])?;

        if items.is_empty() {
            return Err(syn::Error::new(
                input.span(),
                "Should contain at least one element",
            ));
        }

        let mut simple = Vec::new();
        let mut composite = Vec::new();

        for item in items {
            match item {
                ErrorItem::Simple(id) => simple.push(id),
                ErrorItem::Composite(id) => composite.push(id),
            }
        }

        Ok(SkerryErrorList { simple, composite })
    }
}

enum ErrorItem {
    Simple(Ident),
    Composite(Ident),
}

impl Parse for ErrorItem {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        if input.peek(Token![*]) {
            input.parse::<Token![*]>()?;
            Ok(ErrorItem::Composite(input.parse()?))
        } else {
            Ok(ErrorItem::Simple(input.parse()?))
        }
    }
}

struct SkerryExpandInput {
    error_ident: Ident,
    simple_list: Vec<Ident>,
    composite_list: Vec<Ident>,
    seen_composite_list: Vec<Ident>,
}

impl Parse for SkerryExpandInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let error_ident: Ident = input.parse()?;
        input.parse::<Token![,]>()?;

        let simple_list = parse_bracketed_list(input)?;
        input.parse::<Token![,]>()?;

        let composite_list = parse_bracketed_list(input)?;
        input.parse::<Token![,]>()?;

        let seen_composite_list = parse_bracketed_list(input)?;

        Ok(SkerryExpandInput {
            error_ident,
            simple_list,
            composite_list,
            seen_composite_list,
        })
    }
}

fn parse_bracketed_list(input: ParseStream) -> syn::Result<Vec<Ident>> {
    let content;

    syn::bracketed!(content in input);

    let list: Punctuated<Ident, Token![,]> = content.parse_terminated(Ident::parse, Token![,])?;

    Ok(list.into_iter().collect())
}

#[proc_macro]
pub fn skerry_expand(input: TokenStream) -> TokenStream {
    let SkerryExpandInput {
        error_ident,
        simple_list,
        composite_list,
        mut seen_composite_list,
    } = parse_macro_input!(input as SkerryExpandInput);

    let seen_names: std::collections::HashSet<String> = seen_composite_list
        .iter()
        .map(|id| id.to_string())
        .collect();

    let mut remaining_composites: Vec<_> = composite_list
        .into_iter()
        .filter(|id| !seen_names.contains(&id.to_string()))
        .collect();

    let expansion_step = if let Some(next_target) = remaining_composites.pop() {
        seen_composite_list.push(next_target.clone());

        let next_macro_name = format_ident!("__SkerryPrivate_{}_Expand", next_target);

        quote! {
            #next_macro_name!(
                #error_ident,
                [#(#simple_list),*],
                [#(#remaining_composites),*],
                [#(#seen_composite_list),*]
            );
        }
    } else {
        let mut seen_simples = std::collections::HashSet::new();
        let simple_list: Vec<_> = simple_list
            .into_iter()
            .filter(|id| seen_simples.insert(id.to_string()))
            .collect();
        let contains_impls = simple_list.iter().map(|v| {
            quote! {
                impl skerry::skerry_internals::Contains<__skerry_error_tag!(#v)> for #error_ident {}
            }
        });

        let mod_name = format_ident!("__skerry_private_{}", error_ident);
        let not_ident = format_ident!("Not{}", error_ident);

        let subset_bounds = simple_list.iter().map(|v| {
            quote! { T: skerry::skerry_internals::Contains<__skerry_error_tag!(#v)> }
        });
        quote! {
            __skerry_global_error_layout!(START pub, #error_ident, [#(#simple_list),*]);

            mod #mod_name {
                pub auto trait #not_ident {}
                impl !#not_ident for super::#error_ident {}
            }

            impl From<#error_ident> for __skerry_global_error_ident!() {
                fn from(val: #error_ident) -> Self {
                    __skerry_global_error_layout_convert!(START val,#error_ident,Self,[#(#simple_list),*])
                }
            }

            #(
                #contains_impls
            )*

            impl<T> skerry::skerry_internals::IsSubsetOf<T> for #error_ident
            where
                #(#subset_bounds),*
            {}

                impl<E: Into<__skerry_global_error_ident!()> + skerry::skerry_internals::IsSubsetOf<#error_ident> +
                    #mod_name::#not_ident> From<E> for #error_ident {
                        fn from (val: E) -> Self {
                            __skerry_global_error_ident_import!(__LocalGlobalErrors);
                            let val = val.into();
                            __skerry_global_error_layout_convert!(START val,__LocalGlobalErrors,#error_ident,[#(#simple_list),*])
                        }
                    }
        }
    };

    quote! {
        #expansion_step
    }
    .into()
}

fn process_fn(
    sig: &mut Signature,
    prefix: &str,
    empty_on_fail_extract: bool,
) -> syn::Result<TokenStream2> {
    let fn_name = sig.ident.to_string();
    let fn_camel_case = format_camel_case(&fn_name);
    let enum_ident = format_ident!("{}{}Error", prefix, fn_camel_case);

    let error_tokens = match extract_e_macro_tokens(&sig.output) {
        Ok(t) => t,
        Err(e) => {
            if empty_on_fail_extract {
                return Ok(TokenStream2::new());
            } else {
                return Ok(e.into_compile_error());
            }
        }
    };
    let SkerryErrorList { simple, composite } = syn::parse2(error_tokens)?;

    if let ReturnType::Type(_, ref mut ty) = sig.output {
        if let Type::Path(ref mut tp) = **ty {
            if let Some(last) = tp.path.segments.last_mut() {
                if let PathArguments::AngleBracketed(ref mut args) = last.arguments {
                    if let Some(GenericArgument::Type(second_arg)) = args.args.last_mut() {
                        *second_arg = parse_quote!(#enum_ident);
                    }
                }
            }
        }
    }

    let expand_macro_name = format_ident!("__SkerryPrivate_{}_Expand", enum_ident);

    let output = quote! {
        #[macro_export]
        macro_rules! #expand_macro_name {
            ($target_ident:ident, [$($s:ident),*], [$($c:ident),*], [$($seen:ident),*]) => {
                skerry::skerry_internals::skerry_expand!(
                    $target_ident,
                    [$($s,)* #(#simple),*],
                    [$($c,)* #(#composite),*],
                    [$($seen),*]
                );
            };
        }

        // TODO: add vis pass
        skerry::skerry_internals::skerry_expand!(
            #enum_ident,
            [#(#simple),*],
            [#(#composite),*],
            [#enum_ident]
        );
    };

    Ok(output)
}

#[proc_macro_attribute]
pub fn skerry(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as Item);

    match input {
        Item::Fn(mut item_fn) => {
            let expansion = match process_fn(&mut item_fn.sig, "", false) {
                Ok(v) => v,
                Err(e) => return e.into_compile_error().into(),
            };
            quote! {
                #item_fn

                #expansion
            }
            .into()
        }
        Item::Impl(mut item_impl) => {
            if item_impl.trait_.is_some() {
                return syn::Error::new(
                    proc_macro2::Span::call_site(),
                    "#[skerry] does not support trait implementation blocks. \
                    Instead call #[skerry] in your trait definition.",
                )
                .into_compile_error()
                .into();
            }
            let mut expand = quote! {};
            let prefix = match &*item_impl.self_ty {
                Type::Path(path) => path
                    .path
                    .get_ident()
                    .map_or("".to_string(), |i| i.to_string()),
                _ => "".into(),
            };
            for impl_item in &mut item_impl.items {
                match impl_item {
                    syn::ImplItem::Fn(impl_item_fn) => {
                        expand.extend(process_fn(&mut impl_item_fn.sig, &prefix, true))
                    }
                    _ => {}
                }
            }
            quote! {
                #expand
                #item_impl
            }
            .into()
        }
        // Item::Mod(item_mod) => todo!(),
        Item::Trait(mut item_trait) => {
            let mut expand = quote! {};
            let prefix = item_trait.ident.to_string();
            for trait_item in &mut item_trait.items {
                match trait_item {
                    syn::TraitItem::Fn(trait_item_fn) => {
                        expand.extend(process_fn(&mut trait_item_fn.sig, &prefix, true))
                    }
                    _ => {}
                }
            }

            eprintln!("{}", expand);
            quote! {
                #expand
                #item_trait
            }
            .into()
        }
        _ => {
            return syn::Error::new_spanned(
                input,
                "#[skerry] only supports functions and impl/trait blocks",
            )
            .into_compile_error()
            .into();
        }
    }
}

fn extract_e_macro_tokens(output: &ReturnType) -> syn::Result<proc_macro2::TokenStream> {
    let ty = match output {
        ReturnType::Type(_, ty) => ty.as_ref(),
        ReturnType::Default => {
            return Err(syn::Error::new(
                output.span(),
                "Functions must return Result<T, e![...]> to use Skerry.",
            ));
        }
    };

    let tp = match ty {
        Type::Path(tp) => tp,
        _ => {
            return Err(syn::Error::new_spanned(
                ty,
                "Skerry expected Result<T, e![...]>.",
            ));
        }
    };

    let last_seg = tp
        .path
        .segments
        .last()
        .ok_or_else(|| syn::Error::new_spanned(tp, "Skerry expected Result<T, e![...]>."))?;

    if last_seg.ident != "Result" {
        return Err(syn::Error::new_spanned(
            &last_seg.ident,
            "Skerry expected Result<T, e![...]>.",
        ));
    }

    let error_ty = if let PathArguments::AngleBracketed(args) = &last_seg.arguments {
        args.args
            .iter()
            .nth(1)
            .ok_or_else(|| syn::Error::new_spanned(args, "Result must have an error type."))?
    } else {
        return Err(syn::Error::new_spanned(
            last_seg,
            "Result must use angle brackets.",
        ));
    };

    if let GenericArgument::Type(Type::Macro(m)) = error_ty {
        if m.mac.path.is_ident("e") {
            Ok(m.mac.tokens.clone())
        } else {
            Err(syn::Error::new_spanned(
                &m.mac.path,
                "Expected the 'e' macro for error definitions (e![...]).",
            ))
        }
    } else {
        Err(syn::Error::new_spanned(
            error_ty,
            "The error type must be the 'e' macro: e![ErrA, ErrB].",
        ))
    }
}

#[proc_macro_attribute]
pub fn skerry_global(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let mut input = parse_macro_input!(item as ItemEnum);
    let enum_name = &input.ident;
    match &input.vis {
        Visibility::Public(_) => {}
        _ => {
            return syn::Error::new_spanned(&input.vis, "skerry_global enums must be 'pub'")
                .to_compile_error()
                .into();
        }
    }
    let mut macro_arms = quote! {};
    let mut private_structs = quote! {};
    let mut from_impls = quote! {};

    for variant in &mut input.variants {
        let var_id = &variant.ident;

        let struct_name = format_ident!("__SkerryPrivate{}", var_id);
        macro_arms.extend(quote! {
            (#var_id) => { crate::errors::__skerry_private::#struct_name };
        });
        private_structs.extend(quote! {
            pub struct #struct_name;
        });

        if variant.attrs.iter().any(|a| a.path().is_ident("from")) {
            variant.attrs.retain(|a| !a.path().is_ident("from"));
            let syn::Fields::Unnamed(ref fields) = variant.fields else {
                return syn::Error::new_spanned(
                    variant,
                    "#[from] can only be applied to tuples with one element",
                )
                .into_compile_error()
                .into();
            };

            if fields.unnamed.len() > 1 {
                return syn::Error::new_spanned(
                    variant,
                    "#[from] can only be applied to tuples with one element",
                )
                .into_compile_error()
                .into();
            }

            if let Some(syn::Type::Path(path)) = fields.unnamed.get(0).map(|f| &f.ty) {
                from_impls.extend(quote! {
                    impl From<#path> for #enum_name {
                        fn from(value: #path) -> Self {
                            #enum_name::#var_id(value)
                        }
                    }
                    impl<T> skerry::skerry_internals::IsSubsetOf<T> for #path
                    where
                        T: skerry::skerry_internals::Contains<__skerry_error_tag!(#var_id)>
                    {}
                });
            }
        }
    }
    let munch_arms = input.variants.iter().map(|variant| {
            let var_id = &variant.ident;

            match &variant.fields {
                syn::Fields::Unit => quote! {
                    (@munch $val:expr, $f:ident, $t:ident, [ #var_id $(, $rest:ident)* ], { $($acc:tt)* }) => {
                        __skerry_global_error_layout_convert!(@munch $val, $f, $t, [ $($rest),* ], { $($acc)* $f::#var_id => $t::#var_id, })
                    };
                },
                syn::Fields::Unnamed(fields) => {
                    let bindings: Vec<_> = fields.unnamed.iter().enumerate()
                        .map(|(i, _)| format_ident!("v{}", i))
                        .collect();

                    quote! {
                        (@munch $val:expr, $f:ident, $t:ident, [ #var_id $(, $rest:ident)* ], { $($acc:tt)* }) => {
                            __skerry_global_error_layout_convert!(@munch $val, $f, $t, [ $($rest),* ], { $($acc)* $f::#var_id(#(#bindings),*) => $t::#var_id(#(#bindings),*), })
                        };
                    }
                },
                syn::Fields::Named(fields) => {
                    let names: Vec<_> = fields.named.iter().map(|f| &f.ident).collect();
                    quote! {
                        (@munch $val:expr, $f:ident, $t:ident, [ #var_id $(, $rest:ident)* ], { $($acc:tt)* }) => {
                            __skerry_global_error_layout_convert!(@munch $val, $f, $t, [ $($rest),* ], { $($acc)* $f::#var_id { #(#names),* } => $t::#var_id { #(#names),* }, })
                        };
                    }
                }
            }
        });

    let layout_arms = input.variants.iter().map(|variant| {
            let var_id = &variant.ident;
            let fields = &variant.fields;

            quote! {
                (@munch $vis:vis, $t:ident, [ #var_id $(, $rest:ident)* ], { $($acc:tt)* }) => {
                    __skerry_global_error_layout!(@munch $vis, $t, [ $($rest),* ], { $($acc)* #var_id #fields, });
                };
            }
        });

    let enum_full_path: Path = parse_quote!(crate::errors::#enum_name);

    let output = quote! {
        #input

        #[macro_export]
        macro_rules! __skerry_error_tag {
            #macro_arms
            ($ty:tt) => {compile_error!(concat!(stringify!($ty), " does not exist."))};
        }

        #[macro_export]
        macro_rules! __skerry_global_error_layout_convert {
            (START $val:expr, $from_ty:ident, $to_ty:ident, [ $($variants:ident),* ]) => {
                __skerry_global_error_layout_convert!(@munch $val, $from_ty, $to_ty, [ $($variants),* ], { })
            };

            #(#munch_arms)*

            (@munch $val:expr, $f:ident, $t:ident, [ ], { $($acc:tt)* }) => {
                #[allow(unreachable_patterns)]
                match $val {
                    $($acc)*
                    _ => unreachable!("Unexpected variant"),
                }
            };
        }

        #[macro_export]
        macro_rules! __skerry_global_error_layout {
            (START $vis:vis, $ty:ident, [ $($variants:ident),* ]) => {
                __skerry_global_error_layout!(@munch $vis, $ty, [ $($variants),* ], { });
            };

            #(#layout_arms)*

            (@munch $vis:vis, $ty:ident, [ ], { $($acc:tt)* }) => {
                $vis enum $ty {
                    $($acc)*
                }
            };
        }

        #[macro_export]
        macro_rules! __skerry_global_error_ident {
            () => {
                #enum_full_path
            };
        }

        #[macro_export]
        macro_rules! __skerry_global_error_ident_import {
            ($ident:ident) => {
                use #enum_full_path as $ident;
            };
        }

        #[macro_export]
        macro_rules! e {
            ($($tokens:tt)*) => {
            };
        }

        pub mod __skerry_private {
            #private_structs
        }

        #from_impls
    };

    eprintln!("{}", output);

    TokenStream::from(output)
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
    match crate::internal::skerry_fn::skerry_fn_standard(attr, item, None) {
        Ok((def, input)) => quote! {
            #def

            #input
        }
        .into(),
        Err(e) => e,
    }
}

/// An attribute macro applied to **implementation blocks** (`impl`).
///
/// This macro enables the use of `#[skerry_fn]` inside the block and
/// extracts those function definitions to the scope outside the block.
///
/// # Arguments
/// * `prefix(Name)` - Optional. Sets the prefix for the generated external definitions.
///
/// # Example
/// ```ignore
/// #[skerry_impl(prefix(MyStruct))]
/// impl MyStruct {
///     #[skerry_fn]
///     pub fn struct_fn() -> Result<(), &MyError> {
///         // ...
/// #       Ok(())
///     }
/// }
/// ```
#[proc_macro_attribute]
pub fn skerry_impl(attr: TokenStream, item: TokenStream) -> TokenStream {
    crate::internal::skerry_impl::skerry_impl(attr, item)
}

/// An attribute macro applied to **trait definitions**.
///
/// This macro enables the use of `#[skerry_fn]` within a trait and
/// pulls those definitions outside the trait block for Skerry processing.
///
/// # Arguments
/// * `prefix(Name)` - Optional. Sets the prefix for the generated external definitions.
///
/// # Example
/// ```ignore
/// #[skerry_trait(prefix(MyTrait))]
/// trait MyTrait {
///     #[skerry_fn]
///     fn test() -> Result<(), (ErrA, ErrB)> {
///         // ...
/// #       Ok(())
///     }
/// }
/// ```
#[proc_macro_attribute]
pub fn skerry_trait(attr: TokenStream, item: TokenStream) -> TokenStream {
    crate::internal::skerry_trait::skerry_trait(attr, item)
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
    ty: Ident,
    errors: Vec<Path>,
}

impl Parse for Input {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let ty: Ident;

        ty = input.parse()?;
        input.parse::<Token![,]>()?;

        let mut errors = Vec::new();
        while !input.is_empty() {
            if input.peek(Token![,]) {
                input.parse::<Token![,]>()?;
            } else if input.is_empty() {
                break;
            }
            errors.push(input.parse()?);
        }

        Ok(Input { ty, errors })
    }
}

struct DefineErrorInput {
    type_ident: Ident,
    _comma: Token![,],
    bracket: syn::token::Bracket,
    inner_tokens: proc_macro2::TokenStream,
}

impl syn::parse::Parse for DefineErrorInput {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let content;
        Ok(DefineErrorInput {
            type_ident: input.parse()?,
            _comma: input.parse()?,
            bracket: syn::bracketed!(content in input),
            inner_tokens: content.parse()?,
        })
    }
}

#[proc_macro]
pub fn define_error(input: TokenStream) -> TokenStream {
    let DefineErrorInput {
        type_ident,
        inner_tokens,
        bracket,
        ..
    } = parse_macro_input!(input as DefineErrorInput);
    let mut iter = inner_tokens.into_iter().peekable();
    let errors = match process_inner_errors(&mut iter, bracket.span.join()) {
        Ok(v) => v,
        Err(e) => return e.into_compile_error().into(),
    };

    quote_error_gen(type_ident, errors).into()
}
