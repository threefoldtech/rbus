use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{
    parse_macro_input, punctuated::Punctuated, AttributeArgs, FnArg, GenericArgument, ItemTrait,
    Lit, LitStr, Meta, NestedMeta, Pat, PathArguments, ReturnType, TraitItem, TraitItemMethod,
    Type,
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
fn sender_inner_type(
    ty: &FnArg,
) -> Result<&Punctuated<GenericArgument, syn::token::Comma>, &'static str> {
    if let FnArg::Typed(t) = &ty {
        if let Type::Path(p) = t.ty.as_ref() {
            if let Some(seg) = p.path.segments.iter().last() {
                if format!("{}", seg.ident) == "Sender" {
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
                return st.value();
            }
        }
    }

    return format!("{}", m.sig.ident);
}

fn is_stream(m: &TraitItemMethod) -> bool {
    // must take 2 arguments
    if !m.attrs.iter().any(|att| att.path.is_ident("stream")) {
        return false;
    }

    if m.sig.asyncness.is_none() {
        panic!("stream method must be async");
    }

    if m.sig.inputs.len() != 2 {
        panic!("stream method must take (&self, Sender<T>) as arguments");
    }
    // must return ()
    if m.sig.output != ReturnType::Default {
        panic!("stream method must not return any type");
    }

    let rec = &m.sig.inputs[1];

    if let FnArg::Typed(typ) = rec {
        if let Type::Path(p) = typ.ty.as_ref() {
            if let Some(seg) = p.path.segments.iter().last() {
                if format!("{}", seg.ident) != "Sender" {
                    panic!("argument must be of a server::Sender<T> type");
                }
            }
        }
    }

    return true;
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
/// Streams (or events) are supported by adding a method to the trait as follows:
///
/// ```example
///   #[stream]
///   async fn name_of_stream(&self, Sender<T>);
/// ```
/// where T is any concrete type. This method then can use sender to broadcast objects of type T
/// whenever it's needed (timer, on certain events, etc...)
///
/// The stream functions doesn't have to return since it is spawned in it's own routing, hence when
/// streams needed the implementation of the trait need to be Clone (self need to be Clone).
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

    let functions: Vec<&TraitItem> = input
    .items
    .iter()
    .filter(|item| matches!(item, TraitItem::Method(m) if !m.sig.inputs.is_empty() && matches!(m.sig.inputs[0], FnArg::Receiver(_))  && !is_stream(&m))).collect();

    let streams: Vec<&TraitItem> = input
    .items
    .iter()
    .filter(|item| matches!(item, TraitItem::Method(m) if !m.sig.inputs.is_empty() && matches!(m.sig.inputs[0], FnArg::Receiver(_))  && is_stream(&m))).collect();

    let dispatches = functions.iter().map(|item| {
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

    let stub_calls = functions.iter().map(|item| {
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
            return quote! {
                pub async fn #name(&self, #(#inputs,)*) -> rbus::protocol::Result<#ret> {
                    let req = rbus::protocol::Request::new(self.object.clone(), #name_lit)
                        #(.arg(#arg_names)?)*;

                    let out = self.client.request(&self.module, req).await?;

                    out.into()
                }
            };
        }
        unreachable!()
    });
    let streams_stub_calls = streams.iter().map(|item| {
        if let TraitItem::Method(method) = item {
            let name = &method.sig.ident;
            let name_lit = method_name(method);
            let ret = sender_inner_type(&method.sig.inputs[1]).unwrap();
            return quote! {
                pub async fn #name(&self) -> rbus::protocol::Result<rbus::client::Receiver<#ret>> {
                    let receiver = self.client.stream(&self.module, self.object.clone(), #name_lit).await;

                    receiver
                }
            };
        }
        unreachable!()
    });
    let streams_init = streams.iter().map(|item| {
        if let TraitItem::Method(method) = item {
            let name = &method.sig.ident;
            let name_lit = method_name(method);
            return quote! {
                let (sender, sink) = rbus::server::Sender::new();
                let inner = self.inner.clone();
                tokio::spawn(async move {
                    inner.#name(sender).await;
                });
                sinks.insert(#name_lit.to_owned(), sink);
            };
        }

        unreachable!();
    });

    let bounds = if streams.len() > 0 {
        quote! {
            #name_id + Clone + Send + Sync + 'static
        }
    } else {
        quote! {
            #name_id + Send + Sync + 'static
        }
    };

    let vis = &input.vis;
    let output = quote! {
        #[allow(non_snake_case)]

        #[async_trait::async_trait]
        #cleaned

        #vis struct #name_object<T>
        where
            T: #bounds,
        {
            inner: T,
        }

        #[async_trait::async_trait]
        impl<T> rbus::server::Object for #name_object<T>
        where
            T: #bounds,
        {
            fn id(&self) -> rbus::protocol::ObjectID {
                rbus::protocol::ObjectID::new(#name_lit, #version_lit)
            }

            async fn dispatch(&self, request: rbus::protocol::Request) -> rbus::protocol::Result<rbus::protocol::Output> {
                match request.method.as_str() {
                    #(#dispatches,)*

                    _ => Err(rbus::protocol::Error::UnknownMethod(request.method)),
                }
            }
            fn streams(&self) -> rbus::protocol::Result<std::collections::HashMap<String, rbus::server::Sink>>{
                // TODO: build streams from trait definition
                let mut sinks = std::collections::HashMap::default();
                #(#streams_init)*
                Ok(sinks)
            }
        }

        impl<T> From<T> for #name_object<T>
        where
            T: #bounds,
        {
            fn from(inner: T) -> Self {
                Self { inner }
            }
        }

        #vis struct #name_stub {
            module: String,
            client: rbus::client::Client,
            object: rbus::protocol::ObjectID,
        }

        impl #name_stub {
            pub fn new<S: Into<String>>(module: S, client: rbus::client::Client) -> #name_stub {
                #name_stub {
                    module: module.into(),
                    client,
                    object: rbus::protocol::ObjectID::new(#name_lit, #version_lit),
                }
            }

            #(#stub_calls)*
            #(#streams_stub_calls)*
        }
    };

    output.into()
}
