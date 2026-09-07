//! Declarative construction of Harbor widget view trees.
//!
//! This crate deliberately owns only parsing and token generation. Generated code
//! resolves all Harbor APIs through `harbor_widget::__macro_support`.

mod ast;
mod expand;
mod parse;

use proc_macro::TokenStream;

/// Builds a concrete root [`harbor_widget::view::View`] from a component tree.
///
/// The syntax is `view!(cx, component => { children })`, where children may be
/// nested nodes, `{ expression }` interpolation, or `for` / `if` / `match`
/// control flow. Constructors, keys, callbacks, and captures remain ordinary
/// Rust expressions.
#[proc_macro]
pub fn view(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as ast::ViewInput);
    match expand::expand(input) {
        Ok(expansion) => expansion.into(),
        Err(error) => error.into_compile_error().into(),
    }
}
