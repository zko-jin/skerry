use ahash::RandomState;
use proc_macro::TokenStream;
use quote::{
    ToTokens,
    quote,
};
use syn::{
    GenericArgument,
    Ident,
    ItemFn,
    PathArguments,
    ReturnType,
    Token,
    Type,
    parse::{
        Parse,
        ParseStream,
    },
    parse_macro_input,
};

pub fn calculate_ident_hash(ident: &syn::Ident) -> u64 {
    let hasher = RandomState::with_seeds(0, 0, 0, 0);
    hasher.hash_one(ident.to_string())
}

pub fn calculate_sig_hash(sig: &syn::Signature) -> u64 {
    let sig_string = sig.to_token_stream().to_string();
    let normalized: String = sig_string.chars().filter(|c| !c.is_whitespace()).collect();

    let hasher = RandomState::with_seeds(0, 0, 0, 0);
    hasher.hash_one(normalized)
}

#[allow(unused)]
enum ErrorInput {
    Standard(Ident),
    Spread(Ident),
}

#[allow(unused)]
struct EInput {
    errors: Vec<ErrorInput>,
}

impl Parse for EInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut errors = Vec::new();

        if input.is_empty() {
            return Err(syn::Error::new(
                input.span(),
                "Attribute requires you to define at least one error",
            ));
        }

        while !input.is_empty() {
            if input.peek(Token![*]) {
                input.parse::<Token![*]>()?;
                errors.push(ErrorInput::Spread(input.parse()?));
            } else {
                errors.push(ErrorInput::Standard(input.parse()?));
            }
            if input.peek(Token![,]) {
                input.parse::<Token![,]>()?;
            }
        }
        Ok(EInput { errors })
    }
}

pub fn e(attr: TokenStream, item: TokenStream) -> TokenStream {
    let _input = parse_macro_input!(attr as EInput);

    let mut input_fn = parse_macro_input!(item as ItemFn);
    let sig = &input_fn.sig;

    let sig_hash = proc_macro2::Literal::u64_unsuffixed(calculate_sig_hash(sig));

    let return_type = match &sig.output {
        ReturnType::Type(_, ty) => match extract_result_type(ty) {
            Some(inner) => inner,
            None => {
                return syn::Error::new_spanned(ty, "Function must return Result<T>")
                    .to_compile_error()
                    .into();
            }
        },
        ReturnType::Default => {
            return syn::Error::new_spanned(&sig.fn_token, "Function must return Result<T>")
                .to_compile_error()
                .into();
        }
    };

    input_fn.sig.output = syn::parse2(quote! {
        -> Result<#return_type, skerry::skerry_invoke!(#sig_hash)>
    })
    .expect("Failed to rewrite return type");

    TokenStream::from(quote! {
        #input_fn
    })
}

fn extract_result_type(ty: &Type) -> Option<&Type> {
    let path = match ty {
        Type::Path(tp) => &tp.path,
        _ => return None,
    };
    let last_segment = path.segments.last()?;
    if last_segment.ident != "Result" {
        return None;
    }

    if let PathArguments::AngleBracketed(args) = &last_segment.arguments {
        if let Some(GenericArgument::Type(inner)) = args.args.first() {
            return Some(inner);
        }
    }
    None
}
