use proc_macro::TokenStream;
use quote::quote;
use syn::{
    self,
    Item,
    ItemEnum,
    Type,
    Visibility,
    parse_macro_input,
};

use crate::code_gen::replace_fn_error;

mod internal {}

mod code_gen;

#[proc_macro]
pub fn skerry_invoke(input: TokenStream) -> TokenStream {
    use std::{
        env,
        fs,
        path::PathBuf,
    };

    use proc_macro2::Span;
    use skerry_codegen::WrittenResult;

    let span = Span::call_site();

    let out_dir = PathBuf::from(format!(
        "{}/skerry/expansions/{}",
        env::var("OUT_DIR").unwrap(),
        input.to_string()
    ));
    let Ok(bytes) = fs::read(out_dir) else {
        return syn::Error::new(span, "Couldn't read expansion result")
            .to_compile_error()
            .into();
    };

    let Ok(result) = postcard::from_bytes(&bytes) else {
        return syn::Error::new(span, "Expansion result invalid")
            .to_compile_error()
            .into();
    };

    match result {
        WrittenResult::Ok(s) => s.parse().unwrap(),
        WrittenResult::EnumError { msg, .. } => {
            syn::Error::new(span, msg).to_compile_error().into()
        }
        WrittenResult::RawError { msg } => syn::Error::new(span, msg).to_compile_error().into(),
    }
}

#[proc_macro_attribute]
pub fn skerry(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as Item);

    match input {
        Item::Fn(mut item_fn) => {
            match replace_fn_error("", &mut item_fn.sig) {
                Ok(_) => {}
                Err(e) => return e.into_compile_error().into(),
            };
            quote! {
                #item_fn
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
            let prefix = match &*item_impl.self_ty {
                Type::Path(path) => path
                    .path
                    .get_ident()
                    .map_or("".to_string(), |i| i.to_string()),
                _ => "".into(),
            };
            for impl_item in &mut item_impl.items {
                match impl_item {
                    syn::ImplItem::Fn(func) => match replace_fn_error(&prefix, &mut func.sig) {
                        Ok(_) => {}
                        Err(e) => return e.into_compile_error().into(),
                    },
                    _ => {}
                }
            }

            quote! {
                #item_impl
            }
            .into()
        }
        Item::Trait(mut item_trait) => {
            let prefix = item_trait.ident.to_string();
            for trait_item in &mut item_trait.items {
                match trait_item {
                    syn::TraitItem::Fn(func) => match replace_fn_error(&prefix, &mut func.sig) {
                        Ok(_) => {}
                        Err(e) => return e.into_compile_error().into(),
                    },
                    _ => {}
                }
            }

            quote! {
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

#[proc_macro_attribute]
pub fn skerry_global(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let mut input = parse_macro_input!(item as ItemEnum);
    match &input.vis {
        Visibility::Public(_) => {}
        _ => {
            return syn::Error::new_spanned(&input.vis, "skerry_global enums must be 'pub'")
                .to_compile_error()
                .into();
        }
    }

    for variant in &mut input.variants {
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
        }
    }

    let output = quote! {
        include!(concat!(env!("OUT_DIR"), "/skerry/skerry_gen.rs"));
        #input
        skerry::skerry_internals::skerry_invoke!(global);
    };

    TokenStream::from(output)
}
// struct DefineErrorInput {
//     type_ident: Ident,
//     _comma: Token![,],
//     bracket: syn::token::Bracket,
//     inner_tokens: proc_macro2::TokenStream,
// }

// impl syn::parse::Parse for DefineErrorInput {
//     fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
//         let content;
//         Ok(DefineErrorInput {
//             type_ident: input.parse()?,
//             _comma: input.parse()?,
//             bracket: syn::bracketed!(content in input),
//             inner_tokens: content.parse()?,
//         })
//     }
// }

// #[proc_macro]
// pub fn define_error(input: TokenStream) -> TokenStream {
//     let DefineErrorInput {
//         type_ident,
//         inner_tokens,
//         bracket,
//         ..
//     } = parse_macro_input!(input as DefineErrorInput);
//     // let mut iter = inner_tokens.into_iter().peekable();
//     // let errors = match process_inner_errors(&mut iter, bracket.span.join()) {
//     //     Ok(v) => v,
//     //     Err(e) => return e.into_compile_error().into(),
//     // };

//     // quote_error_gen(type_ident, errors).into()
// }
