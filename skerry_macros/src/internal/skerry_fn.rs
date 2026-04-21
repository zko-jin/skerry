use proc_macro::TokenStream;
use proc_macro2::{Delimiter, Span, TokenStream as TokenStream2, TokenTree};
use quote::{ToTokens, format_ident, quote, quote_spanned};
#[cfg(not(feature = "custom_result"))]
use syn::{
    Expr, ExprTry,
    visit_mut::{self, VisitMut},
};
use syn::{
    GenericArgument, Ident, ItemFn, PathArguments, ReturnType, TraitItemFn, Type, parse_quote,
};

#[cfg(not(feature = "custom_result"))]
pub struct QuestionMarkTransformer;

#[cfg(not(feature = "custom_result"))]
impl VisitMut for QuestionMarkTransformer {
    fn visit_expr_mut(&mut self, node: &mut Expr) {
        visit_mut::visit_expr_mut(self, node);

        if let Expr::Try(ExprTry { expr, .. }) = node {
            let inner = expr.as_ref();
            *node = parse_quote! {
                #inner.map_err(|e| Into::<GlobalErrors<_>>::into(e))?
            };
        }
    }
}

/// Internal helper to unify ItemFn and TraitItemFn
struct SkerryFnInput {
    sig: syn::Signature,
    block: Option<syn::Block>,
    attrs: Vec<syn::Attribute>,
    vis: syn::Visibility,
}

impl SkerryFnInput {
    fn from_fn(mut item: ItemFn) -> Self {
        #[cfg(not(feature = "custom_result"))]
        {
            let mut transformer = QuestionMarkTransformer;
            transformer.visit_item_fn_mut(&mut item);
        }
        Self {
            sig: item.sig,
            block: Some(*item.block),
            attrs: item.attrs,
            vis: item.vis,
        }
    }

    fn from_trait_fn(mut item: TraitItemFn) -> Self {
        #[cfg(not(feature = "custom_result"))]
        {
            let mut transformer = QuestionMarkTransformer;
            transformer.visit_trait_item_fn_mut(&mut item);
        }
        Self {
            sig: item.sig,
            block: item.default,
            attrs: item.attrs,
            vis: syn::Visibility::Inherited,
        }
    }
}

pub fn skerry_fn_standard(
    _attr: TokenStream,
    item: TokenStream,
    prefix: Option<&syn::Ident>,
) -> Result<(TokenStream2, TokenStream2), TokenStream> {
    let input = syn::parse::<ItemFn>(item).map_err(|e| TokenStream::from(e.to_compile_error()))?;
    process_skerry_logic(SkerryFnInput::from_fn(input), prefix)
}

pub fn skerry_fn_trait(
    _attr: TokenStream,
    item: TokenStream,
    prefix: Option<&syn::Ident>,
) -> Result<(TokenStream2, TokenStream2), TokenStream> {
    let input =
        syn::parse::<TraitItemFn>(item).map_err(|e| TokenStream::from(e.to_compile_error()))?;
    process_skerry_logic(SkerryFnInput::from_trait_fn(input), prefix)
}

