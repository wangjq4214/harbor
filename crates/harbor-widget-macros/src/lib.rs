//! Declarative construction of Harbor widget view trees.
//!
//! This crate deliberately owns only parsing and token generation. Generated code
//! resolves all Harbor APIs through `harbor_widget::__macro_support`.

mod ast;
mod expand;
mod parse;

use proc_macro::TokenStream;

/// Builds a concrete root [`harbor_widget::view::View`] from a Component tree.
///
/// Use `view! { cx; root }`. A child value is an ordinary Rust `expression;`,
/// a child-bearing component is `expression => { children }`, and bare `for`,
/// `if` / `else`, and `match` organize child lists. Rust blocks are ordinary
/// value expressions and therefore require a trailing semicolon. Roots are built
/// directly through `Component`; child values convert through `IntoChildView`.
#[proc_macro]
pub fn view(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as ast::ViewInput);
    match expand::expand(input) {
        Ok(expansion) => expansion.into(),
        Err(error) => error.into_compile_error().into(),
    }
}
