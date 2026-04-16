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
    pub expand: bool,
}

impl Parse for MacroInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let ty: Ident = input.parse()?;
        input.parse::<Token![,]>()?;

        let content;
        bracketed!(content in input);
        let source = content
            .parse_terminated(Path::parse, Token![,])?
            .into_iter()
            .collect();

        input.parse::<Token![,]>()?;

        let content2;
        bracketed!(content2 in input);
        let to_remove = content2
            .parse_terminated(Path::parse, Token![,])?
            .into_iter()
            .collect();

        let mut expand = true;

        if input.peek(Token![,]) {
            input.parse::<Token![,]>()?;

            if input.peek(Ident) {
                let lookahead = input.lookahead1();
                let ident: Ident = input.parse()?;
                if ident == "no_expand" {
                    expand = false;
                } else {
                    return Err(lookahead.error());
                }
            }
        }

        Ok(MacroInput {
            ty,
            source,
            to_remove,
            expand,
        })
    }
}

pub fn impl_missing_converts(input: TokenStream) -> TokenStream {
    let MacroInput {
        ty,
        source,
        to_remove,
        expand,
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

    let match_from = if expand {
        quote! {
            #(
                GlobalErrors::#to_remove(v) => {
                    #ty::#to_remove(v)
                }
            )*
        }
    } else {
        quote! {
            #(
                GlobalErrors::#to_remove(v) => {
                    v
                }
            )*
        }
    };

    let from_impl = if expand {
        quote! {
            impl<E: skerry::skerry_internals::SkerryError #( + skerry::skerry_internals::MissingConvert<#filtered>)*>
                From<GlobalErrors<E>> for #ty
            {
                fn from(val: GlobalErrors<E>) -> #ty {
                    match val {
                        #match_from
                        _ => unreachable!(),
                    }
                }
            }
        }
    } else {
        quote! {}
    };

    let expanded = quote! {
        #(
            impl skerry::skerry_internals::MissingConvert<#filtered> for #ty {}
        )*

        #from_impl
    };

    TokenStream::from(expanded)
}