fn process_skerry_logic(
    mut input: SkerryFnInput,
    prefix: Option<&syn::Ident>,
) -> Result<(TokenStream2, TokenStream2), TokenStream> {
    // Check if the function returns a Result
    let error_type_tokens = if let ReturnType::Type(_, ty) = &input.sig.output {
        if let Type::Path(tp) = ty.as_ref() {
            let last_seg = tp
                .path
                .segments
                .last()
                .ok_or_else(|| {
                    quote! { compile_error!("Expected a return type."); }.into_token_stream()
                })
                .map_err(|e| TokenStream::from(e))?;

            // Ensure the type is "Result"
            if last_seg.ident != "Result" {
                return Err(quote_spanned! { last_seg.ident.span() =>
                    compile_error!("Skerry functions must return a Result.");
                }
                .into());
            }

            // Extract the second generic argument (the Error part)
            if let PathArguments::AngleBracketed(args) = &last_seg.arguments {
                if let Some(GenericArgument::Type(error_ty)) = args.args.iter().nth(1) {
                    error_ty.to_token_stream()
                } else {
                    return Err(
                        quote! { compile_error!("Result must have an error type."); }.into(),
                    );
                }
            } else {
                return Err(quote! { compile_error!("Result must use angle brackets."); }.into());
            }
        } else {
            return Err(quote! { compile_error!("Unsupported return type format."); }.into());
        }
    } else {
        return Err(
            quote! { compile_error!("Functions must return Result to use Skerry."); }.into(),
        );
    };

    let expanded = match extract_errors_from_tokens(error_type_tokens) {
        Ok(Some(errors)) => {
            // Name Formatting
            let formatted_prefix = match prefix {
                Some(i) => format_camel_case(&i.to_string()),
                None => String::new(),
            };
            let struct_ident = format_ident!(
                "{}{}Error",
                formatted_prefix,
                format_camel_case(&input.sig.ident.to_string())
            );

            // Update Signature with the new Error Struct
            if let ReturnType::Type(_, ty) = &mut input.sig.output {
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

            // Reconstruct Output
            quote_error_gen(struct_ident, errors)
        }
        Ok(None) => quote! {},
        Err(e) => return Err(TokenStream::from(e.to_compile_error())),
    };

    let attrs = input.attrs;
    let vis = input.vis;
    let sig = input.sig;
    let body = input
        .block
        .map(|b| quote! { #b })
        .unwrap_or_else(|| quote! { ; });

    let final_item = quote! {
        #(#attrs)* #vis #sig #body
    };

    Ok((expanded, final_item))
}

pub fn quote_error_gen(
    type_ident: Ident,
    (normal_errors, passthrough_errors): (Vec<Ident>, Vec<Ident>),
) -> TokenStream2 {
    quote! { skerry::skerry_internals::create_fn_error!(
            #type_ident,
            [#(#normal_errors),*],
            [#(#passthrough_errors),*]
        );
    }
}

// Formats to snake_case
pub fn format_snake_case(prefix: &str) -> String {
    let mut formatted = String::new();
    for (i, c) in prefix.chars().enumerate() {
        if c.is_uppercase() && i != 0 {
            formatted.push('_');
        }
        formatted.extend(c.to_lowercase());
    }
    formatted
}

// Formats to CamelCase
fn format_camel_case(base: &str) -> String {
    base.split('_')
        .map(|s| {
            let mut c = s.chars();
            match c.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            }
        })
        .collect::<String>()
}

fn extract_errors_from_tokens(
    tokens: TokenStream2,
) -> syn::Result<Option<(Vec<Ident>, Vec<Ident>)>> {
    let mut iter = tokens.into_iter().peekable();

    // Look for 'e'
    match iter.peek() {
        Some(TokenTree::Ident(ident)) if ident == "e" => {
            iter.next();
            // Look for '!'
            match iter.next() {
                Some(TokenTree::Punct(p)) if p.as_char() == '!' => {
                    if let Some(TokenTree::Group(g)) = iter.next() {
                        if g.delimiter() != Delimiter::Bracket {
                            return Err(syn::Error::new(g.span(), "e! only accepts brackets []."));
                        }
                        let mut inner_stream = g.stream().into_iter().peekable();
                        process_inner_errors(&mut inner_stream, g.span()).map(|e| Some(e))
                    } else {
                        Err(syn::Error::new(p.span(), "e! only accepts brackets []."))
                    }
                }
                _ => Ok(None),
            }
        }
        Some(_) => Ok(None),
        None => Err(syn::Error::new(Span::call_site(), "Unexpected EOF")),
    }
}

pub fn process_inner_errors(
    iter: &mut std::iter::Peekable<proc_macro2::token_stream::IntoIter>,
    error_span: proc_macro2::Span,
) -> syn::Result<(Vec<Ident>, Vec<Ident>)> {
    if iter.peek().is_none() {
        return Err(syn::Error::new(
            error_span,
            "Cannot be empty; expected at least one error type",
        ));
    }

    let mut normal = Vec::new();
    let mut passthrough = Vec::new();
    while let Some(token) = iter.next() {
        match token {
            // Handle Passthrough '*'
            TokenTree::Punct(p) if p.as_char() == '*' => {
                if let Some(TokenTree::Ident(i)) = iter.next() {
                    passthrough.push(i);
                } else {
                    return Err(syn::Error::new(p.span(), "Expected error type after '*'"));
                }
            }

            // Unexpected comma (e.g., double commas ,,)
            TokenTree::Punct(p) if p.as_char() == ',' => {
                return Err(syn::Error::new(
                    p.span(),
                    "Unexpected extra comma in error list",
                ));
            }

            // Handle Normal Error Types
            TokenTree::Ident(i) => {
                normal.push(i);
            }

            t => {
                return Err(syn::Error::new(t.span(), "Expected error type"));
            }
        }

        // Consume ','
        if let Some(next) = iter.peek() {
            match next {
                TokenTree::Punct(p) if p.as_char() == ',' => {
                    iter.next();
                }
                _ => {
                    return Err(syn::Error::new(
                        next.span(),
                        "Expected ',' between error types",
                    ));
                }
            }
        }
    }

    Ok((normal, passthrough))
}
