use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{
    parse_macro_input, AttributeArgs, Ident, ItemTrait, Lit, LitStr, Meta, NestedMeta, Path,
    TraitItem,
};

fn path_ident<'a>(path: &'a Path) -> Option<&'a Ident> {
    path.segments.first().map(|s| &s.ident)
}

// fn stub_branches(ty: &ItemTrait) -> impl Iterator<Item = TokenStream> {
//     branches
// }

#[proc_macro_attribute]
pub fn interface(args: TokenStream, input: TokenStream) -> TokenStream {
    let args = parse_macro_input!(args as AttributeArgs);

    let input = parse_macro_input!(input as ItemTrait);
    let name_id = &input.ident;
    let name = format!("{}", name_id);
    let name_mod = format_ident!("{}_mod", name_id);
    let name_object = format_ident!("{}Object", name_id);

    let mut name_lit = Lit::Str(LitStr::new(&name, name_id.span()));
    let mut version_lit = Lit::Str(LitStr::new("1.0", name_id.span()));

    for arg in args {
        if let NestedMeta::Meta(Meta::NameValue(value)) = arg {
            match path_ident(&value.path).map(|i| format!("{}", i)) {
                Some(name) if name == "name" => {
                    name_lit = value.lit;
                }
                Some(name) if name == "version" => {
                    version_lit = value.lit;
                }
                _ => (),
            };
        }
    }

    let branches = input
        .items
        .iter()
        .filter(|item| matches!(item, TraitItem::Method(_)))
        .map(|item| {
            if let TraitItem::Method(method) = item {
                let name_id = &method.sig.ident;
                let name_str = format!("{}", name_id);
                let args = (0..method.sig.inputs.len() - 1).map(syn::Index::from);
                let branch = if method.sig.asyncness.is_none() {
                    quote! {
                        #name_str => Ok(self
                            .inner
                            .#name_id(
                                #( request.inputs.at(#args)?, )*
                            )
                            .into())
                    }
                } else {
                    quote! {
                        #name_str => Ok(self
                            .inner
                            .#name_id(
                                #( request.inputs.at(#args)?, )*
                            ).await
                            .into())
                    }
                };

                return branch;
            }

            unreachable!();
        });

    let vis = &input.vis;
    let output = quote! {
        #[allow(non_snake_case)]
        mod #name_mod {
            use super::*;
            use rbus::{
                server::Object,
                protocol::{
                    ObjectID,
                    Output,
                    Request}
                };
            #[async_trait::async_trait]
            #input

            pub struct #name_object<T>
            where
                T: #name_id,
            {
                inner: T,
            }

            #[async_trait::async_trait]
            impl<T> Object for #name_object<T>
            where
                T: #name_id + Send + Sync + 'static,
            {
                fn id(&self) -> ObjectID {
                    ObjectID::new(#name_lit, #version_lit)
                }

                async fn dispatch(&self, request: Request) -> protocol::Result<Output> {
                    match request.method.as_str() {
                        #(#branches,)*

                        _ => Err(protocol::Error::UnknownMethod(request.method)),
                    }
                }
            }

            impl<T> From<T> for #name_object<T>
            where
                T: #name_id,
            {
                fn from(inner: T) -> Self {
                    Self { inner }
                }
            }

        }

        #vis use #name_mod :: #name_id;
        #vis use #name_mod :: #name_object;
    };

    output.into()
}
