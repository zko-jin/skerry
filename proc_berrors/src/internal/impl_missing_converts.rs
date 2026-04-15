use std::collections::HashSet;

use proc_macro::TokenStream;
use quote::quote;
use syn::{
    Ident, Path, Token, bracketed,
    parse::{Parse, ParseStream},
    parse_macro_input,
};

struct MacroInput {
    pub ty: Ident,
    pub source: Vec<Path>,
    pub to_remove: Vec<Path>,
}

impl Parse for MacroInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let ty: Ident;
        ty = input.parse()?;
        input.parse::<Token![,]>()?;

        let content;
        bracketed!(content in input);
        let source = content
            .parse_terminated(Path::parse, Token![,])?
            .into_iter()
            .collect();

        let _ = input.parse::<Token![,]>();

        let content2;
        bracketed!(content2 in input);
        let to_remove = content2
            .parse_terminated(Path::parse, Token![,])?
            .into_iter()
            .collect();

        Ok(MacroInput {
            ty,
            source,
            to_remove,
        })
    }
}

pub fn impl_missing_converts(input: TokenStream) -> TokenStream {
    let MacroInput {
        ty,
        source,
        to_remove,
    } = parse_macro_input!(input as MacroInput);

    let blacklist: HashSet<String> = to_remove
        .iter()
        .map(|p| quote!(#p).to_string().replace(" ", ""))
        .collect();

    let filtered: Vec<Path> = source
        .into_iter()
        .filter(|p| {
            let path_str = quote!(#p).to_string().replace(" ", "");
            !blacklist.contains(&path_str)
        })
        .collect();

    let expanded = quote! {
        #(
            impl const berrors::berrors_internals::MissingConvert<#filtered> for #ty {}
        )*

        impl<T, E: berrors::berrors_internals::ComparableError #( + berrors::berrors_internals::MissingConvert<#filtered>)*>
            std::ops::FromResidual<GlobalErrors<E>> for Result<T, #ty>
        {
            fn from_residual(residual: GlobalErrors<E>) -> Result<T, #ty> {
                match residual {
                    #(
                        GlobalErrors::#to_remove(v) => {
                            Result::Err(#ty::#to_remove(v))
                        }
                    )*
                    _ => unreachable!(),
                }
            }
        }
    };

    TokenStream::from(expanded)
}
