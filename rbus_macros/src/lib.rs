use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, ItemTrait};

#[proc_macro_attribute]
pub fn interface(_attr: TokenStream, input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as ItemTrait);

    TokenStream::from(quote! {#input})
}
