use brak_core::{ContentHash, combine_hash, Span};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HirProgram {
    pub items: Vec<HirItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HirItem {
    Function(HirFunction),
    ExternFunction(HirExternFunction),
    GlobalLet(HirGlobalLet),
    Struct(HirStruct),
    Enum(HirEnum),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HirStruct {
    pub name: String,
    pub fields: Vec<HirField>,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HirField {
    pub name: String,
    pub ty: HirType,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HirEnum {
    pub name: String,
    pub variants: Vec<HirVariant>,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HirVariant {
    pub name: String,
    pub fields: Option<Vec<HirType>>,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HirExternFunction {
    pub name: String,
    pub params: Vec<HirParam>,
    pub ret_ty: HirType,
    pub abi: String,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HirFunction {
    pub name: String,
    pub params: Vec<HirParam>,
    pub ret_ty: HirType,
    pub body: HirBlock,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HirParam {
    pub name: String,
    pub ty: HirType,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HirGlobalLet {
    pub name: String,
    pub ty: HirType,
    pub value: Option<Box<HirExpr>>,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HirBlock {
    pub stmts: Vec<HirStmt>,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HirStmt {
    Let {
        name: String,
        ty: HirType,
        value: Option<Box<HirExpr>>,
        span: Span,
    },
    Expr(Box<HirExpr>, Span),
    Return(Option<Box<HirExpr>>, Span),
    If {
        cond: Box<HirExpr>,
        then: HirBlock,
        else_: Option<HirBlock>,
        span: Span,
    },
    Loop {
        body: HirBlock,
        span: Span,
    },
    While {
        cond: Box<HirExpr>,
        body: HirBlock,
        span: Span,
    },
    Break(Span),
    Continue(Span),
    For {
        var: String,
        iterable: Box<HirExpr>,
        body: HirBlock,
        span: Span,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HirExpr {
    Int(i64, Span),
    Float(f64, Span),
    Bool(bool, Span),
    String(String, Span),
    Ident(String, Span),
    Assign(String, Box<HirExpr>, Span),
    BinOp {
        op: HirBinOp,
        lhs: Box<HirExpr>,
        rhs: Box<HirExpr>,
        span: Span,
    },
    UnOp {
        op: HirUnOp,
        expr: Box<HirExpr>,
        span: Span,
    },
    Call {
        callee: Box<HirExpr>,
        args: Vec<HirExpr>,
        span: Span,
    },
    If {
        cond: Box<HirExpr>,
        then: Box<HirExpr>,
        else_: Box<HirExpr>,
        span: Span,
    },
    Block(HirBlock),
    Match {
        expr: Box<HirExpr>,
        arms: Vec<(HirPattern, HirExpr)>, // (pattern, body)
        span: Span,
    },
    Field {
        object: Box<HirExpr>,
        field: String,
        span: Span,
    },
    StructInit {
        name: String,
        fields: Vec<(String, HirExpr)>,
        span: Span,
    },
    /// Enum construction: `EnumName.Variant(...)` (Fase 7).
    /// Fieldless variants have empty args; payload variants carry one
    /// expression per payload field.
    EnumInit {
        enum_name: String,
        variant: String,
        args: Vec<HirExpr>,
        span: Span,
    },
    FieldAssign {
        object: Box<HirExpr>,
        field: String,
        value: Box<HirExpr>,
        span: Span,
    },
}

/// Match arm pattern (BUG-K03: previously flattened to a string, losing all
/// structure — match always executed the first arm).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum HirPattern {
    /// `_` - matches anything
    Wildcard,
    /// `name` - matches anything and binds the scrutinee to `name`
    Binding(String),
    /// literal pattern - compared against the scrutinee with Eq
    Literal(HirLiteral),
    /// `EnumName.Variant` - matches that enum variant's tag (Fase 7)
    Variant {
        enum_name: String,
        variant: String,
        /// Payload destructuring bindings, in declaration order.
        bindings: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum HirLiteral {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
}

impl ContentHash for HirLiteral {
    fn content_hash(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        std::mem::discriminant(self).hash(&mut hasher);
        match self {
            HirLiteral::Int(i) => i.hash(&mut hasher),
            HirLiteral::Float(f) => f.to_bits().hash(&mut hasher),
            HirLiteral::Bool(b) => b.hash(&mut hasher),
            HirLiteral::Str(s) => s.hash(&mut hasher),
        }
        hasher.finish()
    }
}

impl ContentHash for HirPattern {
    fn content_hash(&self) -> u64 {
        match self {
            HirPattern::Wildcard => 1,
            HirPattern::Binding(s) => s.content_hash(),
            HirPattern::Literal(l) => l.content_hash(),
            HirPattern::Variant { enum_name, variant, .. } => {
                combine_hash(enum_name.content_hash(), variant.content_hash())
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HirBinOp {
    Add, Sub, Mul, Div, Mod,
    Eq, Ne, Lt, Le, Gt, Ge,
    And, Or,
    BitAnd, BitOr, BitXor,
    Shl, Shr,
    Range,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HirUnOp {
    Neg, Not, BitNot,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum HirType {
    I32, I64, F32, F64, Bool, String, Void,
    Named(String),
    Ptr(Box<HirType>),
    Ref(Box<HirType>),
    Array(Box<HirType>, usize),
    Slice(Box<HirType>),
    Fn(Vec<HirType>, Box<HirType>),
}

impl HirExpr {
    pub fn span(&self) -> Span {
        match self {
            HirExpr::Int(_, s) | HirExpr::Float(_, s) | HirExpr::Bool(_, s) | HirExpr::String(_, s) => *s,
            HirExpr::Ident(_, s) => *s,
            HirExpr::Assign(_, _, s) => *s,
            HirExpr::BinOp { span, .. }
            | HirExpr::UnOp { span, .. }
            | HirExpr::Call { span, .. }
            | HirExpr::If { span, .. } => *span,
            HirExpr::Block(b) => b.span,
            HirExpr::Match { span, .. } => *span,
            HirExpr::Field { span, .. } => *span,
            HirExpr::StructInit { span, .. } => *span,
            HirExpr::EnumInit { span, .. } => *span,
            HirExpr::FieldAssign { span, .. } => *span,
        }
    }
}

impl std::fmt::Display for HirLiteral {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HirLiteral::Int(i) => write!(f, "{i}"),
            HirLiteral::Float(x) => write!(f, "{x}"),
            HirLiteral::Bool(b) => write!(f, "{b}"),
            HirLiteral::Str(s) => write!(f, "\"{s}\""),
        }
    }
}

impl std::fmt::Display for HirPattern {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HirPattern::Wildcard => write!(f, "_"),
            HirPattern::Binding(s) => write!(f, "{s}"),
            HirPattern::Literal(l) => write!(f, "{l}"),
            HirPattern::Variant { enum_name, variant, .. } => write!(f, "{enum_name}.{variant}"),
        }
    }
}

impl ContentHash for HirProgram {
    fn content_hash(&self) -> u64 {
        let mut h = 0u64;
        for item in &self.items {
            h = combine_hash(h, item.content_hash());
        }
        h
    }
}

impl ContentHash for HirItem {
    fn content_hash(&self) -> u64 {
        match self {
            HirItem::Function(f) => f.content_hash(),
            HirItem::ExternFunction(e) => e.content_hash(),
            HirItem::GlobalLet(l) => l.content_hash(),
            HirItem::Struct(s) => s.content_hash(),
            HirItem::Enum(e) => e.content_hash(),
        }
    }
}

impl ContentHash for HirStruct {
    fn content_hash(&self) -> u64 {
        let mut h = self.name.content_hash();
        for field in &self.fields {
            h = combine_hash(h, field.name.content_hash());
        }
        h
    }
}

impl ContentHash for HirEnum {
    fn content_hash(&self) -> u64 {
        let mut h = self.name.content_hash();
        for variant in &self.variants {
            h = combine_hash(h, variant.name.content_hash());
        }
        h
    }
}

impl ContentHash for HirExternFunction {
    fn content_hash(&self) -> u64 {
        let mut h = self.name.content_hash();
        h = combine_hash(h, self.abi.content_hash());
        h
    }
}

impl ContentHash for HirFunction {
    fn content_hash(&self) -> u64 {
        let mut h = self.name.content_hash();
        h = combine_hash(h, self.body.content_hash());
        h
    }
}

impl ContentHash for HirGlobalLet {
    fn content_hash(&self) -> u64 {
        let mut h = self.name.content_hash();
        if let Some(v) = &self.value {
            h = combine_hash(h, v.content_hash());
        }
        h
    }
}

impl ContentHash for HirBlock {
    fn content_hash(&self) -> u64 {
        let mut h = 0u64;
        for s in &self.stmts {
            h = combine_hash(h, s.content_hash());
        }
        h
    }
}

impl ContentHash for HirStmt {
    fn content_hash(&self) -> u64 {
        match self {
            HirStmt::Let { name, value, .. } => {
                let mut h = name.content_hash();
                if let Some(v) = value {
                    h = combine_hash(h, v.content_hash());
                }
                h
            }
            HirStmt::Expr(e, _) => e.content_hash(),
            HirStmt::Return(e, _) => {
                if let Some(v) = e { v.content_hash() } else { 1 }
            }
            HirStmt::If { cond, then, else_, .. } => {
                let mut h = cond.content_hash();
                h = combine_hash(h, then.content_hash());
                if let Some(b) = else_ {
                    h = combine_hash(h, b.content_hash());
                }
                h
            }
            HirStmt::Loop { body, .. } | HirStmt::While { body, .. } => {
                body.content_hash()
            }
            HirStmt::Break(_) => 2,
            HirStmt::Continue(_) => 3,
            HirStmt::For { var, iterable, body, .. } => {
                let mut h = var.content_hash();
                h = combine_hash(h, iterable.content_hash());
                combine_hash(h, body.content_hash())
            }
        }
    }
}

impl ContentHash for HirExpr {
    fn content_hash(&self) -> u64 {
        match self {
            HirExpr::Int(i, _) => *i as u64,
            HirExpr::Float(f, _) => f.to_bits(),
            HirExpr::Bool(b, _) => *b as u64,
            HirExpr::String(s, _) | HirExpr::Ident(s, _) => s.content_hash(),
            HirExpr::Assign(_, rhs, _) => rhs.content_hash(),
            HirExpr::BinOp { op, lhs, rhs, .. } => {
                let mut h = *op as u64;
                h = combine_hash(h, lhs.content_hash());
                h = combine_hash(h, rhs.content_hash());
                h
            }
            HirExpr::UnOp { op, expr, .. } => {
                combine_hash(*op as u64, expr.content_hash())
            }
            HirExpr::Call { callee, args, .. } => {
                let mut h = callee.content_hash();
                for a in args {
                    h = combine_hash(h, a.content_hash());
                }
                h
            }
            HirExpr::If { cond, then, else_, .. } => {
                let mut h = cond.content_hash();
                h = combine_hash(h, then.content_hash());
                h = combine_hash(h, else_.content_hash());
                h
            }
            HirExpr::Block(b) => b.content_hash(),
            HirExpr::Match { expr, arms, .. } => {
                let mut h = expr.content_hash();
                for (pat, body) in arms {
                    h = combine_hash(h, pat.content_hash());
                    h = combine_hash(h, body.content_hash());
                }
                h
            }
            HirExpr::Field { object, field, .. } => combine_hash(object.content_hash(), field.content_hash()),
            HirExpr::EnumInit { enum_name, variant, .. } => {
                combine_hash(enum_name.content_hash(), variant.content_hash())
            }
            HirExpr::StructInit { name, fields, .. } => {
                let mut h = name.content_hash();
                for (fname, fexpr) in fields {
                    h = combine_hash(h, fname.content_hash());
                    h = combine_hash(h, fexpr.content_hash());
                }
                h
            }
            HirExpr::FieldAssign { object, field, value, .. } => {
                combine_hash(combine_hash(object.content_hash(), field.content_hash()), value.content_hash())
            }
        }
    }
}

impl std::fmt::Display for HirProgram {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for item in &self.items {
            writeln!(f, "{item}")?;
        }
        Ok(())
    }
}

impl std::fmt::Display for HirItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HirItem::Function(func) => write!(f, "{func}"),
            HirItem::ExternFunction(e) => write!(f, "{e}"),
            HirItem::GlobalLet(l) => write!(f, "let {}: {};", l.name, l.ty),
            HirItem::Struct(s) => {
                writeln!(f, "struct {} {{", s.name)?;
                for field in &s.fields {
                    writeln!(f, "  {}: {},", field.name, field.ty)?;
                }
                write!(f, "}}")
            }
            HirItem::Enum(e) => {
                writeln!(f, "enum {} {{", e.name)?;
                for variant in &e.variants {
                    write!(f, "  {}", variant.name)?;
                    if let Some(fields) = &variant.fields {
                        write!(f, "(")?;
                        for (i, ty) in fields.iter().enumerate() {
                            if i > 0 { write!(f, ", ")?; }
                            write!(f, "{ty}")?;
                        }
                        write!(f, ")")?;
                    }
                    writeln!(f, ",")?;
                }
                write!(f, "}}")
            }
        }
    }
}

impl std::fmt::Display for HirExternFunction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "extern \"{}\" fn {}(", self.abi, self.name)?;
        for (i, p) in self.params.iter().enumerate() {
            if i > 0 { write!(f, ", ")?; }
            write!(f, "{}: {}", p.name, p.ty)?;
        }
        write!(f, ") -> {};", self.ret_ty)
    }
}

impl std::fmt::Display for HirFunction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "fn {}(", self.name)?;
        for (i, p) in self.params.iter().enumerate() {
            if i > 0 { write!(f, ", ")?; }
            write!(f, "{}: {}", p.name, p.ty)?;
        }
        write!(f, ") -> {} {}", self.ret_ty, self.body)
    }
}

impl std::fmt::Display for HirBlock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{{")?;
        for s in &self.stmts {
            writeln!(f, "  {s}")?;
        }
        write!(f, "}}")
    }
}

impl std::fmt::Display for HirStmt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HirStmt::Let { name, ty, value, .. } => {
                write!(f, "let {name}: {ty}")?;
                if let Some(v) = value {
                    write!(f, " = {v}")?;
                }
                write!(f, ";")
            }
            HirStmt::Expr(e, _) => write!(f, "{e};"),
            HirStmt::Return(v, _) => {
                if let Some(v) = v { write!(f, "return {v};") }
                else { write!(f, "return;") }
            }
            HirStmt::If { cond, then, else_, .. } => {
                write!(f, "if {cond} {then}")?;
                if let Some(b) = else_ {
                    write!(f, " else {b}")?;
                }
                Ok(())
            }
            HirStmt::Loop { body, .. } => write!(f, "loop {body}"),
            HirStmt::While { cond, body, .. } => write!(f, "while {cond} {body}"),
            HirStmt::Break(_) => write!(f, "break;"),
            HirStmt::Continue(_) => write!(f, "continue;"),
            HirStmt::For { var, iterable, body, .. } => {
                write!(f, "for {var} in {iterable} {body}")
            }
        }
    }
}

impl std::fmt::Display for HirExpr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HirExpr::Int(i, _) => write!(f, "{i}"),
            HirExpr::Float(fl, _) => write!(f, "{fl}"),
            HirExpr::Bool(b, _) => write!(f, "{b}"),
            HirExpr::String(s, _) => write!(f, "\"{s}\""),
            HirExpr::Ident(id, _) => write!(f, "{id}"),
            HirExpr::Assign(name, rhs, _) => write!(f, "({name} = {rhs})"),
            HirExpr::BinOp { op, lhs, rhs, .. } => write!(f, "({lhs} {op} {rhs})"),
            HirExpr::UnOp { op, expr, .. } => write!(f, "{op}{expr}"),
            HirExpr::Call { callee, args, .. } => {
                write!(f, "{callee}(")?;
                for (i, a) in args.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{a}")?;
                }
                write!(f, ")")
            }
            HirExpr::If { cond, then, else_, .. } => {
                write!(f, "if {cond} {then} else {else_}")
            }
            HirExpr::Block(b) => write!(f, "{b}"),
            HirExpr::Match { expr, arms, .. } => {
                writeln!(f, "match {expr} {{")?;
                for (pat, body) in arms {
                    writeln!(f, "    {pat} => {body}")?;
                }
                write!(f, "}}")
            }
            HirExpr::Field { object, field, .. } => write!(f, "{object}.{field}"),
            HirExpr::EnumInit { enum_name, variant, .. } => write!(f, "{enum_name}.{variant}()"),
            HirExpr::StructInit { name, fields, .. } => {
                write!(f, "{} {{ ", name)?;
                for (i, (fname, fexpr)) in fields.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{fname}: {fexpr}")?;
                }
                write!(f, " }}")
            }
            HirExpr::FieldAssign { object, field, value, .. } => write!(f, "{object}.{field} = {value}"),
        }
    }
}

