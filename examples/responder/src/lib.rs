#![feature(proc_macro_diagnostic)]
#![recursion_limit="128"]

#[macro_use] extern crate quote;
extern crate devise;
extern crate proc_macro;
extern crate rocket_http;

mod http_attrs;

use quote::ToTokens;
use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use devise::{*, ext::TypeExt};

use http_attrs::{ContentType, Status};

#[derive(Default, FromMeta)]
struct ItemAttr {
    content_type: Option<SpanWrapped<ContentType>>,
    status: Option<SpanWrapped<Status>>,
}

#[derive(Default, FromMeta)]
struct FieldAttr {
    ignore: bool,
}

#[proc_macro_derive(Responder, attributes(response))]
pub fn derive_responder(input: TokenStream) -> TokenStream {
    DeriveGenerator::build_for(input, quote!(impl<'__r> ::rocket::response::Responder<'__r>))
        .generic_support(GenericSupport::Lifetime)
        .data_support(DataSupport::Struct | DataSupport::Enum)
        .replace_generic(0, 0)
        .validate_generics(|_, generics| match generics.lifetimes().count() > 1 {
            true => Err(generics.span().error("only one lifetime is supported")),
            false => Ok(())
        })
        .validate_fields(|_, fields| match fields.count() {
            0 => return Err(fields.span().error("need at least one field")),
            _ => Ok(())
        })
        .function(|_, inner| quote! {
            fn respond_to(
                self,
                __req: &::rocket::Request
            ) -> ::rocket::response::Result<'__r> {
                #inner
            }
        })
        .try_map_fields(|_, fields| {
            fn set_header_tokens<T: ToTokens + Spanned>(item: T) -> TokenStream2 {
                quote_spanned!(item.span().into() => __res.set_header(#item);)
            }

            let attr = ItemAttr::from_attrs("response", fields.parent_attrs())
                .unwrap_or_else(|| Ok(Default::default()))?;

            let responder = fields.iter().next().map(|f| {
                let (accessor, ty) = (f.accessor(), f.ty.with_stripped_lifetimes());
                quote_spanned! { f.span().into() =>
                   let mut __res = <#ty as ::rocket::response::Responder>::respond_to(
                       #accessor, __req
                   )?;
                }
            }).expect("have at least one field");

            let mut headers = vec![];
            for field in fields.iter().skip(1) {
                let attr = FieldAttr::from_attrs("response", &field.attrs)
                    .unwrap_or_else(|| Ok(Default::default()))?;

                if !attr.ignore {
                    headers.push(set_header_tokens(field.accessor()));
                }
            }

            let content_type = attr.content_type.map(set_header_tokens);
            let status = attr.status.map(|status| {
                quote_spanned!(status.span().into() => __res.set_status(#status);)
            });

            Ok(quote! {
                #responder
                #(#headers)*
                #content_type
                #status
                Ok(__res)
            })
        })
        .to_tokens()
}
