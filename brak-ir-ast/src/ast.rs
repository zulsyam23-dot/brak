use std::fmt;

use brak_core::{ContentHash, combine_hash, Span};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Program {
    pub items: Vec<Item>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Item {
    FnDef(FnDef),
    ExternFn(ExternFn),
    Let(Let),
    Struct(StructDef),
    Enum(EnumDef),
    Trait(TraitDef),
    Impl(ImplDef),
    Use(UseStmt),
    Mod(ModDef),
    Const(ConstDef),
    Static(StaticDef),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructDef {
    pub vis: Visibility,
    pub name: Ident,
    pub fields: Vec<Field>,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Field {
    pub vis: Visibility,
    pub name: Ident,
    pub ty: Type,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnumDef {
    pub vis: Visibility,
    pub name: Ident,
    pub variants: Vec<Variant>,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Variant {
    pub name: Ident,
    pub fields: Option<Vec<Type>>, // Tuple variants for now
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraitDef {
    pub vis: Visibility,
    pub name: Ident,
    pub methods: Vec<FnDef>,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImplDef {
    pub trait_name: Option<Ident>,
    pub target_ty: Type,
    pub methods: Vec<FnDef>,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UseStmt {
    pub path: Vec<Ident>,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModDef {
    pub name: Ident,
    pub items: Option<Vec<Item>>, // None means external file
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConstDef {
    pub vis: Visibility,
    pub name: Ident,
    pub ty: Type,
    pub value: Expr,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StaticDef {
    pub vis: Visibility,
    pub name: Ident,
    pub ty: Type,
    pub value: Expr,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Visibility {
    Public,
    Private,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternFn {
    pub name: Ident,
    pub params: Vec<Param>,
    pub ret_ty: Option<Type>,
    pub abi: String, // e.g. "C", "system"
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FnDef {
    pub name: Ident,
    pub params: Vec<Param>,
    pub ret_ty: Option<Type>,
    pub body: Block,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Param {
    pub name: Ident,
    pub ty: Option<Type>,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Let {
    pub name: Ident,
    pub ty: Option<Type>,
    pub value: Option<Box<Expr>>,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
    pub stmts: Vec<Stmt>,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Stmt {
    Let(Let),
    Expr(Expr),
    Return(Option<Expr>, Span),
    Break(Span),
    Continue(Span),
    If {
        cond: Box<Expr>,
        then: Block,
        else_: Option<Block>,
        span: Span,
    },
    While {
        cond: Box<Expr>,
        body: Block,
        span: Span,
    },
    Loop {
        body: Block,
        span: Span,
    },
    For {
        var: Ident,
        iterable: Box<Expr>,
        body: Block,
        span: Span,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Expr {
    Int(i64, Span),
    Float(f64, Span),
    Bool(bool, Span),
    String(String, Span),
    Ident(Ident),
    Assign(Box<Expr>, Box<Expr>, Span),
    BinOp {
        op: BinOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
        span: Span,
    },
    UnOp {
        op: UnOp,
        expr: Box<Expr>,
        span: Span,
    },
    Call {
        callee: Box<Expr>,
        args: Vec<Expr>,
        span: Span,
    },
    If {
        cond: Box<Expr>,
        then: Box<Expr>,
        else_: Box<Expr>,
        span: Span,
    },
    Match {
        expr: Box<Expr>,
        arms: Vec<MatchArm>,
        span: Span,
    },
    Field {
        object: Box<Expr>,
        field: Ident,
        span: Span,
    },
    Block(Block),
    StructInit {
        name: Ident,
        fields: Vec<(Ident, Expr)>,
        span: Span,
    },
    /// Enum construction: `EnumName.Variant()` (Fase 7, fieldless variants).
    EnumCons {
        enum_name: Ident,
        variant: Ident,
        args: Vec<Expr>,
        span: Span,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub body: Expr,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Pattern {
    Ident(Ident),
    Literal(Expr),
    Wildcard(Span),
    /// `EnumName.Variant` — matches that exact variant (fieldless enums).
    Variant {
        enum_name: Ident,
        variant: Ident,
        span: Span,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BinOp {
    Add, Sub, Mul, Div, Mod,
    Eq, Ne, Lt, Le, Gt, Ge,
    And, Or,
    BitAnd, BitOr, BitXor,
    Shl, Shr,
    Range,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnOp {
    Neg,
    Not,
    BitNot,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ident {
    pub name: String,
    pub span: Span,
}

impl Ident {
    pub fn new(name: impl Into<String>, span: Span) -> Self {
        Self { name: name.into(), span }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Type {
    I32,
    I64,
    F32,
    F64,
    Bool,
    String,
    Void,
    Named(String),
    Ptr(Box<Type>),
    Ref(Box<Type>),
    Array(Box<Type>, usize),
    Slice(Box<Type>),
    Fn(Vec<Type>, Box<Type>),
}

impl ContentHash for Program {
    fn content_hash(&self) -> u64 {
        let mut h = 0u64;
        for item in &self.items {
            h = combine_hash(h, item.content_hash());
        }
        h
    }
}

impl ContentHash for Item {
    fn content_hash(&self) -> u64 {
        match self {
            Item::FnDef(f) => f.content_hash(),
            Item::ExternFn(e) => e.content_hash(),
            Item::Let(l) => l.content_hash(),
            Item::Struct(s) => s.name.content_hash(),
            Item::Enum(e) => e.name.content_hash(),
            Item::Trait(t) => t.name.content_hash(),
            Item::Impl(i) => i.target_ty.content_hash(),
            Item::Use(u) => u.path.len() as u64,
            Item::Mod(m) => m.name.content_hash(),
            Item::Const(c) => c.name.content_hash(),
            Item::Static(s) => s.name.content_hash(),
        }
    }
}

impl ContentHash for Type {
    fn content_hash(&self) -> u64 {
        match self {
            Type::I32 => 1,
            Type::I64 => 2,
            Type::F32 => 3,
            Type::F64 => 4,
            Type::Bool => 5,
            Type::String => 6,
            Type::Void => 7,
            Type::Named(s) => s.content_hash(),
            Type::Ptr(t) => combine_hash(8, t.content_hash()),
            Type::Ref(t) => combine_hash(9, t.content_hash()),
            Type::Array(t, len) => combine_hash(combine_hash(10, t.content_hash()), *len as u64),
            Type::Slice(t) => combine_hash(11, t.content_hash()),
            Type::Fn(args, ret) => {
                let mut h = 12;
                for a in args { h = combine_hash(h, a.content_hash()); }
                combine_hash(h, ret.content_hash())
            }
        }
    }
}

impl ContentHash for ExternFn {
    fn content_hash(&self) -> u64 {
        let mut h = self.name.content_hash();
        h = combine_hash(h, self.abi.content_hash());
        h
    }
}

impl ContentHash for FnDef {
    fn content_hash(&self) -> u64 {
        let mut h = self.name.content_hash();
        h = combine_hash(h, self.body.content_hash());
        h
    }
}

impl ContentHash for Block {
    fn content_hash(&self) -> u64 {
        let mut h = 0u64;
        for s in &self.stmts {
            h = combine_hash(h, s.content_hash());
        }
        h
    }
}

impl ContentHash for Stmt {
    fn content_hash(&self) -> u64 {
        match self {
            Stmt::Let(l) => l.content_hash(),
            Stmt::Expr(e) => e.content_hash(),
            Stmt::Return(_, _) => 1,
            Stmt::Break(_) => 2,
            Stmt::Continue(_) => 3,
            Stmt::If { cond, then, else_, .. } => {
                let mut h = cond.content_hash();
                h = combine_hash(h, then.content_hash());
                if let Some(b) = else_ {
                    h = combine_hash(h, b.content_hash());
                }
                h
            }
            Stmt::While { cond, body, .. } => {
                combine_hash(cond.content_hash(), body.content_hash())
            }
            Stmt::Loop { body, .. } => body.content_hash(),
            Stmt::For { var, iterable, body, .. } => {
                let mut h = var.content_hash();
                h = combine_hash(h, iterable.content_hash());
                combine_hash(h, body.content_hash())
            }
        }
    }
}

impl ContentHash for Expr {
    fn content_hash(&self) -> u64 {
        match self {
            Expr::Int(i, _) => *i as u64,
            Expr::Float(f, _) => f.to_bits(),
            Expr::Bool(b, _) => *b as u64,
            Expr::String(s, _) => s.content_hash(),
            Expr::Ident(id) => id.content_hash(),
            Expr::Assign(lhs, rhs, _) => {
                combine_hash(lhs.content_hash(), rhs.content_hash())
            }
            Expr::BinOp { op, lhs, rhs, .. } => {
                let mut h = *op as u64;
                h = combine_hash(h, lhs.content_hash());
                h = combine_hash(h, rhs.content_hash());
                h
            }
            Expr::UnOp { op, expr, .. } => {
                combine_hash(*op as u64, expr.content_hash())
            }
            Expr::Call { callee, .. } => callee.content_hash(),
            Expr::If { cond, then, else_, .. } => {
                let mut h = cond.content_hash();
                h = combine_hash(h, then.content_hash());
                h = combine_hash(h, else_.content_hash());
                h
            }
            Expr::Match { expr, .. } => expr.content_hash(),
            Expr::Field { object, field, .. } => combine_hash(object.content_hash(), field.content_hash()),
            Expr::Block(b) => b.content_hash(),
            Expr::StructInit { name, fields, .. } => {
                let mut h = name.content_hash();
                for (fname, fexpr) in fields {
                    h = combine_hash(h, fname.content_hash());
                    h = combine_hash(h, fexpr.content_hash());
                }
                h
            }
            Expr::EnumCons { enum_name, variant, args, .. } => {
                let mut h = combine_hash(enum_name.content_hash(), variant.content_hash());
                for a in args {
                    h = combine_hash(h, a.content_hash());
                }
                h
            }
        }
    }
}

impl Expr {
    pub fn span(&self) -> Span {
        match self {
            Expr::Int(_, s) | Expr::Float(_, s) | Expr::Bool(_, s) | Expr::String(_, s) => *s,
            Expr::Ident(id) => id.span,
            Expr::Assign(_, _, span) | Expr::BinOp { span, .. } | Expr::UnOp { span, .. }
            | Expr::Call { span, .. } | Expr::If { span, .. } => *span,
            Expr::Match { span, .. } => *span,
            Expr::Field { span, .. } => *span,
            Expr::Block(b) => b.span,
            Expr::StructInit { span, .. } => *span,
            Expr::EnumCons { span, .. } => *span,
        }
    }
}

impl ContentHash for Ident {
    fn content_hash(&self) -> u64 {
        self.name.content_hash()
    }
}

impl ContentHash for Let {
    fn content_hash(&self) -> u64 {
        self.name.content_hash()
    }
}

impl fmt::Display for Program {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for item in &self.items {
            writeln!(f, "{item}")?;
        }
        Ok(())
    }
}

impl fmt::Display for Item {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Item::FnDef(fd) => write!(f, "{fd}"),
            Item::ExternFn(e) => write!(f, "{e}"),
            Item::Let(l) => write!(f, "{l}"),
            Item::Struct(s) => {
                write!(f, "struct {}(", s.name.name)?;
                for (i, field) in s.fields.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{}: {}", field.name.name, field.ty)?;
                }
                write!(f, ")")
            }
            Item::Enum(e) => write!(f, "enum {}", e.name.name),
            Item::Trait(t) => write!(f, "trait {}", t.name.name),
            Item::Impl(i) => write!(f, "impl for {}", i.target_ty),
            Item::Use(_) => write!(f, "use ..."),
            Item::Mod(m) => write!(f, "mod {}", m.name.name),
            Item::Const(c) => write!(f, "const {}: {} = ...", c.name.name, c.ty),
            Item::Static(s) => write!(f, "static {}: {} = ...", s.name.name, s.ty),
        }
    }
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Type::I32 => write!(f, "i32"),
            Type::I64 => write!(f, "i64"),
            Type::F32 => write!(f, "f32"),
            Type::F64 => write!(f, "f64"),
            Type::Bool => write!(f, "bool"),
            Type::String => write!(f, "string"),
            Type::Void => write!(f, "void"),
            Type::Named(s) => write!(f, "{s}"),
            Type::Ptr(t) => write!(f, "*{t}"),
            Type::Ref(t) => write!(f, "&{t}"),
            Type::Array(t, n) => write!(f, "[{t}; {n}]"),
            Type::Slice(t) => write!(f, "[{t}]"),
            Type::Fn(args, ret) => {
                write!(f, "fn(")?;
                for (i, a) in args.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{a}")?;
                }
                write!(f, ") -> {ret}")
            }
        }
    }
}

impl fmt::Display for ExternFn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "extern \"{}\" fn {}(", self.abi, self.name.name)?;
        for (i, p) in self.params.iter().enumerate() {
            if i > 0 { write!(f, ", ")?; }
            write!(f, "{}", p.name.name)?;
        }
        write!(f, ");")
    }
}

impl fmt::Display for FnDef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "fn {}(", self.name.name)?;
        for (i, p) in self.params.iter().enumerate() {
            if i > 0 { write!(f, ", ")?; }
            write!(f, "{}", p.name.name)?;
        }
        write!(f, ")")?;
        if let Some(ty) = &self.ret_ty {
            write!(f, " -> {ty}")?;
        }
        write!(f, " {}", self.body)
    }
}

impl fmt::Display for Block {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "{{")?;
        for stmt in &self.stmts {
            writeln!(f, "    {stmt}")?;
        }
        write!(f, "}}")
    }
}

impl fmt::Display for Let {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "let {}", self.name.name)?;
        if let Some(ty) = &self.ty {
            write!(f, ": {ty}")?;
        }
        if let Some(val) = &self.value {
            write!(f, " = {val}")?;
        }
        write!(f, ";")
    }
}

impl fmt::Display for Stmt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Stmt::Let(l) => write!(f, "let {} = {};", l.name.name, l.value.as_ref().map(|v| v.to_string()).unwrap_or_default()),
            Stmt::Expr(e) => write!(f, "{e};"),
            Stmt::Return(None, _) => write!(f, "return;"),
            Stmt::Return(Some(e), _) => write!(f, "return {e};"),
            Stmt::If { cond, then, else_, .. } => {
                write!(f, "if {cond} {then}")?;
                if let Some(b) = else_ {
                    write!(f, " else {b}")?;
                }
                Ok(())
            }
            Stmt::While { cond, body, .. } => {
                write!(f, "while {cond} {body}")
            }
            Stmt::Break(_) => write!(f, "break;"),
            Stmt::Continue(_) => write!(f, "continue;"),
            Stmt::Loop { body, .. } => write!(f, "loop {body}"),
            Stmt::For { var, iterable, body, .. } => {
                write!(f, "for {} in {iterable} {body}", var.name)
            }
        }
    }
}

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Expr::Int(i, _) => write!(f, "{i}"),
            Expr::Float(fl, _) => write!(f, "{fl}"),
            Expr::Bool(b, _) => write!(f, "{b}"),
            Expr::String(s, _) => write!(f, "\"{s}\""),
            Expr::Ident(id) => write!(f, "{}", id.name),
            Expr::Assign(lhs, rhs, _) => write!(f, "({lhs} = {rhs})"),
            Expr::BinOp { op, lhs, rhs, .. } => write!(f, "({lhs} {op} {rhs})"),
            Expr::UnOp { op, expr, .. } => write!(f, "{op}{expr}"),
            Expr::Call { callee, args, .. } => {
                write!(f, "{callee}(")?;
                for (i, a) in args.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{a}")?;
                }
                write!(f, ")")
            }
            Expr::If { cond, then, else_, .. } => {
                write!(f, "if {cond} {then} else {else_}")
            }
            Expr::Match { expr, arms, .. } => {
                writeln!(f, "match {expr} {{")?;
                for arm in arms {
                    write!(f, "    {} => {}", arm.pattern, arm.body)?;
                }
                write!(f, "}}")
            }
            Expr::Field { object, field, .. } => write!(f, "{object}.{}", field.name),
            Expr::Block(b) => write!(f, "{b}"),
            Expr::StructInit { name, fields, .. } => {
                write!(f, "{} {{ ", name.name)?;
                for (i, (fname, fexpr)) in fields.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{}: {}", fname.name, fexpr)?;
                }
                write!(f, " }}")
            }
            Expr::EnumCons { enum_name, variant, args, .. } => {
                write!(f, "{}.{}(", enum_name.name, variant.name)?;
                for (i, a) in args.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{a}")?;
                }
                write!(f, ")")
            }
        }
    }
}

impl fmt::Display for Pattern {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Pattern::Ident(id) => write!(f, "{}", id.name),
            Pattern::Literal(e) => write!(f, "{e}"),
            Pattern::Wildcard(_) => write!(f, "_"),
            Pattern::Variant { enum_name, variant, .. } => write!(f, "{}.{}", enum_name.name, variant.name),
        }
    }
}

