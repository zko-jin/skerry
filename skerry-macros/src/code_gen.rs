use ahash::RandomState;
use proc_macro::TokenStream;
use quote::{
    ToTokens,
    quote,
};
use syn::{
    Expr,
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

pub fn calculate_sig_hash(prefix: &str, sig: &syn::Signature) -> u64 {
    let sig_string = sig.to_token_stream().to_string();
    let normalized: String = format!(
        "{}{}",
        prefix,
        sig_string
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect::<String>()
    );

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

pub fn replace_fn_error(prefix: &str, sig: &mut syn::Signature) -> syn::Result<()> {
    let sig_hash = proc_macro2::Literal::u64_unsuffixed(calculate_sig_hash(prefix, sig));

    match &mut sig.output {
        ReturnType::Type(_, ty) => match extract_result_error_type(ty) {
            Some(arg) => {
                // TODO: Check if it's e![...]
                *arg = GenericArgument::Type(Type::Macro(syn::parse_quote! {
                    skerry::skerry_internals::skerry_invoke!(#sig_hash)
                }));
            }
            None => {
                return Err(syn::Error::new_spanned(
                    ty,
                    "Function must return Result<T, e![...]>",
                ));
            }
        },
        ReturnType::Default => {
            return Err(syn::Error::new_spanned(
                sig.fn_token,
                "Function must return Result<T, e![...]>",
            ));
        }
    };

    Ok(())
}

fn extract_result_error_type(ty: &mut Type) -> Option<&mut GenericArgument> {
    let path = match ty {
        Type::Path(tp) => &mut tp.path,
        _ => return None,
    };
    let last_segment = path.segments.last_mut()?;
    if last_segment.ident != "Result" {
        return None;
    }

    if let PathArguments::AngleBracketed(args) = &mut last_segment.arguments {
        return args.args.get_mut(1);
    }
    None
}
