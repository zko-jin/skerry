use proc_macro::TokenStream;
use proc_macro2::{TokenStream as TokenStream2, TokenTree};
use quote::{ToTokens, format_ident, quote};
use syn::visit_mut::{self, VisitMut};
use syn::{Expr, ExprTry};
use syn::{
    GenericArgument, ItemFn, PathArguments, ReturnType, Type, parse_macro_input, parse_quote,
};

struct QuestionMarkTransformer;

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

pub fn skerry_fn(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let mut input_fn = parse_macro_input!(item as ItemFn);

    let mut transformer = QuestionMarkTransformer;
    transformer.visit_item_fn_mut(&mut input_fn);

    let fn_name = &input_fn.sig.ident;

    // 2. Derive Names
    let struct_name_str = fn_name
        .to_string()
        .split('_')
        .map(|s| {
            let mut c = s.chars();
            match c.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            }
        })
        .collect::<String>()
        + "Error";
    let struct_ident = format_ident!("{}", struct_name_str);

    let mut normal_errors = Vec::new();
    let mut passthrough_errors = Vec::new();

    // 3. Token Munching Logic
    if let ReturnType::Type(_, ty) = &input_fn.sig.output {
        if let Type::Path(tp) = ty.as_ref() {
            if let Some(seg) = tp.path.segments.last() {
                if let PathArguments::AngleBracketed(args) = &seg.arguments {
                    if let Some(GenericArgument::Type(error_part)) = args.args.iter().nth(1) {
                        let tokens = error_part.to_token_stream();
                        let mut iter = tokens.into_iter().peekable();

                        if let Some(TokenTree::Group(g)) = iter.next() {
                            if g.delimiter() == proc_macro2::Delimiter::Parenthesis {
                                let mut g_iter = g.stream().into_iter().peekable();
                                while let Some(token) = g_iter.next() {
                                    if let TokenTree::Punct(ref p) = token {
                                        if p.as_char() == '&' {
                                            let mut path_tokens = TokenStream2::new();
                                            while let Some(next) = g_iter.peek() {
                                                if let TokenTree::Punct(p) = next {
                                                    if p.as_char() == ',' {
                                                        break;
                                                    }
                                                }
                                                path_tokens.extend(std::iter::once(
                                                    g_iter.next().unwrap(),
                                                ));
                                            }
                                            passthrough_errors.push(path_tokens);
                                            continue;
                                        }
                                        if p.as_char() == ',' {
                                            continue;
                                        }
                                    }
                                    let mut path_tokens = token.to_token_stream();
                                    while let Some(next) = g_iter.peek() {
                                        if let TokenTree::Punct(p) = next {
                                            if p.as_char() == ',' {
                                                break;
                                            }
                                        }
                                        path_tokens.extend(std::iter::once(g_iter.next().unwrap()));
                                    }
                                    normal_errors.push(path_tokens);
                                }
                            }
                        } else {
                            // Single error case
                            let mut s_iter = error_part.to_token_stream().into_iter().peekable();
                            if let Some(TokenTree::Punct(p)) = s_iter.peek() {
                                if p.as_char() == '&' {
                                    s_iter.next(); // consume &
                                    passthrough_errors.push(s_iter.collect());
                                } else {
                                    normal_errors.push(error_part.to_token_stream());
                                }
                            } else {
                                normal_errors.push(error_part.to_token_stream());
                            }
                        }
                    }
                }
            }
        }
    }

    // 4. Update Function Signature
    if let ReturnType::Type(_, ty) = &mut input_fn.sig.output {
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

    let expanded = quote! {
        skerry::skerry_internals::create_fn_error!(
            #struct_ident,
            #fn_name,
            [#(#normal_errors),*],
            [#(#passthrough_errors),*]
        );

        #input_fn
    };

    TokenStream::from(expanded)
}
