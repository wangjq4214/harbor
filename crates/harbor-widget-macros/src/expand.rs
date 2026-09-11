use crate::ast::{Child, IfChild, MatchArm, MatchChild, Node, ViewInput};
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
    Ok(expander.expand_root(&input.root, &input.cx))
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

    fn expand_root(&mut self, node: &Node, cx: &syn::Expr) -> TokenStream {
        let children = self.fresh_ident("children");
        let component = self.fresh_ident("component");
        let build_cx = self.fresh_ident("cx");
        let children_body = self.expand_children(&node.children, &children);
        let component_expr = &node.component;
        let support = &self.support;
        let span = node.span();

        quote_spanned! {span=>
            {
                let mut #children = #support::Children::new();
                #children_body
                let #component = <_ as #support::WithChildren>::with_children(
                    #component_expr,
                    #children,
                )
                .unwrap_or_else(|error| {
                    ::core::panic!("view! child construction failed: {}", error)
                });
                let #build_cx = #cx;
                <_ as #support::Component>::build(&#component, #build_cx)
            }
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
            Child::Node(node) => self.expand_node(node, parent),
            Child::Interpolation { expr, span } => {
                let support = &self.support;
                quote_spanned! {*span=>
                    <_ as #support::IntoChildren>::append_to(#expr, &mut #parent);
                }
            }
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

    fn expand_node(&mut self, node: &Node, parent: &Ident) -> TokenStream {
        let children = self.fresh_ident("children");
        let component = self.fresh_ident("component");
        let child_view = self.fresh_ident("child");
        let children_body = self.expand_children(&node.children, &children);
        let component_expr = &node.component;
        let support = &self.support;
        let span = node.span();

        quote_spanned! {span=>
            {
                let mut #children = #support::Children::new();
                #children_body
                let #component = <_ as #support::WithChildren>::with_children(
                    #component_expr,
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

#[cfg(test)]
mod tests {
    use super::*;
    use quote::quote;

    #[test]
    fn expansion_uses_only_the_macro_support_boundary() {
        let input: ViewInput = syn::parse2(quote! {
            cx, Root::new() => {
                { existing }
                for item in items { Row::new(item).keyed(item.id) => {} }
                if visible { Leaf::new() => {} } else { Leaf::new() => {} }
                match choice { Some(value) => { Row::new(value) => {} }, None => {} }
            }
        })
        .unwrap();

        let expansion = expand(input).unwrap().to_string();
        assert!(expansion.contains("__macro_support"));
        assert!(expansion.contains("IntoChildren"));
        assert!(expansion.contains("WithChildren"));
        assert!(!expansion.contains("clone"));
        assert!(!expansion.contains("use_state"));
    }

    #[test]
    fn nested_nodes_receive_distinct_internal_collectors() {
        let input: ViewInput = syn::parse2(quote! {
            cx, Root::new() => { Parent::new() => { Leaf::new() => {} } }
        })
        .unwrap();

        let expansion = expand(input).unwrap().to_string();
        assert!(expansion.contains("__harbor_view_children_0"));
        assert!(expansion.contains("__harbor_view_children_3"));
    }
}
