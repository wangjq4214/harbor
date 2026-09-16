use crate::ast::{Child, ForChild, IfBranch, IfChild, MatchArm, MatchChild, Node, Root, ViewInput};
use syn::parse::{Parse, ParseStream};
use syn::{Expr, Pat, Result, Token, braced, spanned::Spanned};

impl Parse for ViewInput {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let cx = input.call(Expr::parse_without_eager_brace)?;
        if input.peek(Token![,]) {
            return Err(input
                .error("view! uses `;` after the context expression; replace this `,` with `;`"));
        }
        input.parse::<Token![;]>()?;

        let root = parse_root(input)?;
        if !input.is_empty() {
            if input.peek(Token![;]) {
                return Err(input.error(
                    "view! parent roots do not take a trailing `;`; remove this semicolon",
                ));
            }
            return Err(input.error("view! accepts exactly one root"));
        }

        Ok(Self { cx, root })
    }
}

fn parse_root(input: ParseStream<'_>) -> Result<Root> {
    let expr = input.parse::<Expr>()?;
    let span = expr.span();

    if input.peek(Token![;]) {
        input.parse::<Token![;]>()?;
        return Ok(Root::Value { expr, span });
    }
    if input.peek(Token![=>]) {
        input.parse::<Token![=>]>()?;
        let children = parse_child_block(input, "view! expected a braced child list after `=>`")?;
        return Ok(Root::Parent(Node {
            component: expr,
            children,
        }));
    }

    Err(input.error("view! expected `;` or `=> { ... }` after the root expression"))
}

fn parse_child_block(input: ParseStream<'_>, message: &str) -> Result<Vec<Child>> {
    if !input.peek(syn::token::Brace) {
        return Err(input.error(message));
    }
    let content;
    braced!(content in input);
    parse_children(&content)
}

fn parse_children(input: ParseStream<'_>) -> Result<Vec<Child>> {
    let mut children = Vec::new();
    while !input.is_empty() {
        children.push(parse_child(input)?);
    }
    Ok(children)
}

fn parse_child(input: ParseStream<'_>) -> Result<Child> {
    let child = if input.peek(Token![for]) {
        parse_for(input).map(Child::For)?
    } else if input.peek(Token![if]) {
        parse_if(input).map(Child::If)?
    } else if input.peek(Token![match]) {
        parse_match(input).map(Child::Match)?
    } else {
        return parse_expression_child(input);
    };

    reject_trailing_semicolon(input, "control-flow child")?;
    Ok(child)
}

fn parse_expression_child(input: ParseStream<'_>) -> Result<Child> {
    let expr = input.parse::<Expr>()?;
    let span = expr.span();

    if input.peek(Token![;]) {
        input.parse::<Token![;]>()?;
        return Ok(Child::Value { expr, span });
    }
    if input.peek(Token![=>]) {
        input.parse::<Token![=>]>()?;
        let children = parse_child_block(input, "view! expected a braced child list after `=>`")?;
        reject_trailing_semicolon(input, "parent child")?;
        return Ok(Child::Parent(Node {
            component: expr,
            children,
        }));
    }

    Err(input.error("view! expected `;` or `=> { ... }` after the child expression"))
}

fn reject_trailing_semicolon(input: ParseStream<'_>, construct: &str) -> Result<()> {
    if input.peek(Token![;]) {
        return Err(input.error(format!(
            "view! {construct} does not take a trailing `;`; remove this semicolon"
        )));
    }
    Ok(())
}

fn parse_for(input: ParseStream<'_>) -> Result<ForChild> {
    let for_token = input.parse::<Token![for]>()?;
    let pattern = input.call(Pat::parse_multi_with_leading_vert)?;
    input.parse::<Token![in]>()?;
    let iter = input.call(Expr::parse_without_eager_brace)?;
    let body = parse_child_block(input, "view! `for` child requires a braced child list")?;

    Ok(ForChild {
        for_token,
        pattern,
        iter,
        body,
    })
}

fn parse_if(input: ParseStream<'_>) -> Result<IfChild> {
    let mut branches = vec![parse_if_branch(input)?];
    let mut else_body = None;

    while input.peek(Token![else]) {
        input.parse::<Token![else]>()?;
        if input.peek(Token![if]) {
            branches.push(parse_if_branch(input)?);
        } else {
            else_body = Some(parse_child_block(
                input,
                "view! `else` child requires a braced child list",
            )?);
            break;
        }
    }

    Ok(IfChild {
        branches,
        else_body,
    })
}