impl fmt::Display for BinOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BinOp::Add => write!(f, "+"),
            BinOp::Sub => write!(f, "-"),
            BinOp::Mul => write!(f, "*"),
            BinOp::Div => write!(f, "/"),
            BinOp::Mod => write!(f, "%"),
            BinOp::Eq => write!(f, "=="),
            BinOp::Ne => write!(f, "!="),
            BinOp::Lt => write!(f, "<"),
            BinOp::Le => write!(f, "<="),
            BinOp::Gt => write!(f, ">"),
            BinOp::Ge => write!(f, ">="),
            BinOp::And => write!(f, "&&"),
            BinOp::Or => write!(f, "||"),
            BinOp::BitAnd => write!(f, "&"),
            BinOp::BitOr => write!(f, "|"),
            BinOp::BitXor => write!(f, "^"),
            BinOp::Shl => write!(f, "<<"),
            BinOp::Shr => write!(f, ">>"),
            BinOp::Range => write!(f, ".."),
        }
    }
}

impl fmt::Display for UnOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UnOp::Neg => write!(f, "-"),
            UnOp::Not => write!(f, "!"),
            UnOp::BitNot => write!(f, "~"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use brak_core::SourceLoc;

    fn dummy_span() -> Span {
        brak_core::DUMMY_SPAN
    }

    fn ident(name: &str) -> Ident {
        Ident { name: name.to_string(), span: dummy_span() }
    }

    #[test]
    fn test_program_display() {
        let prog = Program {
            items: vec![
                Item::FnDef(FnDef {
                    name: ident("main"),
                    params: vec![],
                    ret_ty: None,
                    body: Block {
                    stmts: vec![Stmt::Return(Some(Expr::Int(42, dummy_span())), dummy_span())],
                        span: dummy_span(),
                    },
                    span: dummy_span(),
                }),
            ],
        };
        let s = prog.to_string();
        assert!(s.contains("fn main()"));
        assert!(s.contains("return 42"));
    }

    #[test]
    fn test_expr_display() {
        let expr = Expr::BinOp {
            op: BinOp::Add,
            lhs: Box::new(Expr::Int(1, dummy_span())),
            rhs: Box::new(Expr::Int(2, dummy_span())),
            span: dummy_span(),
        };
        assert_eq!(expr.to_string(), "(1 + 2)");
    }

    #[test]
    fn test_content_hash_deterministic() {
        let a = Program {
            items: vec![Item::FnDef(FnDef {
                name: ident("f"),
                params: vec![],
                ret_ty: None,
                body: Block { stmts: vec![], span: dummy_span() },
                span: dummy_span(),
            })],
        };
        let b = Program {
            items: vec![Item::FnDef(FnDef {
                name: ident("f"),
                params: vec![],
                ret_ty: None,
                body: Block { stmts: vec![], span: dummy_span() },
                span: dummy_span(),
            })],
        };
        assert_eq!(a.content_hash(), b.content_hash());
    }

    #[test]
    fn test_content_hash_different() {
        let a = Program {
            items: vec![Item::FnDef(FnDef {
                name: ident("foo"),
                params: vec![],
                ret_ty: None,
                body: Block { stmts: vec![], span: dummy_span() },
                span: dummy_span(),
            })],
        };
        let b = Program {
            items: vec![Item::FnDef(FnDef {
                name: ident("bar"),
                params: vec![],
                ret_ty: None,
                body: Block { stmts: vec![], span: dummy_span() },
                span: dummy_span(),
            })],
        };
        assert_ne!(a.content_hash(), b.content_hash());
    }

    #[test]
    fn test_serde_roundtrip() {
        let prog = Program {
            items: vec![Item::FnDef(FnDef {
                name: ident("main"),
                params: vec![],
                ret_ty: None,
                body: Block {
                    stmts: vec![Stmt::Return(Some(Expr::Int(42, dummy_span())), dummy_span())],
                    span: dummy_span(),
                },
                span: dummy_span(),
            })],
        };
        let json = serde_json::to_string(&prog).unwrap();
        let restored: Program = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.items.len(), 1);
    }

    // --- Comprehensive AST Display Tests ---

    #[test]
    fn test_all_expr_variants_display() {
        let s = Expr::Int(42, dummy_span()).to_string();
        assert_eq!(s, "42");
        let s = Expr::Float(3.14, dummy_span()).to_string();
        assert!(s.contains("3.14"));
        let s = Expr::Bool(true, dummy_span()).to_string();
        assert_eq!(s, "true");
        let s = Expr::String("hello".to_string(), dummy_span()).to_string();
        assert_eq!(s, "\"hello\"");
        let s = Expr::Ident(ident("x")).to_string();
        assert_eq!(s, "x");
        let s = Expr::Assign(
            Box::new(Expr::Ident(ident("x"))),
            Box::new(Expr::Int(1, dummy_span())),
            dummy_span(),
        ).to_string();
        assert_eq!(s, "(x = 1)");
        let s = Expr::UnOp {
            op: UnOp::Neg,
            expr: Box::new(Expr::Int(5, dummy_span())),
            span: dummy_span(),
        }.to_string();
        assert_eq!(s, "-5");
        let s = Expr::Call {
            callee: Box::new(Expr::Ident(ident("f"))),
            args: vec![Expr::Int(1, dummy_span()), Expr::Int(2, dummy_span())],
            span: dummy_span(),
        }.to_string();
        assert_eq!(s, "f(1, 2)");
    }

    #[test]
    fn test_all_binop_display() {
        assert_eq!(BinOp::Add.to_string(), "+");
        assert_eq!(BinOp::Sub.to_string(), "-");
        assert_eq!(BinOp::Mul.to_string(), "*");
        assert_eq!(BinOp::Div.to_string(), "/");
        assert_eq!(BinOp::Mod.to_string(), "%");
        assert_eq!(BinOp::Eq.to_string(), "==");
        assert_eq!(BinOp::Ne.to_string(), "!=");
        assert_eq!(BinOp::Lt.to_string(), "<");
        assert_eq!(BinOp::Le.to_string(), "<=");
        assert_eq!(BinOp::Gt.to_string(), ">");
        assert_eq!(BinOp::Ge.to_string(), ">=");
        assert_eq!(BinOp::And.to_string(), "&&");
        assert_eq!(BinOp::Or.to_string(), "||");
        assert_eq!(BinOp::BitAnd.to_string(), "&");
        assert_eq!(BinOp::BitOr.to_string(), "|");
        assert_eq!(BinOp::BitXor.to_string(), "^");
        assert_eq!(BinOp::Shl.to_string(), "<<");
        assert_eq!(BinOp::Shr.to_string(), ">>");
        assert_eq!(BinOp::Range.to_string(), "..");
    }

    #[test]
    fn test_all_unop_display() {
        assert_eq!(UnOp::Neg.to_string(), "-");
        assert_eq!(UnOp::Not.to_string(), "!");
        assert_eq!(UnOp::BitNot.to_string(), "~");
    }

    #[test]
    fn test_all_type_display() {
        assert_eq!(Type::I32.to_string(), "i32");
        assert_eq!(Type::I64.to_string(), "i64");
        assert_eq!(Type::F32.to_string(), "f32");
        assert_eq!(Type::F64.to_string(), "f64");
        assert_eq!(Type::Bool.to_string(), "bool");
        assert_eq!(Type::String.to_string(), "string");
        assert_eq!(Type::Void.to_string(), "void");
        assert_eq!(Type::Named("MyType".into()).to_string(), "MyType");
        assert_eq!(Type::Ptr(Box::new(Type::I32)).to_string(), "*i32");
        assert_eq!(Type::Ref(Box::new(Type::I32)).to_string(), "&i32");
        assert_eq!(Type::Array(Box::new(Type::I32), 10).to_string(), "[i32; 10]");
        assert_eq!(Type::Slice(Box::new(Type::I32)).to_string(), "[i32]");
        assert_eq!(Type::Fn(vec![Type::I32, Type::Bool], Box::new(Type::Void)).to_string(), "fn(i32, bool) -> void");
    }

    #[test]
    fn test_stmt_variants_display() {
        let s = Stmt::Break(dummy_span()).to_string();
        assert_eq!(s, "break;");
        let s = Stmt::Continue(dummy_span()).to_string();
        assert_eq!(s, "continue;");
        let s = Stmt::Loop {
            body: Block { stmts: vec![], span: dummy_span() },
            span: dummy_span(),
        }.to_string();
        assert!(s.contains("loop"));
        let s = Stmt::For {
            var: ident("i"),
            iterable: Box::new(Expr::Int(10, dummy_span())),
            body: Block { stmts: vec![], span: dummy_span() },
            span: dummy_span(),
        }.to_string();
        assert!(s.contains("for"));
        assert!(s.contains("i"));
        assert!(s.contains("10"));
    }

    #[test]
    fn test_expr_span() {
        let span = Expr::Int(1, Span::new(SourceLoc::new(1, 1, 0), SourceLoc::new(1, 2, 1))).span();
        assert_eq!(span.start.offset, 0);
        let span = Expr::Match {
            expr: Box::new(Expr::Int(1, dummy_span())),
            arms: vec![],
            span: Span::new(SourceLoc::new(1, 1, 0), SourceLoc::new(1, 5, 4)),
        }.span();
        assert_eq!(span.end.offset, 4);
    }

    #[test]
    fn test_type_content_hash_all_variants() {
        let mut hashes = std::collections::HashSet::new();
        hashes.insert(Type::I32.content_hash());
        hashes.insert(Type::I64.content_hash());
        hashes.insert(Type::F32.content_hash());
        hashes.insert(Type::F64.content_hash());
        hashes.insert(Type::Bool.content_hash());
        hashes.insert(Type::String.content_hash());
        hashes.insert(Type::Void.content_hash());
        hashes.insert(Type::Named("A".into()).content_hash());
        hashes.insert(Type::Ptr(Box::new(Type::I32)).content_hash());
        hashes.insert(Type::Ref(Box::new(Type::I32)).content_hash());
        hashes.insert(Type::Array(Box::new(Type::I32), 5).content_hash());
        hashes.insert(Type::Slice(Box::new(Type::I32)).content_hash());
        hashes.insert(Type::Fn(vec![], Box::new(Type::Void)).content_hash());
        assert_eq!(hashes.len(), 13, "all Type variants must have unique content hashes");
    }

    #[test]
    fn test_all_item_variants_display() {
        let items = vec![
            Item::FnDef(FnDef { name: ident("f"), params: vec![], ret_ty: None, body: Block { stmts: vec![], span: dummy_span() }, span: dummy_span() }),
            Item::ExternFn(ExternFn { name: ident("ext"), params: vec![], ret_ty: None, abi: "C".into(), span: dummy_span() }),
            Item::Let(Let { name: ident("x"), ty: Some(Type::I32), value: Some(Box::new(Expr::Int(1, dummy_span()))), span: dummy_span() }),
            Item::Struct(StructDef { vis: Visibility::Public, name: ident("Point"), fields: vec![], span: dummy_span() }),
            Item::Enum(EnumDef { vis: Visibility::Public, name: ident("Color"), variants: vec![], span: dummy_span() }),
            Item::Trait(TraitDef { vis: Visibility::Private, name: ident("Clone"), methods: vec![], span: dummy_span() }),
            Item::Impl(ImplDef { trait_name: None, target_ty: Type::Named("MyType".into()), methods: vec![], span: dummy_span() }),
            Item::Use(UseStmt { path: vec![ident("std")], span: dummy_span() }),
            Item::Mod(ModDef { name: ident("m"), items: None, span: dummy_span() }),
            Item::Const(ConstDef { vis: Visibility::Public, name: ident("MAX"), ty: Type::I32, value: Expr::Int(100, dummy_span()), span: dummy_span() }),
            Item::Static(StaticDef { vis: Visibility::Public, name: ident("GLOBAL"), ty: Type::I32, value: Expr::Int(0, dummy_span()), span: dummy_span() }),
        ];
        for item in &items {
            let s = item.to_string();
            assert!(!s.is_empty(), "Item variant display should not be empty");
        }
    }

    #[test]
    fn test_match_arm_pattern_display() {
        let arm = MatchArm {
            pattern: Pattern::Wildcard(dummy_span()),
            body: Expr::Int(0, dummy_span()),
            span: dummy_span(),
        };
        assert_eq!(arm.pattern.to_string(), "_");
        let arm = MatchArm {
            pattern: Pattern::Ident(ident("x")),
            body: Expr::Int(1, dummy_span()),
            span: dummy_span(),
        };
        assert_eq!(arm.pattern.to_string(), "x");
        let arm = MatchArm {
            pattern: Pattern::Literal(Expr::Int(42, dummy_span())),
            body: Expr::Bool(true, dummy_span()),
            span: dummy_span(),
        };
        assert_eq!(arm.pattern.to_string(), "42");
    }
}
