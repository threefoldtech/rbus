use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{
    parse_macro_input, punctuated::Punctuated, AttributeArgs, FnArg, GenericArgument, Ident,
    ItemTrait, Lit, LitStr, Meta, NestedMeta, Pat, Path, PathArguments, ReturnType, TraitItem,
    TraitItemMethod, Type,
};

fn return_inner_type(
    ty: &ReturnType,
) -> Result<&Punctuated<GenericArgument, syn::token::Comma>, &'static str> {
    if let ReturnType::Type(_, ty) = &ty {
        if let Type::Path(p) = ty.as_ref() {
            if let Some(seg) = p.path.segments.iter().last() {
                if format!("{}", seg.ident) == "Result" {
                    if let PathArguments::AngleBracketed(inner) = &seg.arguments {
                        return Ok(&inner.args);
                    }
                }
            }
        }
    }

    Err("all interface method must return Result<T>")
}

fn method_name(m: &TraitItemMethod) -> String {
    for attr in m.attrs.iter() {
        if !attr.path.is_ident("rename") {
            continue;
        }

        let meta = attr
            .parse_meta()
            .expect("rename require single string literal");

        if let Meta::List(l) = meta {
            if !l.path.is_ident("rename") {
                continue;
            }

            let meta = l
                .nested
                .first()
                .expect("rename requires single string literal");
            if let NestedMeta::Lit(Lit::Str(st)) = meta {
                return format!("{}", st.value());
            }
        }
    }

    return format!("{}", m.sig.ident);
}
/// annotate the service trait with `object` this will
/// generate a usable server and client stubs.
/// it accepts
/// - name [optional] default to trait name
/// - version [optional] default to 1.0
///
/// NOTE:
/// - only trait methods with first argument as receiver will be available for RPC
/// - receiver must be a shared ref to self (&self)
/// - all input arguments must be of type <T: Serialize>
/// - return must be a Result (any Result) as long as the E type can be stringfied <E: Display>
///
/// Once a trait is marked as Object, two extra structures will be available in the same scope as the
/// annotated trait.
///
/// `struct [Name]Object;`
/// `struct [Name]Stub;`
///
/// where [Name] is replaced by actual trait name
///
/// The generated Object is a wrapper that can be used on top of any of the trait implementation
/// that takes care of call dispatching and serialization/deserialization of inputs/outputs
///
/// `let obj = [Name]Object::from(traitImpl);`
///
/// the generated Stub is a wrapper on top of the rbus::Client to abstract calls to remote service
///
/// `let stub = [Name]Stub::new("module", client);`
///
/// for compatibility with other languages (for example Golang) there is a helper attribute
/// #[rename("new_name")] that can be added on method to rename the method. Since rust uses
/// snake_case, while Go uses CamelCase. rename is needed if method will be used across languages
///
#[proc_macro_attribute]
pub fn object(args: TokenStream, input: TokenStream) -> TokenStream {
    let args = parse_macro_input!(args as AttributeArgs);

    let input = parse_macro_input!(input as ItemTrait);
    let mut cleaned = input.clone();

    for item in cleaned.items.iter_mut() {
        if let TraitItem::Method(ref mut method) = item {
            //TODO: this removes ALL attributes on the
            //method. May be we should only remove the rename attribute
            method.attrs = vec![];
        }
    }

    let name_id = &input.ident;
    let name = format!("{}", name_id);
    let name_mod = format_ident!("{}_mod", name_id);
    let name_object = format_ident!("{}Object", name_id);
    let name_stub = format_ident!("{}Stub", name_id);

    let mut name_lit = Lit::Str(LitStr::new(&name, name_id.span()));
    let mut version_lit = Lit::Str(LitStr::new("1.0", name_id.span()));

    for arg in args {
        if let NestedMeta::Meta(Meta::NameValue(value)) = arg {
            if value.path.is_ident("name") {
                name_lit = value.lit;
            } else if value.path.is_ident("version") {
                version_lit = value.lit;
            }
        }
    }

    let dispatches = input
        .items
        .iter()
        .filter(|item| matches!(item, TraitItem::Method(m) if m.sig.inputs.len() > 0 && matches!(m.sig.inputs[0], FnArg::Receiver(_))))
        .map(|item| {
            if let TraitItem::Method(method) = item {
                let name_id = &method.sig.ident;
                let name_lit = method_name(method);
                let args = (0..method.sig.inputs.len() - 1).map(syn::Index::from);
                let branch = if method.sig.asyncness.is_none() {
                    quote! {
                        #name_lit => Ok(self
                            .inner
                            .#name_id(
                                #( request.inputs.at(#args)?, )*
                            )
                            .into())
                    }
                } else {
                    quote! {
                        #name_lit => Ok(self
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

    let stub_calls = input
    .items
    .iter()
    .filter(|item| matches!(item, TraitItem::Method(m) if m.sig.inputs.len() > 0 && matches!(m.sig.inputs[0], FnArg::Receiver(_))))
    .map(|item| {
        if let TraitItem::Method(method) = item {
            let name = &method.sig.ident;
            let name_lit = method_name(method);
            let inputs = method.sig.inputs.iter().skip(1);
            let arg_names = method.sig.inputs.iter().skip(1).map(|arg| {
               if let FnArg::Typed(a) = &arg {
                    if let Pat::Ident(i) = a.pat.as_ref() {
                        return &i.ident;
                    }
               }
               unreachable!();
            });
            let ret = return_inner_type(&method.sig.output).unwrap();
            return quote!{
                pub async fn #name(&self, #(#inputs,)*) -> protocol::Result<#ret> {
                    let req = protocol::Request::new(self.object.clone(), #name_lit)
                        #(.arg(#arg_names)?)*;

                    let mut client = self.client.clone();
                    let out = client.request(&self.module, req).await?;

                    out.into()
                }
            };
        }
        unreachable!()
    });

    let vis = &input.vis;
    let output = quote! {
        #[allow(non_snake_case)]
        mod #name_mod {
            use super::*;
            use rbus::{
                server,
                client,
                protocol
            };

            #[async_trait::async_trait]
            #cleaned

            pub struct #name_object<T>
            where
                T: #name_id,
            {
                inner: T,
            }

            #[async_trait::async_trait]
            impl<T> server::Object for #name_object<T>
            where
                T: #name_id + Send + Sync + 'static,
            {
                fn id(&self) -> protocol::ObjectID {
                    protocol::ObjectID::new(#name_lit, #version_lit)
                }

                async fn dispatch(&self, request: protocol::Request) -> protocol::Result<protocol::Output> {
                    match request.method.as_str() {
                        #(#dispatches,)*

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

            pub struct #name_stub {
                module: String,
                client: client::Client,
                object: protocol::ObjectID,
            }

            impl #name_stub {
                pub fn new<S: Into<String>>(module: S, client: client::Client) -> #name_stub {
                    #name_stub {
                        module: module.into(),
                        client,
                        object: protocol::ObjectID::new(#name_lit, #version_lit),
                    }
                }

                #(#stub_calls)*
            }
        }

        #vis use #name_mod :: #name_id;
        #vis use #name_mod :: #name_object;
        #vis use #name_mod :: #name_stub;
    };

    output.into()
}
