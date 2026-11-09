//! Recursive-descent parser.
//!
//! Grammar sketch (case-insensitive keywords):
//! ```text
//! program   := { stmt | funcdef }
//! stmt      := expr '=' expr
//!            | 'Duplicate' ident '=' expr ',' expr NL { stmt } 'End'
//!            | 'Call' ident '(' exprs [':' exprs] ')'
//!            | '$' Directive
//! funcdef   := ('Function'|'Procedure') ident '(' params [':' params] ')' NL { fstmt } 'End'
//! fstmt     := ident [index] (':='|'=') expr
//!            | 'If' expr 'Then' ... ['Else' ...] 'EndIf' | single-line If
//!            | 'Repeat' ... 'Until' expr
//!            | 'While' expr ['Do'] ... 'End' | 'For' ident '=' expr 'To' expr ['Step' expr] ... 'End'
//!            | 'Call' ... | 'Return'
//! expr      := or ; or := and {'or' and} ; and := not {'and' not}
//! not       := 'not' not | cmp ; cmp := sum [cmpop sum]
//! sum       := term {('+'|'-') term} ; term := unary {('*'|'/') unary}
//! unary     := '-' unary | '+' unary | pow ; pow := atom ['^' unary]
//! atom      := number [unit] | string | ident ['[' exprs ']'] | ident '(' args ')' | '(' expr ')'
//! ```

use crate::ast::*;
use crate::lexer::{lex, Comment, Span, Tok, Token};

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Diagnostic {
    pub msg: String,
    pub span: Span,
    pub severity: Severity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum Severity {
    Error,
    Warning,
    Info,
}

pub struct ParseResult {
    pub program: Program,
    pub diagnostics: Vec<Diagnostic>,
    pub comments: Vec<Comment>,
}

struct Parser<'a> {
    toks: &'a [Token],
    pos: usize,
    diags: Vec<Diagnostic>,
    comments: &'a [Comment],
    /// Inside the condition of If / While / Until: `=` compares.
    cond: bool,
}

type PResult<T> = Result<T, Diagnostic>;

fn kw(t: &Tok, k: &str) -> bool {
    matches!(t, Tok::Ident(s) if s.eq_ignore_ascii_case(k))
}

pub fn canon(s: &str) -> String {
    s.to_lowercase()
}

const RESERVED: &[&str] = &[
    "duplicate",
    "end",
    "function",
    "procedure",
    "call",
    "if",
    "then",
    "else",
    "elseif",
    "endif",
    "repeat",
    "until",
    "while",
    "do",
    "for",
    "to",
    "step",
    "and",
    "or",
    "not",
    "return",
];

pub fn parse(src: &str) -> ParseResult {
    let lexed = lex(src);
    let mut p = Parser { toks: &lexed.tokens, pos: 0, diags: Vec::new(), comments: &lexed.comments, cond: false };
    for e in lexed.errors {
        p.diags.push(Diagnostic { msg: e.msg, span: e.span, severity: Severity::Error });
    }
    let mut prog = Program::default();
    loop {
        p.skip_newlines();
        if p.at(&Tok::Eof) {
            break;
        }
        let t = p.peek().clone();
        let res = if kw(&t, "function") || kw(&t, "procedure") { p.funcdef().map(|f| prog.funcs.push(f)) } else { p.stmt().map(|s| prog.stmts.push(s)) };
        if let Err(d) = res {
            p.diags.push(d);
            p.recover();
        }
    }
    ParseResult { program: prog, diagnostics: p.diags, comments: lexed.comments }
}

