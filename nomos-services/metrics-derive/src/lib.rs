use heck::{ToShoutySnakeCase, ToSnakeCase};
use proc_macro2::{Delimiter, TokenTree};
use quote::{format_ident, quote, ToTokens};
use syn::{parse_macro_input, Attribute, DeriveInput};

#[proc_macro_derive(MetricsData, attributes(metrics))]
pub fn metrics_query(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let output = match metrics_data_codegen(input) {
        Ok(output) => output,
        Err(err) => err.to_compile_error(),
    };
    output.into()
}

fn metrics_data_codegen(input: DeriveInput) -> Result<proc_macro2::TokenStream, syn::Error> {
    match input.data {
        syn::Data::Enum(item_enum) => {
            let mut variants = vec![];
            for variant in item_enum.variants {
                let span = variant.clone();
                variants.push(Variant {
                    attrs: variant.attrs,
                    name: variant.ident,
                    field: match variant.fields.into_iter().next() {
                        Some(field) => field,
                        None => {
                            return Err(syn::Error::new_spanned(
                                span,
                                "Variant must have exactly one field",
                            ))
                        }
                    },
                    key: None,
                });
            }

            MetricsData {
                name: input.ident,
                variants,
            }
            .codegen()
        }
        syn::Data::Struct(s) => {
            let mut variants = vec![];
            for field in s.fields {
                variants.push(Variant {
                    attrs: field.attrs.clone(),
                    name: match &field.ident {
                        Some(name) => name.clone(),
                        None => {
                            return Err(syn::Error::new_spanned(field, "field must have a name"))
                        }
                    },
                    field,
                    key: None,
                });
            }

            MetricsData {
                name: input.ident,
                variants,
            }
            .codegen()
        }
        _ => Err(syn::Error::new_spanned(
            input,
            "MetricsData cannot be derived for unions",
        )),
    }
}

struct MetricsData {
    name: syn::Ident,
    variants: Vec<Variant>,
}

impl MetricsData {
    fn codegen(self) -> Result<proc_macro2::TokenStream, syn::Error> {
        let name = &self.name;
        let mut query_fns = vec![];
        let mut metrics = false;

        for mut variant in self.variants {
            for attr in &mut variant.attrs {
                if attr.path.is_ident("metrics") {
                    if metrics {
                        return Err(syn::Error::new_spanned(attr, "duplicate metrics attribute"));
                    }

                    variant.key = Variant::apply_attr(attr)?;
                    metrics = true;
                }
            }
            query_fns.push(variant.codegen(name)?);
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
}

struct Variant {
    name: syn::Ident,
    field: syn::Field,
    key: Option<syn::Expr>,
    attrs: Vec<Attribute>,
}

impl Variant {
    fn apply_attr(attr: &Attribute) -> Result<Option<syn::Expr>, syn::Error> {
        let ts = match attr.tokens.to_token_stream().into_iter().next() {
            Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Parenthesis => g.stream(),
            _ => {
                return Err(syn::Error::new_spanned(
                    &attr.tokens,
                    "Expected parenthesis",
                ))
            }
        };

        let mut key = None;
        let mut iter = ts.into_iter();
        let mut duplicate_key = false;
        while let Some(tt) = iter.next() {
            match tt {
                TokenTree::Ident(name) => {
                    let name_str = name.to_string();
                    if name_str == "key" {
                        if duplicate_key {
                            return Err(syn::Error::new_spanned(attr, "duplicate key attribute"));
                        }
                        remove_eqaul_symbol(&name.span(), &mut iter)?;
                        key = Some(syn::parse2::<syn::Expr>(extract_attr(&mut iter))?);
                        duplicate_key = true;
                        continue;
                    }
                }
                x => return Err(syn::Error::new_spanned(&x, "Expected identifier")),
            }
        }

        Ok(key)
    }

    fn codegen(self, name: &syn::Ident) -> Result<proc_macro2::TokenStream, syn::Error> {
        let key = &self.key;
        let field_type = &self.field.ty;
        let variant_name = &self.name;
        let variant_name_str = variant_name.to_string();
        let variant_query_fn = format_ident!("{}", &variant_name.to_string().to_snake_case());

        let variant_key = format_ident!("{}", &variant_name.to_string().to_shouty_snake_case());
        let key = key
            .as_ref()
            .map(|tt| quote! { #tt })
            .unwrap_or(quote! { #variant_name_str });

        Ok(quote! {
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
        })
    }
}

fn remove_eqaul_symbol(
    span: &proc_macro2::Span,
    iter: &mut dyn Iterator<Item = TokenTree>,
) -> Result<(), syn::Error> {
    match iter.next() {
        Some(TokenTree::Punct(p)) if p.as_char() == '=' => Ok(()),
        Some(x) => Err(syn::Error::new_spanned(&x, "Expected equal symbol")),
        None => Err(syn::Error::new(
            *span,
            "Expected equal symbol, but got nothing",
        )),
    }
}

fn extract_attr(iter: &mut dyn Iterator<Item = TokenTree>) -> proc_macro2::TokenStream {
    iter.take_while(|tt| match tt {
        TokenTree::Punct(p) => !(p.as_char() == ',' && p.spacing() == proc_macro2::Spacing::Alone),
        _ => true,
    })
    .collect()
}
