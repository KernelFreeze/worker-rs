mod event;

use proc_macro::TokenStream;

#[proc_macro_attribute]
pub fn event(attr: TokenStream, item: TokenStream) -> TokenStream {
    event::expand_macro(attr, item)
}