impl<'a> Parser<'a> {
    fn peek(&self) -> &Tok {
        &self.toks[self.pos].tok
    }
    fn peek_at(&self, k: usize) -> &Tok {
        &self.toks[(self.pos + k).min(self.toks.len() - 1)].tok
    }
    fn span(&self) -> Span {
        self.toks[self.pos].span
    }
    fn prev_span(&self) -> Span {
        self.toks[self.pos.saturating_sub(1)].span
    }
    fn at(&self, t: &Tok) -> bool {
        self.peek() == t
    }
    fn at_kw(&self, k: &str) -> bool {
        kw(self.peek(), k)
    }
    fn bump(&mut self) -> Token {
        let t = self.toks[self.pos].clone();
        if self.pos < self.toks.len() - 1 {
            self.pos += 1;
        }
        t
    }
    fn err<T>(&self, msg: impl Into<String>) -> PResult<T> {
        Err(Diagnostic { msg: msg.into(), span: self.span(), severity: Severity::Error })
    }
    fn expect(&mut self, t: Tok) -> PResult<Token> {
        if *self.peek() == t {
            Ok(self.bump())
        } else {
            self.err(format!("expected {t}, found {}", self.peek()))
        }
    }
    fn expect_kw(&mut self, k: &str) -> PResult<()> {
        if self.at_kw(k) {
            self.bump();
            Ok(())
        } else {
            self.err(format!("expected '{k}', found {}", self.peek()))
        }
    }
    fn ident(&mut self) -> PResult<(String, Span)> {
        match self.peek().clone() {
            Tok::Ident(s) => {
                let sp = self.bump().span;
                Ok((s, sp))
            }
            other => self.err(format!("expected identifier, found {other}")),
        }
    }
    fn skip_newlines(&mut self) {
        while matches!(self.peek(), Tok::Newline | Tok::Semicolon) {
            self.bump();
        }
    }
    fn end_of_stmt(&mut self) -> PResult<()> {
        match self.peek() {
            Tok::Newline | Tok::Semicolon => {
                self.bump();
                Ok(())
            }
            Tok::Eof => Ok(()),
            t => self.err(format!("unexpected {t} after statement")),
        }
    }
    fn recover(&mut self) {
        while !matches!(self.peek(), Tok::Newline | Tok::Semicolon | Tok::Eof) {
            self.bump();
        }
    }

    fn trailing_comment(&self, end: Span) -> Option<String> {
        self.comments.iter().find(|c| c.span.line == self.line_of_end(end) && c.span.start >= end.end).map(|c| c.text.trim().to_string())
    }
    fn line_of_end(&self, s: Span) -> u32 {
        // spans record the starting line; equations are single-line in practice
        s.line
    }

    // ---------------------------------------------------------------- statements
    fn stmt(&mut self) -> PResult<Stmt> {
        let start = self.span();
        match self.peek().clone() {
            Tok::Directive(name, args) => {
                self.bump();
                Ok(Stmt::Directive { name: canon(&name), args, span: start })
            }
            t if kw(&t, "duplicate") => {
                self.bump();
                let (var, _) = self.ident()?;
                self.expect(Tok::Eq)?;
                let from = self.expr()?;
                if !matches!(self.peek(), Tok::Comma | Tok::Colon) && !self.at_kw("to") {
                    return self.err("expected ',' in Duplicate range");
                }
                self.bump();
                let to = self.expr()?;
                self.end_of_stmt()?;
                let mut body = Vec::new();
                loop {
                    self.skip_newlines();
                    if self.at_kw("end") {
                        self.bump();
                        break;
                    }
                    if self.at(&Tok::Eof) {
                        return Err(Diagnostic { msg: "Duplicate without matching End".into(), span: start, severity: Severity::Error });
                    }
                    match self.stmt() {
                        Ok(s) => body.push(s),
                        Err(d) => {
                            self.diags.push(d);
                            self.recover();
                        }
                    }
                }
                let span = start.to(self.prev_span());
                Ok(Stmt::Duplicate { var: canon(&var), from, to, body, span })
            }
            t if kw(&t, "call") => {
                self.bump();
                let (name, _) = self.ident()?;
                let (inputs, outputs) = self.call_args()?;
                let span = start.to(self.prev_span());
                self.end_of_stmt()?;
                Ok(Stmt::Call { name: canon(&name), inputs, outputs, span })
            }
            _ => {
                let lhs = self.expr()?;
                if !self.at(&Tok::Eq) && !self.at(&Tok::Assign) {
                    return self.err(format!("expected '=' in equation, found {}", self.peek()));
                }
                self.bump();
                let rhs = self.expr()?;
                let unit = if let Tok::Unit(u) = self.peek().clone() {
                    self.bump();
                    Some(u)
                } else {
                    None
                };
                let span = start.to(self.prev_span());
                let trailing_comment = self.trailing_comment(span);
                self.end_of_stmt()?;
                Ok(Stmt::Equation { lhs, rhs, span, trailing_comment, unit })
            }
        }
    }

