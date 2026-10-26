//! Declarative construction of Harbor widget view trees.
//!
//! This crate deliberately owns only parsing and token generation. Generated code
//! resolves all Harbor APIs through `harbor_widget::__macro_support`.

mod ast;
mod expand;
mod parse;

use proc_macro::TokenStream;

/// Builds a view description from a Component tree.
///
/// Use `view! { root }`. A child value is an ordinary Rust `expression;`,
/// a child-bearing component is `expression => { children }`, and bare `for`,
/// `if` / `else`, and `match` organize child lists. Rust blocks are ordinary
/// value expressions and therefore require a trailing semicolon. Roots and
/// child values convert through `IntoChildView`: components defer until
/// reconciliation assigns their Fiber, already-built `View` values pass
/// through unchanged.
#[proc_macro]
pub fn view(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as ast::ViewInput);
    match expand::expand(input) {
        Ok(expansion) => expansion.into(),
        Err(error) => error.into_compile_error().into(),
    }
}
