use proc_macro::TokenStream;
use quote::quote;
use syn::{
    Fields,
    Item,
    ItemMod,
    parse_macro_input,
};

pub fn skerry_mod(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input_mod = parse_macro_input!(item as ItemMod);

    let (_, items) = input_mod
        .content
        .expect("error_module must be used on a module with a body");

    let mut struct_names = Vec::new();
    let mut out_items = Vec::new();
    let mut from_macros = Vec::new();

    for mut item in items {
        if let Item::Struct(s) = &mut item {
            let from_idx = s.attrs.iter().position(|attr| attr.path().is_ident("from"));

            if let Some(index) = from_idx {
                s.attrs.remove(index);

                if let Fields::Unnamed(fields) = &s.fields {
                    if fields.unnamed.len() == 1 {
                        let first_field = fields.unnamed.first().unwrap();
                        from_macros.push((s.ident.clone(), first_field.ty.clone()));
                    } else {
                        return syn::Error::new_spanned(
                            &s.fields,
                            "#[from] structs must have exactly one field",
                        )
                        .to_compile_error()
                        .into();
                    }
                } else {
                    return syn::Error::new_spanned(s, "#[from] can only be used on tuple structs")
                        .to_compile_error()
                        .into();
                }
            }
            struct_names.push(s.ident.clone());
        }
        out_items.push(item);
    }

    let from_impls = from_macros.iter().map(|(struct_name, inner_type)| {
        quote! {
            impl From<#inner_type> for #struct_name {
                fn from(val: #inner_type) -> #struct_name {
                    Self(val)
                }
            }
        }
    });

    let expanded = quote! {
        #(#out_items)*

        #(#from_impls)*

        pub enum GlobalErrors<E> {
            #(
                #struct_names(#struct_names),
            )*
            _PHANTOM(std::marker::PhantomData<E>)
        }
    };

    TokenStream::from(expanded)
}
