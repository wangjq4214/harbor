use proc_macro2::Span;
use syn::{Expr, Pat, Token, spanned::Spanned};

/// Parsed input to `view! { cx; root }`.
pub struct ViewInput {
    pub cx: Expr,
    pub root: Root,
}

/// The single root of a declarative view tree.
pub enum Root {
    /// A component built directly without attaching children.
    Value { expr: Expr, span: Span },
    /// A component that receives a declarative child list before it is built.
    Parent(Node),
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
    /// One ordinary Rust expression converted through `IntoChildView`.
    Value {
        expr: Expr,
        span: Span,
    },
    /// A component expression that receives a child list through `WithChildren`.
    Parent(Node),
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
