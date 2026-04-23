use proc_macro::TokenStream;
use quote::quote;
use syn::{
    Ident,
    Token,
    parse::{
        Parse,
        ParseStream,
    },
    parse_macro_input,
};

#[allow(dead_code)]
enum ErrorInput {
    Standard(Ident),
    Spread(Ident),
}

struct EInput {
    _errors: Vec<ErrorInput>,
}

impl Parse for EInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut _errors = Vec::new();

        if input.is_empty() {
            return Err(syn::Error::new(
                input.span(),
                "e![] requires you to define at least one error",
            ));
        }

        while !input.is_empty() {
            if input.peek(Token![*]) {
                input.parse::<Token![*]>()?;
                _errors.push(ErrorInput::Spread(input.parse()?));
            } else {
                _errors.push(ErrorInput::Standard(input.parse()?));
            }
            if input.peek(Token![,]) {
                input.parse::<Token![,]>()?;
            }
        }
        Ok(EInput { _errors })
    }
}

pub fn e(input: TokenStream) -> TokenStream {
    let _input = parse_macro_input!(input as EInput);

    let span = proc_macro::Span::call_site();
    let line = span.start().line();
    let line_lit = proc_macro2::Literal::usize_unsuffixed(line);
    let file = span.file();
    let short_path = if let Some(idx) = file.find("src/") {
        &file[idx..]
    } else {
        &file
    };

    quote! {
        skerry_invoke!{#short_path, #line_lit}
    }
    .into()
}