    /// `( in1, in2 : out1, out2 )`
    fn call_args(&mut self) -> PResult<(Vec<Expr>, Vec<Expr>)> {
        self.expect(Tok::LParen)?;
        let mut ins = Vec::new();
        let mut outs = Vec::new();
        let mut in_outputs = false;
        if !self.at(&Tok::RParen) {
            loop {
                let e = self.expr()?;
                if in_outputs {
                    outs.push(e)
                } else {
                    ins.push(e)
                }
                match self.peek() {
                    Tok::Comma => {
                        self.bump();
                    }
                    Tok::Colon if !in_outputs => {
                        self.bump();
                        in_outputs = true;
                    }
                    _ => break,
                }
            }
        }
        self.expect(Tok::RParen)?;
        Ok((ins, outs))
    }

    fn funcdef(&mut self) -> PResult<FuncDef> {
        let start = self.span();
        let is_procedure = self.at_kw("procedure");
        self.bump();
        let (display, _) = self.ident()?;
        self.expect(Tok::LParen)?;
        let mut params = Vec::new();
        let mut outputs = Vec::new();
        let mut in_out = false;
        if !self.at(&Tok::RParen) {
            loop {
                let (p, _) = self.ident()?;
                if in_out {
                    outputs.push(canon(&p))
                } else {
                    params.push(canon(&p))
                }
                match self.peek() {
                    Tok::Comma => {
                        self.bump();
                    }
                    Tok::Colon if !in_out => {
                        self.bump();
                        in_out = true;
                    }
                    _ => break,
                }
            }
        }
        self.expect(Tok::RParen)?;
        let doc = self.trailing_comment(start.to(self.prev_span()));
        self.end_of_stmt()?;
        let body = self.fblock(&["end"])?;
        self.expect_kw("end")?;
        let span = start.to(self.prev_span());
        if is_procedure && outputs.is_empty() {
            self.diags.push(Diagnostic { msg: "procedure has no outputs (use ':' to separate inputs from outputs)".into(), span, severity: Severity::Warning });
        }
        Ok(FuncDef { name: canon(&display), display, params, outputs, is_procedure, body, span, doc })
    }

    /// Parse function statements until one of `terms` keywords (not consumed).
    fn fblock(&mut self, terms: &[&str]) -> PResult<Vec<FStmt>> {
        let mut out = Vec::new();
        loop {
            self.skip_newlines();
            if terms.iter().any(|k| self.at_kw(k)) {
                return Ok(out);
            }
            if self.at(&Tok::Eof) {
                return self.err(format!("expected '{}' before end of input", terms[0]));
            }
            match self.fstmt() {
                Ok(s) => out.push(s),
                Err(d) => {
                    self.diags.push(d);
                    self.recover();
                }
            }
        }
    }

    fn fstmt(&mut self) -> PResult<FStmt> {
        let start = self.span();
        let s = self.fstmt_inner()?;
        let _ = start;
        Ok(s)
    }

