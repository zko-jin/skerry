use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{
    Expr, Ident, ImplItem, ImplItemFn, ItemImpl, Token, Type, parenthesized,
    parse::{Parse, ParseStream},
    parse_macro_input,
};

pub struct ImplTraitAttribute {
    pub prefix: Option<Ident>,
}
impl Parse for ImplTraitAttribute {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut prefix = None;

        while !input.is_empty() {
            let key: Ident = input.parse()?;

            match key.to_string().as_str() {
                "prefix" => {
                    let content;
                    parenthesized!(content in input);
                    prefix = Some(content.parse()?);
                }
                s => {
                    return Err(syn::Error::new(
                        key.span(),
                        format!(
                            "\"{}\" is not an attribute, valid attributes are \"prefix\".",
                            s
                        ),
                    ));
                }
            }

            if input.peek(Token![,]) {
                input.parse::<Token![,]>()?;
            }
        }

        Ok(ImplTraitAttribute { prefix })
    }
}

pub fn type_to_ident(ty: &Type) -> Ident {
    match ty {
        Type::Path(tp) => {
            let last_seg = tp
                .path
                .segments
                .last()
                .map(|s| s.ident.to_string())
                .unwrap_or_else(|| "UnknownType".to_string());
            format_ident!("{}", last_seg)
        }
        Type::Reference(tr) => {
            // Maybe add lifetime?
            let inner = type_to_ident(&tr.elem);
            format_ident!("{}Ref", inner)
        }
        Type::Slice(ts) => {
            let inner = type_to_ident(&ts.elem);
            format_ident!("{}Slice", inner)
        }
        Type::Array(ta) => {
            let inner = type_to_ident(&ta.elem);
            let suffix = if let Expr::Lit(lit) = &ta.len {
                quote!(#lit).to_string()
            } else {
                String::new()
            };
            format_ident!("{}Array{}", inner, suffix)
        }
        _ => format_ident!("GenericType"),
        // TODO Implement these
        // Type::Ptr(_) => todo!(),
        // Type::BareFn(type_bare_fn) => todo!(),
        // Type::Group(type_group) => todo!(),
        // Type::ImplTrait(type_impl_trait) => todo!(),
        // Type::Infer(type_infer) => todo!(),
        // Type::Macro(type_macro) => todo!(),
        // Type::Never(type_never) => todo!(),
        // Type::Paren(type_paren) => todo!(),
        // Type::TraitObject(type_trait_object) => todo!(),
        // Type::Tuple(type_tuple) => todo!(),
        // Type::Verbatim(token_stream) => todo!(),
    }
}

pub fn skerry_impl(attr: TokenStream, item: TokenStream) -> TokenStream {
    let mut input_impl = parse_macro_input!(item as ItemImpl);
    let mut top_level_code = quote! {};
    let attr_args = syn::parse_macro_input!(attr as ImplTraitAttribute);

    for item in &mut input_impl.items {
        if let ImplItem::Fn(method) = item {
            if let Some(pos) = method
                .attrs
                .iter()
                .position(|a| a.path().is_ident("skerry_fn"))
            {
                method.attrs.remove(pos);

                let method_tokens = quote!(#method).into();

                let prefix = match &attr_args.prefix {
                    Some(p) => Some(p.clone()),
                    None => Some(type_to_ident(&input_impl.self_ty)),
                };

                match crate::internal::skerry_fn::skerry_fn_standard(
                    TokenStream::new(),
                    method_tokens,
                    prefix.as_ref(),
                ) {
                    Ok((top_stream, method_stream)) => {
                        top_level_code.extend(top_stream);

                        match syn::parse2::<ImplItemFn>(method_stream) {
                            Ok(new_method) => *method = new_method,
                            Err(e) => return e.to_compile_error().into(),
                        }
                    }
                    Err(compile_error) => return compile_error,
                }
            }
        }
    }

    let expanded = quote! {
        #top_level_code

        #input_impl
    };

    TokenStream::from(expanded)
}
