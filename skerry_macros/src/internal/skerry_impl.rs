use proc_macro::TokenStream;
use quote::quote;
use syn::{
    Ident, ImplItem, ImplItemFn, ItemImpl, Token, parenthesized,
    parse::{Parse, ParseStream},
    parse_macro_input,
};

pub struct ImplAttribute {
    pub prefix: Option<Ident>,
}
impl Parse for ImplAttribute {
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

        Ok(ImplAttribute { prefix })
    }
}

pub fn skerry_impl(attr: TokenStream, item: TokenStream) -> TokenStream {
    let mut input_impl = parse_macro_input!(item as ItemImpl);
    let mut top_level_code = quote! {};
    let attr_args = syn::parse_macro_input!(attr as ImplAttribute);

    // Iterate through the items in the impl (methods, etc.)
    for item in &mut input_impl.items {
        if let ImplItem::Fn(method) = item {
            // Check for the #[skerry_fn] marker
            if let Some(pos) = method
                .attrs
                .iter()
                .position(|a| a.path().is_ident("skerry_fn"))
            {
                method.attrs.remove(pos);

                // Convert method to TokenStream to pass to your logic
                let method_tokens = quote!(#method).into();

                match crate::internal::skerry_fn::skerry_fn_standard(
                    TokenStream::new(),
                    method_tokens,
                    attr_args.prefix.as_ref(),
                ) {
                    Ok((top_stream, method_stream)) => {
                        // 1. Add the generated Enum/Macro call to the top-level bucket
                        top_level_code.extend(top_stream);

                        // 2. Replace the current method with the transformed one
                        // We parse it back into an ImplItemFn
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

    // Wrap everything back up
    let expanded = quote! {
        #top_level_code

        #input_impl
    };

    TokenStream::from(expanded)
}
