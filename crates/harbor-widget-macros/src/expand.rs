use crate::ast::{Child, IfChild, MatchArm, MatchChild, Node, Root, ViewInput};
use proc_macro_crate::{FoundCrate, crate_name};
use proc_macro2::{Ident, Span, TokenStream};
use quote::{quote, quote_spanned};
use syn::{Error, Result};

pub fn expand(input: ViewInput) -> Result<TokenStream> {
    let support = support_path()?;
    let mut expander = Expander {
        support,
        next_ident: 0,
    };
    Ok(expander.expand_root(&input.root))
}

fn support_path() -> Result<TokenStream> {
    match crate_name("harbor-widget").map_err(|error| {
        Error::new(
            Span::call_site(),
            format!("view! could not resolve harbor-widget: {error}"),
        )
    })? {
        FoundCrate::Itself => Ok(quote!(crate::__macro_support)),
        FoundCrate::Name(name) => {
            let crate_name = Ident::new(&name, Span::call_site());
            Ok(quote!(#crate_name::__macro_support))
        }
    }
}

struct Expander {
    support: TokenStream,
    next_ident: usize,
}

impl Expander {
    fn fresh_ident(&mut self, role: &str) -> Ident {
        let index = self.next_ident;
        self.next_ident += 1;
        Ident::new(&format!("__harbor_view_{role}_{index}"), Span::mixed_site())
    }

    fn expand_root(&mut self, root: &Root) -> TokenStream {
        match root {
            Root::Value { expr, span } => self.expand_value_root(expr, *span),
            Root::Parent(node) => self.expand_parent_root(node),
        }
    }

    fn expand_value_root(&mut self, expr: &syn::Expr, span: Span) -> TokenStream {
        let component = self.fresh_ident("component");
        let support = &self.support;

        quote_spanned! {span=>
            {
                let #component = #support::resolve_node(#expr, ::core::file!(), ::core::line!(), ::core::column!());
                <_ as #support::IntoChildView>::into_child_view(#component)
            }
        }
    }

    fn expand_parent_root(&mut self, node: &Node) -> TokenStream {
        let children = self.fresh_ident("children");
        let component = self.fresh_ident("component");
        let children_body = self.expand_children(&node.children, &children);
        let attachment = self.attachment(&node.children, node.span());
        let component_expr = &node.component;
        let support = &self.support;
        let span = node.span();

        quote_spanned! {span=>
            {
                let mut #children = #support::Children::new();
                #children_body
                let #component = #attachment(
                    #support::resolve_node(#component_expr, ::core::file!(), ::core::line!(), ::core::column!()),
                    #children,
                )
                .unwrap_or_else(|error| {
                    ::core::panic!("view! child construction failed: {}", error)
                });
                <_ as #support::IntoChildView>::into_child_view(#component)
            }
        }
    }

    fn attachment(&self, children: &[Child], span: Span) -> TokenStream {
        let support = &self.support;
        let (minimum, maximum) = child_bounds(children);
        // Even around a dynamic loop, two unconditional values are invalid.
        match maximum.or_else(|| (minimum >= 2).then_some(2)) {
            Some(bound) => quote_spanned!(span=> #support::with_static_children::<#bound, _>),
            None => quote_spanned!(span=> <_ as #support::WithChildren>::with_children),
        }
    }

    fn expand_children(&mut self, children: &[Child], parent: &Ident) -> TokenStream {
        let mut expansion = TokenStream::new();
        for child in children {
            expansion.extend(self.expand_child(child, parent));
        }
        expansion
    }

    fn expand_child(&mut self, child: &Child, parent: &Ident) -> TokenStream {
        match child {
            Child::Value { expr, span } => self.expand_value_child(expr, *span, parent),
            Child::Parent(node) => self.expand_parent_child(node, parent),
            Child::For(child) => {
                let pattern = &child.pattern;
                let iter = &child.iter;
                let body = self.expand_children(&child.body, parent);
                let span = child.for_token.span;
                quote_spanned! {span=>
                    for #pattern in #iter {
                        #body
                    }
                }
            }
            Child::If(child) => self.expand_if(child, parent),
            Child::Match(child) => self.expand_match(child, parent),
        }
    }

    fn expand_value_child(&mut self, expr: &syn::Expr, span: Span, parent: &Ident) -> TokenStream {
        let value = self.fresh_ident("value");
        let child_view = self.fresh_ident("child");
        let support = &self.support;

        quote_spanned! {span=>
            {
                let #value = #support::resolve_node(#expr, ::core::file!(), ::core::line!(), ::core::column!());
                let #child_view = <_ as #support::IntoChildView>::into_child_view(#value);
                #support::Children::push(&mut #parent, #child_view);
            }
        }
    }

    fn expand_parent_child(&mut self, node: &Node, parent: &Ident) -> TokenStream {
        let children = self.fresh_ident("children");
        let component = self.fresh_ident("component");
        let child_view = self.fresh_ident("child");
        let children_body = self.expand_children(&node.children, &children);
        let attachment = self.attachment(&node.children, node.span());
        let component_expr = &node.component;
        let support = &self.support;
        let span = node.span();

        quote_spanned! {span=>
            {
                let mut #children = #support::Children::new();
                #children_body
                let #component = #attachment(
                    #support::resolve_node(#component_expr, ::core::file!(), ::core::line!(), ::core::column!()),
                    #children,
                )
                .unwrap_or_else(|error| {
                    ::core::panic!("view! child construction failed: {}", error)
                });
                let #child_view = <_ as #support::IntoChildView>::into_child_view(#component);
                #support::Children::push(&mut #parent, #child_view);
            }
        }
    }

    fn expand_if(&mut self, child: &IfChild, parent: &Ident) -> TokenStream {
        let first = &child.branches[0];
        let condition = &first.condition;
        let body = self.expand_children(&first.body, parent);
        let mut tail = match &child.else_body {
            Some(body) => {
                let body = self.expand_children(body, parent);
                quote!(else { #body })
            }
            None => TokenStream::new(),
        };

        for branch in child.branches[1..].iter().rev() {
            let condition = &branch.condition;
            let body = self.expand_children(&branch.body, parent);
            tail = quote!(else if #condition { #body } #tail);
        }

        let span = first.if_token.span;
        quote_spanned! {span=>
            if #condition { #body } #tail
        }
    }

    fn expand_match(&mut self, child: &MatchChild, parent: &Ident) -> TokenStream {
        let expr = &child.expr;
        let span = child.match_token.span;
        let arms: Vec<_> = child
            .arms
            .iter()
            .map(|arm| self.expand_match_arm(arm, parent))
            .collect();

        quote_spanned! {span=>
            match #expr {
                #(#arms)*
            }
        }
    }

    fn expand_match_arm(&mut self, arm: &MatchArm, parent: &Ident) -> TokenStream {
        let pattern = &arm.pattern;
        let body = self.expand_children(&arm.body, parent);
        let guard = arm
            .guard
            .as_ref()
            .map(|(_, condition)| quote!(if #condition));

        quote! {
            #pattern #guard => { #body },
        }
    }
}

/// Conservative lower/upper incoming bounds, without executing Rust expressions.
fn child_bounds(children: &[Child]) -> (usize, Option<usize>) {
    children
        .iter()
        .fold((0usize, Some(0usize)), |(min, max), child| {
            let (child_min, child_max) = match child {
                Child::Value { .. } | Child::Parent(_) => (1, Some(1)),
                Child::For(child) => {
                    let (_, upper) = child_bounds(&child.body);
                    (0, if upper == Some(0) { Some(0) } else { None })
                }
                Child::If(child) => {
                    let alternatives = child.branches.iter().map(|b| child_bounds(&b.body)).chain(
                        ::core::iter::once(
                            child
                                .else_body
                                .as_ref()
                                .map_or((0, Some(0)), |body| child_bounds(body)),
                        ),
                    );
                    alternative_bounds(alternatives)
                }
                Child::Match(child) => {
                    alternative_bounds(child.arms.iter().map(|arm| child_bounds(&arm.body)))
                }
            };
            (
                min.saturating_add(child_min),
                max.zip(child_max).map(|(a, b)| a.saturating_add(b)),
            )
        })
}

fn alternative_bounds(
    alternatives: impl Iterator<Item = (usize, Option<usize>)>,
) -> (usize, Option<usize>) {
    alternatives.fold((usize::MAX, Some(0)), |(min, max), (a, b)| {
        (min.min(a), max.zip(b).map(|(x, y)| x.max(y)))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::quote;

    #[test]
    fn expansion_uses_root_child_and_parent_protocols() {
        let input: ViewInput = syn::parse2(quote! {
            Root::new() => {
                existing;
                Row::new() => { Leaf::new(); }
                for item in items { Row::new(item); }
                if visible { Leaf::new(); } else { Leaf::new(); }
                match choice { Some(value) => { Row::new(value); }, None => {} }
            }
        })
        .unwrap();

        let expansion = expand(input).unwrap().to_string();
        assert!(expansion.contains("__macro_support"));
        assert!(expansion.contains("IntoChildView"));
        assert!(expansion.contains("with_static_children"));
        assert!(!expansion.contains("Component>::build"));
        assert!(!expansion.contains("IntoChildren"));
        assert!(!expansion.contains("clone"));
        assert!(!expansion.contains("use_state"));
    }

    #[test]
    fn leaf_root_defers_without_children_attachment() {
        let input: ViewInput = syn::parse2(quote! { Root::new(); }).unwrap();
        let expansion = expand(input).unwrap().to_string();

        assert!(expansion.contains("IntoChildView"));
        assert!(!expansion.contains("WithChildren"));
    }

    #[test]
    fn nested_parents_receive_distinct_internal_collectors() {
        let input: ViewInput = syn::parse2(quote! {
            Root::new() => { Parent::new() => { Leaf::new(); } }
        })
        .unwrap();

        let expansion = expand(input).unwrap().to_string();
        assert!(expansion.contains("__harbor_view_children_0"));
        assert!(expansion.contains("__harbor_view_children_2"));
    }
    #[test]
    fn counts_sequences_and_alternatives_without_summing_exclusive_branches() {
        for (tokens, expected) in [
            (quote! { Parent => {} }, (0, Some(0))),
            (
                quote! { Parent => { if x { a; } else { b; } } },
                (1, Some(1)),
            ),
            (
                quote! { Parent => { match x { A => { a; }, B => { b; } } } },
                (1, Some(1)),
            ),
            (quote! { Parent => { a; if x { b; } } }, (1, Some(2))),
            (quote! { Parent => { for x in xs { a; } } }, (0, None)),
            (quote! { Parent => { a; for x in xs { b; } c; } }, (2, None)),
        ] {
            let input: ViewInput = syn::parse2(tokens).unwrap();
            let Root::Parent(node) = input.root else {
                panic!("expected parent");
            };
            assert_eq!(child_bounds(&node.children), expected);
        }
    }
}