    fn fstmt_inner(&mut self) -> PResult<FStmt> {
        let start = self.span();
        if self.at_kw("if") {
            self.bump();
            let cond = self.cond_expr()?;
            self.expect_kw("then")?;
            if !matches!(self.peek(), Tok::Newline | Tok::Semicolon) {
                // single-line if: If c Then stmt [Else stmt]
                let then = vec![self.simple_fstmt()?];
                let els = if self.at_kw("else") {
                    self.bump();
                    vec![self.simple_fstmt()?]
                } else {
                    vec![]
                };
                return Ok(FStmt::If { cond, then, els, span: start.to(self.prev_span()) });
            }
            let then = self.fblock(&["endif", "else", "elseif", "end"])?;
            let els = self.if_tail()?;
            return Ok(FStmt::If { cond, then, els, span: start.to(self.prev_span()) });
        }
        if self.at_kw("repeat") {
            self.bump();
            let body = self.fblock(&["until"])?;
            self.bump();
            let until = self.cond_expr()?;
            return Ok(FStmt::Repeat { body, until, span: start.to(self.prev_span()) });
        }
        if self.at_kw("while") {
            self.bump();
            let cond = self.cond_expr()?;
            if self.at_kw("do") {
                self.bump();
            }
            let body = self.fblock(&["end"])?;
            self.bump();
            return Ok(FStmt::While { cond, body, span: start.to(self.prev_span()) });
        }
        if self.at_kw("for") {
            self.bump();
            let (v, _) = self.ident()?;
            self.expect(Tok::Eq)?;
            let from = self.expr()?;
            if self.at_kw("to") || self.at(&Tok::Comma) {
                self.bump();
            } else {
                return self.err("expected 'to' in For loop");
            }
            let to = self.expr()?;
            let step = if self.at_kw("step") {
                self.bump();
                Some(self.expr()?)
            } else {
                None
            };
            let body = self.fblock(&["end"])?;
            self.bump();
            return Ok(FStmt::For { var: canon(&v), from, to, step, body, span: start.to(self.prev_span()) });
        }
        self.simple_fstmt()
    }

    fn if_tail(&mut self) -> PResult<Vec<FStmt>> {
        if self.at_kw("endif") {
            self.bump();
            return Ok(vec![]);
        }
        if self.at_kw("end") {
            // allow "End If" / "End"
            self.bump();
            if self.at_kw("if") {
                self.bump();
            }
            return Ok(vec![]);
        }
        if self.at_kw("elseif") {
            let start = self.span();
            self.bump();
            let cond = self.cond_expr()?;
            self.expect_kw("then")?;
            let then = self.fblock(&["endif", "else", "elseif", "end"])?;
            let els = self.if_tail()?;
            return Ok(vec![FStmt::If { cond, then, els, span: start.to(self.prev_span()) }]);
        }
        // else
        self.bump();
        if self.at_kw("if") {
            // "Else If" written as two words
            let start = self.span();
            self.bump();
            let cond = self.cond_expr()?;
            self.expect_kw("then")?;
            let then = self.fblock(&["endif", "else", "elseif", "end"])?;
            let els = self.if_tail()?;
            return Ok(vec![FStmt::If { cond, then, els, span: start.to(self.prev_span()) }]);
        }
        let els = self.fblock(&["endif", "end"])?;
        if self.at_kw("end") {
            self.bump();
            if self.at_kw("if") {
                self.bump();
            }
        } else {
            self.bump();
        }
        Ok(els)
    }

    fn simple_fstmt(&mut self) -> PResult<FStmt> {
        let start = self.span();
        if self.at_kw("return") {
            self.bump();
            return Ok(FStmt::Return { span: start });
        }
        if self.at_kw("call") {
            self.bump();
            let (name, _) = self.ident()?;
            let (inputs, outs) = self.call_args()?;
            let mut outputs = Vec::new();
            for o in outs {
                match o {
                    Expr::Var { name, idx, .. } => outputs.push((name, idx)),
                    other => return Err(Diagnostic { msg: "procedure outputs must be variables".into(), span: other.span(), severity: Severity::Error }),
                }
            }
            return Ok(FStmt::Call { name: canon(&name), inputs, outputs, span: start.to(self.prev_span()) });
        }
        let (name, _) = self.ident()?;
        let mut idx = Vec::new();
        if self.at(&Tok::LBracket) {
            self.bump();
            idx = self.expr_list(Tok::RBracket)?;
        }
        if !self.at(&Tok::Assign) && !self.at(&Tok::Eq) {
            return self.err(format!("expected ':=' in assignment, found {}", self.peek()));
        }
        self.bump();
        let value = self.expr()?;
        Ok(FStmt::Assign { target: canon(&name), idx, value, span: start.to(self.prev_span()) })
    }