impl std::fmt::Display for HirBinOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HirBinOp::Add => write!(f, "+"), HirBinOp::Sub => write!(f, "-"),
            HirBinOp::Mul => write!(f, "*"), HirBinOp::Div => write!(f, "/"),
            HirBinOp::Mod => write!(f, "%"),
            HirBinOp::Eq => write!(f, "=="), HirBinOp::Ne => write!(f, "!="),
            HirBinOp::Lt => write!(f, "<"), HirBinOp::Le => write!(f, "<="),
            HirBinOp::Gt => write!(f, ">"), HirBinOp::Ge => write!(f, ">="),
            HirBinOp::And => write!(f, "&&"), HirBinOp::Or => write!(f, "||"),
            HirBinOp::BitAnd => write!(f, "&"), HirBinOp::BitOr => write!(f, "|"),
            HirBinOp::BitXor => write!(f, "^"),
            HirBinOp::Shl => write!(f, "<<"), HirBinOp::Shr => write!(f, ">>"),
            HirBinOp::Range => write!(f, ".."),
        }
    }
}

impl std::fmt::Display for HirUnOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HirUnOp::Neg => write!(f, "-"),
            HirUnOp::Not => write!(f, "!"),
            HirUnOp::BitNot => write!(f, "~"),
        }
    }
}

impl std::fmt::Display for HirType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HirType::I32 => write!(f, "i32"),
            HirType::I64 => write!(f, "i64"),
            HirType::F32 => write!(f, "f32"),
            HirType::F64 => write!(f, "f64"),
            HirType::Bool => write!(f, "bool"),
            HirType::String => write!(f, "string"),
            HirType::Void => write!(f, "void"),
            HirType::Named(s) => write!(f, "{s}"),
            HirType::Ptr(t) => write!(f, "*{t}"),
            HirType::Ref(t) => write!(f, "&{t}"),
            HirType::Array(t, n) => write!(f, "[{t}; {n}]"),
            HirType::Slice(t) => write!(f, "[{t}]"),
            HirType::Fn(args, ret) => {
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