fn parse_if_branch(input: ParseStream<'_>) -> Result<IfBranch> {
    let if_token = input.parse::<Token![if]>()?;
    let condition = input.call(Expr::parse_without_eager_brace)?;
    let body = parse_child_block(input, "view! `if` child requires a braced child list")?;

    Ok(IfBranch {
        if_token,
        condition,
        body,
    })
}

fn parse_match(input: ParseStream<'_>) -> Result<MatchChild> {
    let match_token = input.parse::<Token![match]>()?;
    let expr = input.call(Expr::parse_without_eager_brace)?;
    if !input.peek(syn::token::Brace) {
        return Err(input.error("view! `match` child requires braced match arms"));
    }
    let content;
    braced!(content in input);

    let mut arms = Vec::new();
    while !content.is_empty() {
        let pattern = content.call(Pat::parse_multi_with_leading_vert)?;
        let guard = if content.peek(Token![if]) {
            let if_token = content.parse::<Token![if]>()?;
            let condition = content.call(Expr::parse_without_eager_brace)?;
            Some((if_token, condition))
        } else {
            None
        };
        content.parse::<Token![=>]>()?;
        let body = parse_child_block(&content, "view! match arm body must be a braced child list")?;
        arms.push(MatchArm {
            pattern,
            guard,
            body,
        });

        if content.peek(Token![,]) {
            content.parse::<Token![,]>()?;
        }
    }

    Ok(MatchChild {
        match_token,
        expr,
        arms,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::quote;

    fn parse_error(tokens: proc_macro2::TokenStream) -> syn::Error {
        match syn::parse2::<ViewInput>(tokens) {
            Ok(_) => panic!("input unexpectedly parsed"),
            Err(error) => error,
        }
    }

    #[test]
    fn parses_values_parents_blocks_and_control_flow() {
        let input: ViewInput = syn::parse2(quote! {
            cx; Root::new() => {
                Leaf::new();
                existing;
                { let value = 1; Leaf::new(value) };
                for item in items { Row::new(item); }
                if enabled { Leaf::new(); } else if fallback { Leaf::new(); } else { Leaf::new(); }
                match choice {
                    Some(value) if value > 0 => { Row::new(value); },
                    None => {}
                }
            }
        })
        .unwrap();

        let Root::Parent(root) = input.root else {
            panic!("parent root unexpectedly parsed as a value");
        };
        assert_eq!(root.children.len(), 6);
        assert!(matches!(root.children[0], Child::Value { .. }));
        assert!(matches!(root.children[1], Child::Value { .. }));
        assert!(matches!(root.children[2], Child::Value { .. }));
        assert!(matches!(root.children[3], Child::For(_)));
        assert!(matches!(root.children[4], Child::If(_)));
        assert!(matches!(root.children[5], Child::Match(_)));
    }

    #[test]
    fn parses_leaf_root_and_struct_literals() {
        let leaf: ViewInput = syn::parse2(quote! { cx; Leaf::new(); }).unwrap();
        assert!(matches!(leaf.root, Root::Value { .. }));

        let parent: ViewInput = syn::parse2(quote! {
            cx; Root { label: "root" } => { Leaf { value: 1 }; }
        })
        .unwrap();
        let Root::Parent(root) = parent.root else {
            panic!("struct-literal root unexpectedly parsed as a value");
        };
        assert_eq!(root.children.len(), 1);
    }

    #[test]
    fn parenthesized_control_flow_is_a_value() {
        let input: ViewInput = syn::parse2(quote! {
            cx; Root::new() => {
                (if enabled { one() } else { two() });
                ({ match choice { Some(value) => value, None => fallback } });
            }
        })
        .unwrap();
        let Root::Parent(root) = input.root else {
            panic!("parent root unexpectedly parsed as a value");
        };
        assert!(
            root.children
                .iter()
                .all(|child| matches!(child, Child::Value { .. }))
        );
    }

    #[test]
    fn rejects_old_separator_with_migration_diagnostic() {
        let error = parse_error(quote! { cx, Root::new(); });
        assert!(error.to_string().contains("replace this `,` with `;`"));
    }

    #[test]
    fn rejects_a_second_root() {
        let error = parse_error(quote! {
            cx; Root::new() => {} Other::new();
        });
        assert!(error.to_string().contains("exactly one root"));
    }

    #[test]
    fn rejects_non_block_child_body_and_trailing_parent_semicolon() {
        let body_error = parse_error(quote! {
            cx; Root::new() => Leaf::new();
        });
        assert!(body_error.to_string().contains("braced child list"));

        let semicolon_error = parse_error(quote! {
            cx; Root::new() => {};
        });
        assert!(
            semicolon_error
                .to_string()
                .contains("do not take a trailing")
        );
    }
}
