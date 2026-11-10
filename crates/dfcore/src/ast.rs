//! Abstract syntax tree of a DeltaF Studio script.

use crate::lexer::Span;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Pow,
    And,
    Or,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CmpOp {
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Arg {
    /// Named argument (`P=...`) as written, if any.
    pub name: Option<String>,
    pub value: Expr,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Num {
        value: f64,
        unit: Option<String>,
        span: Span,
    },
    Str {
        value: String,
        span: Span,
    },
    /// `name` is canonical (lower case); `display` keeps the user spelling.
    Var {
        name: String,
        display: String,
        idx: Vec<Expr>,
        span: Span,
    },
    Neg(Box<Expr>, Span),
    Not(Box<Expr>, Span),
    Bin(BinOp, Box<Expr>, Box<Expr>, Span),
    Cmp(CmpOp, Box<Expr>, Box<Expr>, Span),
    Call {
        name: String,
        display: String,
        args: Vec<Arg>,
        span: Span,
    },
}

impl Expr {
    pub fn span(&self) -> Span {
        match self {
            Expr::Num { span, .. }
            | Expr::Str { span, .. }
            | Expr::Var { span, .. }
            | Expr::Neg(_, span)
            | Expr::Not(_, span)
            | Expr::Bin(_, _, _, span)
            | Expr::Cmp(_, _, _, span)
            | Expr::Call { span, .. } => *span,
        }
    }

    /// Visit every node depth-first.
    pub fn walk(&self, f: &mut impl FnMut(&Expr)) {
        f(self);
        match self {
            Expr::Var { idx, .. } => idx.iter().for_each(|e| e.walk(f)),
            Expr::Neg(a, _) | Expr::Not(a, _) => a.walk(f),
            Expr::Bin(_, a, b, _) | Expr::Cmp(_, a, b, _) => {
                a.walk(f);
                b.walk(f)
            }
            Expr::Call { args, .. } => args.iter().for_each(|a| a.value.walk(f)),
            _ => {}
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    /// `unit`: `a = b - c/d [bar]`, the unit of the variable the equation computes.
    Equation {
        lhs: Expr,
        rhs: Expr,
        span: Span,
        trailing_comment: Option<String>,
        unit: Option<String>,
    },
    Duplicate {
        var: String,
        from: Expr,
        to: Expr,
        body: Vec<Stmt>,
        span: Span,
    },
    /// `Call proc(in1, in2 : out1, out2)` used as equations.
    Call {
        name: String,
        inputs: Vec<Expr>,
        outputs: Vec<Expr>,
        span: Span,
    },
    Directive {
        name: String,
        args: String,
        span: Span,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum FStmt {
    Assign { target: String, idx: Vec<Expr>, value: Expr, span: Span },
    If { cond: Expr, then: Vec<FStmt>, els: Vec<FStmt>, span: Span },
    Repeat { body: Vec<FStmt>, until: Expr, span: Span },
    While { cond: Expr, body: Vec<FStmt>, span: Span },
    For { var: String, from: Expr, to: Expr, step: Option<Expr>, body: Vec<FStmt>, span: Span },
    Call { name: String, inputs: Vec<Expr>, outputs: Vec<(String, Vec<Expr>)>, span: Span },
    Return { span: Span },
}

#[derive(Debug, Clone, PartialEq)]
pub struct FuncDef {
    pub name: String,
    pub display: String,
    pub params: Vec<String>,
    /// Non-empty for procedures (`Procedure p(a, b : c, d)`).
    pub outputs: Vec<String>,
    pub is_procedure: bool,
    pub body: Vec<FStmt>,
    pub span: Span,
    pub doc: Option<String>,
}

/// `Module name(in1, in2 : out1)` ... `End`: a block of equations (EES).
#[derive(Debug, Clone, PartialEq)]
pub struct ModuleDef {
    pub name: String,
    pub display: String,
    pub params: Vec<String>,
    pub outputs: Vec<String>,
    pub body: Vec<Stmt>,
    pub span: Span,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Program {
    pub stmts: Vec<Stmt>,
    pub funcs: Vec<FuncDef>,
    /// `Module` / `Subprogram`: equations instantiated at each `Call`
    pub modules: Vec<ModuleDef>,
}
