use std::collections::HashSet;

use proc_macro::TokenStream;
use quote::{
    format_ident,
    quote,
};
use syn::{
    Attribute,
    Ident,
    ImplItemFn,
    ItemFn,
    ItemImpl,
    ItemTrait,
    Path,
    ReturnType,
    Token,
    TraitItemFn,
    Type,
    parse::{
        Parse,
        ParseStream,
    },
    parse_macro_input,
    parse_quote,
    punctuated::Punctuated,
    visit_mut::{
        self,
        VisitMut,
    },
};

use crate::internal::skerry_fn::{
    format_snake_case,
    process_inner_errors,
    quote_error_gen,
};

mod internal {
    pub mod skerry_fn;
    pub mod skerry_impl;
    pub mod skerry_mod;
    pub mod skerry_trait;
}

#[cfg(feature = "codegen")]
#[proc_macro]
pub fn skerry_invoke(input: TokenStream) -> TokenStream {
    use std::{
        env,
        fs,
        path::PathBuf,
    };

    let hash = parse_macro_input!(input as syn::LitInt);
    let out_dir = PathBuf::from(format!(
        "{}/skerry/expansions/{}",
        env::var("OUT_DIR").unwrap(),
        hash
    ));
    eprintln!("{}", hash);
    let Ok(bytes) = fs::read(out_dir) else {
        return syn::Error::new_spanned(hash, "Couldn't read expansion result")
            .to_compile_error()
            .into();
    };

    let Some(marker_byte) = bytes.get(0).cloned() else {
        return syn::Error::new_spanned(hash, "Expansion result empty")
            .to_compile_error()
            .into();
    };

    let expansion = String::from_utf8_lossy(&bytes[1..]);
    match marker_byte {
        b'!' => syn::Error::new_spanned(hash, expansion)
            .to_compile_error()
            .into(),
        b'+' => expansion.parse().unwrap(),
        _ => syn::Error::new_spanned(hash, "Unknown marker. Codegen is corrupted")
            .to_compile_error()
            .into(),
    }
}

#[cfg(feature = "codegen")]
mod code_gen;

#[cfg(feature = "codegen")]
#[proc_macro_attribute]
pub fn e(_attr: TokenStream, item: TokenStream) -> TokenStream {
    code_gen::e(_attr, item)
}

#[cfg(feature = "codegen")]
#[proc_macro_attribute]
pub fn skerry_error(_attr: TokenStream, item: TokenStream) -> TokenStream {
    use syn::Item;

    use crate::code_gen::calculate_ident_hash;

    let item = parse_macro_input!(item as Item);
    let ident = match &item {
        Item::Struct(s) => &s.ident,
        Item::Enum(e) => &e.ident,
        _ => {
            return syn::Error::new_spanned(item, "skerry_error only supports structs and enums")
                .to_compile_error()
                .into();
        }
    };

    let hash = proc_macro2::Literal::u64_unsuffixed(calculate_ident_hash(ident));
    quote! {
        skerry::skerry_invoke!{ #hash }
        #item
    }
    .into()
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

#[proc_macro]
pub fn create_fn_error_step(input: TokenStream) -> TokenStream {
    let Input { ty, errors } = parse_macro_input!(input as Input);

    let priv_module = format_ident!("__skerry_private_{}", ty);

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

    let macro_ident = format_ident!("{}_errors", format_snake_case(&ty.to_string()));

    let not_trait = format_ident!("Not{}", ty);

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

        mod #priv_module {
            pub auto trait #not_trait {}
            impl !#not_trait for super::#ty {}
        }

        #(
            impl skerry::skerry_internals::Contains<#variants> for #ty {}
        )*
        impl<T: #(skerry::skerry_internals::Contains<#variants>)+*> skerry::skerry_internals::IsSupersetOf<T> for #ty {}

        impl<E: Into<GlobalErrors<E>> + skerry::skerry_internals::IsSubsetOf<#ty> + #priv_module::#not_trait> From<E> for #ty
        {
            fn from(val: E) -> #ty {
                match val.into() {
                    #(
                        GlobalErrors::#variants(v) => {
                            #ty::#variants(v)
                        }
                    )*
                    _ => unreachable!(),
                }
            }
        }

        impl From<#ty> for GlobalErrors<#ty> {
            fn from(val: #ty) -> GlobalErrors<#ty> {
                match val {
                    #(
                        #ty::#variants(v) => GlobalErrors::#variants(v),
                    )*
                }
            }
        }
    };

    TokenStream::from(expanded)
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

struct SkerryVisitor;

impl SkerryVisitor {
    fn is_skerry_result(rt: &ReturnType) -> bool {
        if let ReturnType::Type(_, ty) = rt {
            if let Type::Path(tp) = ty.as_ref() {
                let last_segment = tp.path.segments.last().unwrap();
                if last_segment.ident == "Result" {
                    if let syn::PathArguments::AngleBracketed(args) = &last_segment.arguments {
                        if let Some(syn::GenericArgument::Type(Type::Macro(m))) = args.args.last() {
                            return m.mac.path.is_ident("e");
                        }
                    }
                }
            }
        }
        false
    }

    fn add_attr(attrs: &mut Vec<Attribute>, name: &str) {
        let ident = syn::Ident::new(name, proc_macro2::Span::call_site());
        attrs.push(parse_quote!(#[#ident]));
    }
}

impl VisitMut for SkerryVisitor {
    fn visit_item_fn_mut(&mut self, i: &mut ItemFn) {
        if Self::is_skerry_result(&i.sig.output) {
            Self::add_attr(&mut i.attrs, "skerry_fn");
        }
        visit_mut::visit_item_fn_mut(self, i);
    }

    fn visit_item_impl_mut(&mut self, i: &mut ItemImpl) {
        Self::add_attr(&mut i.attrs, "skerry_impl");
        visit_mut::visit_item_impl_mut(self, i);
    }

    // Handle functions inside impl blocks
    fn visit_impl_item_fn_mut(&mut self, i: &mut ImplItemFn) {
        if Self::is_skerry_result(&i.sig.output) {
            Self::add_attr(&mut i.attrs, "skerry_fn");
        }
        visit_mut::visit_impl_item_fn_mut(self, i);
    }

    // Handle trait definitions
    fn visit_item_trait_mut(&mut self, i: &mut ItemTrait) {
        Self::add_attr(&mut i.attrs, "skerry_trait");
        visit_mut::visit_item_trait_mut(self, i);
    }

    // Handle functions inside traits
    fn visit_trait_item_fn_mut(&mut self, i: &mut TraitItemFn) {
        if Self::is_skerry_result(&i.sig.output) {
            Self::add_attr(&mut i.attrs, "skerry_fn");
        }
        visit_mut::visit_trait_item_fn_mut(self, i);
    }
}

#[proc_macro_attribute]
pub fn skerry(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let mut input = parse_macro_input!(item as syn::ItemMod);
    let mut visitor = SkerryVisitor;

    visitor.visit_item_mod_mut(&mut input);

    quote!(#input).into()
}
