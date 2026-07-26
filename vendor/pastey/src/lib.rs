extern crate proc_macro;

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, ItemFn, ItemStruct, ItemEnum, ItemTrait, ItemImpl, ItemMod};

#[proc_macro_attribute]
pub fn paste(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item
}

#[proc_macro]
pub fn pastey(_input: TokenStream) -> TokenStream {
    TokenStream::new()
}

#[proc_macro]
pub fn paste_impl(_input: TokenStream) -> TokenStream {
    TokenStream::new()
}

#[proc_macro]
pub fn paste_items(_input: TokenStream) -> TokenStream {
    TokenStream::new()
}

#[proc_macro]
pub fn paste_tokens(_input: TokenStream) -> TokenStream {
    TokenStream::new()
}

#[proc_macro_attribute]
pub fn paste_attr(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item
}

#[proc_macro_derive(Paste, attributes(paste))]
pub fn derive_paste(_item: TokenStream) -> TokenStream {
    TokenStream::new()
}

#[proc_macro_derive(Pastey, attributes(paste))]
pub fn derive_pastey(_item: TokenStream) -> TokenStream {
    TokenStream::new()
}