    // ---------------------------------------------------------------- expressions
    pub fn expr(&mut self) -> PResult<Expr> {
        self.or_expr()
    }
    /// Condition of If / ElseIf / While / Until, where `=` is a comparison (EES).
    fn cond_expr(&mut self) -> PResult<Expr> {
        let was = std::mem::replace(&mut self.cond, true);
        let r = self.expr();
        self.cond = was;
        r
    }
    fn or_expr(&mut self) -> PResult<Expr> {
        let mut a = self.and_expr()?;
        while self.at_kw("or") {
            self.bump();
            let b = self.and_expr()?;
            let sp = a.span().to(b.span());
            a = Expr::Bin(BinOp::Or, Box::new(a), Box::new(b), sp);
        }
        Ok(a)
    }
    fn and_expr(&mut self) -> PResult<Expr> {
        let mut a = self.not_expr()?;
        while self.at_kw("and") {
            self.bump();
            let b = self.not_expr()?;
            let sp = a.span().to(b.span());
            a = Expr::Bin(BinOp::And, Box::new(a), Box::new(b), sp);
        }
        Ok(a)
    }
    fn not_expr(&mut self) -> PResult<Expr> {
        if self.at_kw("not") {
            let sp = self.bump().span;
            let a = self.not_expr()?;
            let s = sp.to(a.span());
            return Ok(Expr::Not(Box::new(a), s));
        }
        self.cmp_expr()
    }
    fn cmp_expr(&mut self) -> PResult<Expr> {
        let a = self.sum()?;
        let op = match self.peek() {
            Tok::Lt => CmpOp::Lt,
            Tok::Le => CmpOp::Le,
            Tok::Gt => CmpOp::Gt,
            Tok::Ge => CmpOp::Ge,
            Tok::Ne => CmpOp::Ne,
            Tok::Eq if self.cond => CmpOp::Eq,
            _ => return Ok(a),
        };
        self.bump();
        let b = self.sum()?;
        let sp = a.span().to(b.span());
        Ok(Expr::Cmp(op, Box::new(a), Box::new(b), sp))
    }
    fn sum(&mut self) -> PResult<Expr> {
        let mut a = self.term()?;
        loop {
            let op = match self.peek() {
                Tok::Plus => BinOp::Add,
                Tok::Minus => BinOp::Sub,
                _ => return Ok(a),
            };
            self.bump();
            let b = self.term()?;
            let sp = a.span().to(b.span());
            a = Expr::Bin(op, Box::new(a), Box::new(b), sp);
        }
    }
    fn term(&mut self) -> PResult<Expr> {
        let mut a = self.unary()?;
        loop {
            let op = match self.peek() {
                Tok::Star => BinOp::Mul,
                Tok::Slash => BinOp::Div,
                _ => return Ok(a),
            };
            self.bump();
            let b = self.unary()?;
            let sp = a.span().to(b.span());
            a = Expr::Bin(op, Box::new(a), Box::new(b), sp);
        }
    }
    fn unary(&mut self) -> PResult<Expr> {
        match self.peek() {
            Tok::Minus => {
                let sp = self.bump().span;
                let a = self.unary()?;
                let s = sp.to(a.span());
                Ok(Expr::Neg(Box::new(a), s))
            }
            Tok::Plus => {
                self.bump();
                self.unary()
            }
            _ => self.pow(),
        }
    }
    fn pow(&mut self) -> PResult<Expr> {
        let a = self.atom()?;
        if self.at(&Tok::Caret) {
            self.bump();
            let b = self.unary()?; // right associative, allows 2^-1
            let sp = a.span().to(b.span());
            return Ok(Expr::Bin(BinOp::Pow, Box::new(a), Box::new(b), sp));
        }
        Ok(a)
    }
    fn expr_list(&mut self, close: Tok) -> PResult<Vec<Expr>> {
        let mut v = Vec::new();
        if *self.peek() != close {
            loop {
                v.push(self.expr()?);
                if self.at(&Tok::Comma) {
                    self.bump();
                } else {
                    break;
                }
            }
        }
        self.expect(close)?;
        Ok(v)
    }
    fn atom(&mut self) -> PResult<Expr> {
        let sp = self.span();
        match self.peek().clone() {
            Tok::Num(v) => {
                self.bump();
                let unit = if let Tok::Unit(u) = self.peek().clone() {
                    self.bump();
                    Some(u)
                } else {
                    None
                };
                Ok(Expr::Num { value: v, unit, span: sp.to(self.prev_span()) })
            }
            Tok::Str(s) => {
                self.bump();
                Ok(Expr::Str { value: s, span: sp })
            }
            Tok::LParen => {
                self.bump();
                let e = self.expr()?;
                self.expect(Tok::RParen)?;
                Ok(e)
            }
            Tok::Ident(name) => {
                // `if(` inside an expression is the EES function IF(A, B, X, Y, Z)
                let if_call = canon(&name) == "if" && *self.peek_at(1) == Tok::LParen;
                if RESERVED.contains(&canon(&name).as_str()) && !if_call {
                    return self.err(format!("unexpected keyword '{name}'"));
                }
                self.bump();
                if self.at(&Tok::LParen) {
                    self.bump();
                    let mut args = Vec::new();
                    if !self.at(&Tok::RParen) {
                        loop {
                            // named arg?  ident '=' expr   (but not '==')
                            let named = matches!(self.peek(), Tok::Ident(_)) && *self.peek_at(1) == Tok::Eq;
                            if named {
                                let (n, _) = self.ident()?;
                                self.bump();
                                let value = self.expr()?;
                                args.push(Arg { name: Some(n), value });
                            } else {
                                let value = self.expr()?;
                                args.push(Arg { name: None, value });
                            }
                            if self.at(&Tok::Comma) || self.at(&Tok::Semicolon) {
                                self.bump();
                            } else {
                                break;
                            }
                        }
                    }
                    self.expect(Tok::RParen)?;
                    return Ok(Expr::Call { name: canon(&name), display: name, args, span: sp.to(self.prev_span()) });
                }
                let mut idx = Vec::new();
                if self.at(&Tok::LBracket) {
                    self.bump();
                    idx = self.expr_list(Tok::RBracket)?;
                }
                Ok(Expr::Var { name: canon(&name), display: name, idx, span: sp.to(self.prev_span()) })
            }
            other => self.err(format!("expected expression, found {other}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parse_basic() {
        let r = parse("x + 2*y^2 = 3 [bar]\nh[1] = enthalpy(CO2, P=P[1], T=T[1]) {kJ/kg}\n");
        assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
        assert_eq!(r.program.stmts.len(), 2);
        if let Stmt::Equation { trailing_comment, .. } = &r.program.stmts[1] {
            assert_eq!(trailing_comment.as_deref(), Some("kJ/kg"));
        } else {
            panic!()
        }
    }
    #[test]
    fn equals_compares_in_conditions() {
        let r = parse("Function f(x)\n  If x = 1 Then\n    f := 2\n  Else\n    f := 3\n  EndIf\n  Repeat\n    x := x + 1\n  Until (x = 5) or not (x <> 9)\nEnd\ny = f(1)\n");
        assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
        let FStmt::If { cond, .. } = &r.program.funcs[0].body[0] else { panic!() };
        assert!(matches!(cond, Expr::Cmp(CmpOp::Eq, ..)));
    }
    #[test]
    fn parse_duplicate_and_function() {
        let src = "Duplicate j=1,k\n x[j] = j\nEnd\nFunction f(a, b)\n If a > b Then\n f := a\n Else\n f := b\n EndIf\nEnd\ny = f(1,2)";
        let r = parse(src);
        assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
        assert_eq!(r.program.funcs.len(), 1);
        assert_eq!(r.program.stmts.len(), 2);
    }
    #[test]
    fn ees_if_function_in_expression() {
        let r = parse("y = IF(a, b, 1, 2, 3)\nFunction f(x)\n  If x > 0 Then f := if(x, 1, 0, 0, 1)\nEnd");
        assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
        assert_eq!(r.program.stmts.len(), 1);
    }
    #[test]
    fn error_recovery() {
        let r = parse("x = = 2\ny = 3");
        assert_eq!(r.diagnostics.len(), 1);
        assert_eq!(r.program.stmts.len(), 1);
    }
}
