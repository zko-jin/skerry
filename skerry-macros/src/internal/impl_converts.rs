use proc_macro::TokenStream;
use quote::quote;
use syn::{
    Ident,
    Path,
    Token,
    bracketed,
    parse::{
        Parse,
        ParseStream,
    },
    parse_macro_input,
};

struct MacroInput {
    pub ty: Ident,
    pub source: Vec<Path>,
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

        Ok(MacroInput { ty, source })
    }
}

pub fn impl_converts(input: TokenStream) -> TokenStream {
    let MacroInput { ty, source } = parse_macro_input!(input as MacroInput);

    let expanded = quote! {

        impl<E: skerry::skerry_internals::SkerryError #( + skerry::skerry_internals::Contains<#source>)*>
            From<GlobalErrors<E>> for #ty
        {
            fn from(val: GlobalErrors<E>) -> #ty {
                match val {
                    #(
                        GlobalErrors::#source(v) => {
                            #ty::#source(v)
                        }
                    )*
                    _ => unreachable!(),
                }
            }
        }

        #(
            impl skerry::skerry_internals::Contains<#source> for #ty {}
        )*
    };

    TokenStream::from(expanded)
}
