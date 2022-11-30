use heck::{ToShoutySnakeCase, ToSnakeCase};
use quote::{format_ident, quote};
use syn::{parse_macro_input, DeriveInput};

#[proc_macro_derive(MetricsData)]
pub fn metrics_query(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input_enum = parse_macro_input!(input as DeriveInput);
    let output = match metrics_data_codegen(&input_enum) {
        Ok(output) => output,
        Err(err) => err.to_compile_error(),
    };
    output.into()
}

fn metrics_data_codegen(input: &DeriveInput) -> Result<proc_macro2::TokenStream, syn::Error> {
    match &input.data {
        syn::Data::Enum(item_enum) => {
            let enum_name = &input.ident;
            let mut query_fns = vec![];
            for variant in &item_enum.variants {
                let mut key = None;
                for attr in &variant.attrs {
                    if attr.path.is_ident("key") {
                        // default key
                        key = Some(syn::parse2::<syn::Expr>(attr.tokens.clone())?);
                    }
                }
                if variant.fields.len() != 1 {
                    return Err(syn::Error::new_spanned(
                        variant,
                        "variant must have exactly one field",
                    ));
                }

                let field = variant.fields.iter().next().unwrap();
                let field_type = &field.ty;

                let variant_name = &variant.ident;
                let variant_name_str = variant_name.to_string();
                let variant_query_fn =
                    format_ident!("{}", &variant.ident.to_string().to_snake_case());

                let variant_key =
                    format_ident!("{}", &variant.ident.to_string().to_shouty_snake_case());
                let key = key
                    .map(|tt| quote! { #tt })
                    .unwrap_or(quote! { #variant_name_str });
                query_fns.push(quote! {
                    pub async fn #variant_query_fn(&self) -> ::core::option::Option<#field_type> {
                        static #variant_key: once_cell::sync::OnceCell<::overwatch_rs::services::ServiceId> = ::once_cell::sync::OnceCell::new();

                        let map = self.stack.lock();
                        map
                        .get(#variant_key.get_or_init(|| {
                          #key
                        }))
                        .cloned()
                        .map(|data| match data {
                            #enum_name::#variant_name(data) => data,
                            _ => ::std::unreachable!(),
                        })
                    }
                });
            }

            Ok(quote! {
                #[derive(Debug, Clone)]
                #[repr(transparent)]
                pub struct MetricsBackend {
                    stack: ::std::sync::Arc<::parking_lot::Mutex<::std::collections::HashMap<::overwatch_rs::services::ServiceId, #enum_name>>>,
                }

                impl Default for MetricsBackend {
                    fn default() -> Self {
                        Self {
                            stack: ::std::sync::Arc::new(::parking_lot::Mutex::new(::std::collections::HashMap::new())),
                        }
                    }
                }

                impl MetricsBackend {
                    pub fn new() -> Self {
                        Self::default()
                    }
                }

                #[::async_graphql::Object]
                impl MetricsBackend {
                    #(#query_fns)*
                }
            })
        }
        syn::Data::Struct(s) => {
            let name = &input.ident;
            let mut query_fns = vec![];
            for field in &s.fields {
                let mut key = None;
                for attr in &field.attrs {
                    if attr.path.is_ident("key") {
                        // default key
                        key = Some(syn::parse2::<syn::Expr>(attr.tokens.clone())?);
                    }
                }
                let field_type = &field.ty;

                let variant_name = match &field.ident {
                    Some(name) => name.clone(),
                    None => return Err(syn::Error::new_spanned(field, "field must have a name")),
                };

                let variant_name_str = variant_name.to_string();
                let variant_query_fn =
                    format_ident!("{}", &variant_name.to_string().to_snake_case());

                let variant_key =
                    format_ident!("{}", &variant_name.to_string().to_shouty_snake_case());
                let key = key
                    .map(|tt| quote! { #tt })
                    .unwrap_or(quote! { #variant_name_str });
                query_fns.push(quote! {
                    pub async fn #variant_query_fn(&self) -> ::core::option::Option<#field_type> {
                        static #variant_key: once_cell::sync::OnceCell<::overwatch_rs::services::ServiceId> = ::once_cell::sync::OnceCell::new();

                        let map = self.stack.lock();
                        map
                        .get(#variant_key.get_or_init(|| {
                          #key
                        }))
                        .cloned()
                        .map(|data| match data {
                            #name::#variant_name(data) => data,
                            _ => ::std::unreachable!(),
                        })
                    }
                });
            }

            Ok(quote! {
                #[derive(Debug, Clone)]
                #[repr(transparent)]
                pub struct MetricsBackend {
                    stack: ::std::sync::Arc<::parking_lot::Mutex<::std::collections::HashMap<::overwatch_rs::services::ServiceId, #name>>>,
                }

                impl Default for MetricsBackend {
                    fn default() -> Self {
                        Self {
                            stack: ::std::sync::Arc::new(::parking_lot::Mutex::new(::std::collections::HashMap::new())),
                        }
                    }
                }

                impl MetricsBackend {
                    pub fn new() -> Self {
                        Self::default()
                    }
                }

                #[::async_graphql::Object]
                impl MetricsBackend {
                    #(#query_fns)*
                }
            })
        }
        _ => Err(syn::Error::new_spanned(
            input,
            "MetricsData cannot be derived for unions",
        )),
    }
}
