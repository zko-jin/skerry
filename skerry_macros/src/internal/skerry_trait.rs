use proc_macro::TokenStream;
use quote::quote;
use syn::{ItemTrait, TraitItem, TraitItemFn, parse_macro_input};

use crate::internal::skerry_impl::ImplAttribute;

pub fn skerry_trait(attr: TokenStream, item: TokenStream) -> TokenStream {
    let mut input_trait = parse_macro_input!(item as ItemTrait);
    let mut top_level_code = quote! {};
    let attr_args = syn::parse_macro_input!(attr as ImplAttribute);

    for item in &mut input_trait.items {
        if let TraitItem::Fn(method) = item {
            if let Some(pos) = method
                .attrs
                .iter()
                .position(|a| a.path().is_ident("skerry_fn"))
            {
                method.attrs.remove(pos);

                let method_tokens = quote!(#method).into();

                match crate::internal::skerry_fn::skerry_fn_trait(
                    TokenStream::new(),
                    method_tokens,
                    attr_args.prefix.as_ref(),
                ) {
                    Ok((top_stream, method_stream)) => {
                        top_level_code.extend(top_stream);

                        match syn::parse2::<TraitItemFn>(method_stream) {
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

        #input_trait
    };

    TokenStream::from(expanded)
}
