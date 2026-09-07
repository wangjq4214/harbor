use crate::ast::{Child, ForChild, IfBranch, IfChild, MatchArm, MatchChild, Node, ViewInput};
use syn::parse::{Parse, ParseStream};
use syn::{Expr, Pat, Result, Token, braced, spanned::Spanned};

impl Parse for ViewInput {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let cx = input.call(Expr::parse_without_eager_brace)?;
        input.parse::<Token![,]>()?;
        let root = parse_node(input)?;

        if !input.is_empty() {
            return Err(input.error("view! accepts exactly one root node"));
        }

        Ok(Self { cx, root })
    }
}

fn parse_node(input: ParseStream<'_>) -> Result<Node> {
    let component = input.parse::<Expr>()?;
    input.parse::<Token![=>]>()?;
    let children = parse_child_block(input)?;

    Ok(Node {
        component,
        children,
    })
}

fn parse_child_block(input: ParseStream<'_>) -> Result<Vec<Child>> {
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
    if input.peek(syn::token::Brace) {
        return parse_interpolation(input);
    }
    if input.peek(Token![for]) {
        return parse_for(input).map(Child::For);
    }
    if input.peek(Token![if]) {
        return parse_if(input).map(Child::If);
    }
    if input.peek(Token![match]) {
        return parse_match(input).map(Child::Match);
    }

    parse_node(input).map(Child::Node)
}

fn parse_interpolation(input: ParseStream<'_>) -> Result<Child> {
    let content;
    braced!(content in input);
    if content.is_empty() {
        return Err(content.error("view! interpolation requires an expression"));
    }

    let expr = content.parse::<Expr>()?;
    if !content.is_empty() {
        return Err(content.error("view! interpolation accepts exactly one expression"));
    }
    let span = expr.span();
    Ok(Child::Interpolation { expr, span })
}

fn parse_for(input: ParseStream<'_>) -> Result<ForChild> {
    let for_token = input.parse::<Token![for]>()?;
    let pattern = input.call(Pat::parse_multi_with_leading_vert)?;
    input.parse::<Token![in]>()?;
    let iter = input.call(Expr::parse_without_eager_brace)?;
    let body = parse_child_block(input)?;

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
            else_body = Some(parse_child_block(input)?);
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
    let body = parse_child_block(input)?;

    Ok(IfBranch {
        if_token,
        condition,
        body,
    })
}

fn parse_match(input: ParseStream<'_>) -> Result<MatchChild> {
    let match_token = input.parse::<Token![match]>()?;
    let expr = input.call(Expr::parse_without_eager_brace)?;
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
        let body = parse_child_block(&content)?;
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

    #[test]
    fn parses_nested_nodes_interpolation_and_control_flow() {
        let input: ViewInput = syn::parse2(quote! {
            cx, Root::new() => {
                Leaf::new() => {}
                { existing }
                for item in items {
                    Row::new(item) => {}
                }
                if enabled {
                    Leaf::new() => {}
                } else if fallback {
                    Leaf::new() => {}
                } else {
                    Leaf::new() => {}
                }
                match choice {
                    Some(value) if value > 0 => { Row::new(value) => {} },
                    None => {}
                }
            }
        })
        .unwrap();

        assert_eq!(input.root.children.len(), 5);
        assert!(matches!(input.root.children[0], Child::Node(_)));
        assert!(matches!(
            input.root.children[1],
            Child::Interpolation { .. }
        ));
        assert!(matches!(input.root.children[2], Child::For(_)));
        assert!(matches!(input.root.children[3], Child::If(_)));
        assert!(matches!(input.root.children[4], Child::Match(_)));
    }

    #[test]
    fn parses_struct_literals_in_component_expressions() {
        let input: ViewInput = syn::parse2(quote! {
            cx, Root { label: "root" } => { Leaf { value: 1 } => {} }
        })
        .unwrap();

        assert_eq!(input.root.children.len(), 1);
    }

    #[test]
    fn rejects_a_second_root() {
        let error = match syn::parse2::<ViewInput>(quote! {
            cx, Root::new() => {} Other::new() => {}
        }) {
            Ok(_) => panic!("second root unexpectedly parsed"),
            Err(error) => error,
        };

        assert!(error.to_string().contains("exactly one root"));
    }

    #[test]
    fn rejects_non_block_child_body() {
        let error = match syn::parse2::<ViewInput>(quote! {
            cx, Root::new() => Leaf::new()
        }) {
            Ok(_) => panic!("non-block child body unexpectedly parsed"),
            Err(error) => error,
        };

        assert!(error.to_string().contains("expected curly braces"));
    }
}
