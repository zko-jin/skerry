use proc_macro::TokenStream;
use quote::quote;
use syn::{Item, ItemMod, parse_macro_input};

pub fn skerry_mod(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input_mod = parse_macro_input!(item as ItemMod);

    let (_, items) = input_mod
        .content
        .expect("error_module must be used on a module with a body");

    let mut struct_names = Vec::new();
    let mut out_items = Vec::new();

    for item in items {
        if let Item::Struct(s) = &item {
            struct_names.push(s.ident.clone());
        }
        out_items.push(item);
    }

    let expanded = quote! {
        #(#out_items)*

        pub enum GlobalErrors<E: skerry::skerry_internals::SkerryError> {
            #(
                #struct_names(#struct_names),
            )*
            _PHANTOM(std::marker::PhantomData<E>)
        }

        #[macro_export]
        macro_rules! skerry_impl_missing_errors {
            ($ty:ty, [$($v:ident),*]) => {
                skerry::skerry_internals::impl_missing_converts!(
                    $ty,
                    [#(#struct_names),*],
                    [$($v),*]
                );
            };
        }

        #(
            impl skerry::skerry_internals::SkerryError for #struct_names {}

            // skerry_impl_missing_errors!(#struct_names, [#struct_names], no_expand);

            impl<I: Into<#struct_names>> From<I> for GlobalErrors<#struct_names> {
                fn from(val: I) -> Self {
                    Self::#struct_names(val.into())
                }
            }
        )*

    };

    TokenStream::from(expanded)
}
