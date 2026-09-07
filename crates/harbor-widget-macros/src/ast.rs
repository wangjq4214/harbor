use proc_macro2::Span;
use syn::{Expr, Pat, Token, spanned::Spanned};

/// Parsed input to `view!(cx, component => { children })`.
pub struct ViewInput {
    pub cx: Expr,
    pub root: Node,
}

/// A component expression with a declarative child block.
pub struct Node {
    pub component: Expr,
    pub children: Vec<Child>,
}

impl Node {
    pub fn span(&self) -> Span {
        self.component.span()
    }
}

/// One item inside a node's child block.
pub enum Child {
    Node(Node),
    Interpolation { expr: Expr, span: Span },
    For(ForChild),
    If(IfChild),
    Match(MatchChild),
}

/// A `for pattern in expression { children }` child.
pub struct ForChild {
    pub for_token: Token![for],
    pub pattern: Pat,
    pub iter: Expr,
    pub body: Vec<Child>,
}

/// An `if` / `else if` / `else` child chain.
pub struct IfChild {
    pub branches: Vec<IfBranch>,
    pub else_body: Option<Vec<Child>>,
}

pub struct IfBranch {
    pub if_token: Token![if],
    pub condition: Expr,
    pub body: Vec<Child>,
}

/// A `match expression { pattern [if guard] => { children } }` child.
pub struct MatchChild {
    pub match_token: Token![match],
    pub expr: Expr,
    pub arms: Vec<MatchArm>,
}

pub struct MatchArm {
    pub pattern: Pat,
    pub guard: Option<(Token![if], Expr)>,
    pub body: Vec<Child>,
}
