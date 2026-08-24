use brak_core::{ContentHash, combine_hash, Span};
use serde::{Deserialize, Serialize};

pub type LocalId = usize;
pub type BlockId = usize;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MirProgram {
    pub functions: Vec<MirFunction>,
    pub extern_functions: Vec<MirExternFunction>,
    pub structs: Vec<MirStruct>,
    pub enums: Vec<MirEnum>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MirStruct {
    pub name: String,
    pub fields: Vec<MirField>,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MirField {
    pub name: String,
    pub ty: MirType,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MirEnum {
    pub name: String,
    pub variants: Vec<MirVariant>,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MirVariant {
    pub name: String,
    pub fields: Option<Vec<MirType>>,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MirExternFunction {
    pub name: String,
    pub params: Vec<MirType>,
    pub ret_ty: MirType,
    pub abi: String,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MirFunction {
    pub name: String,
    pub params: Vec<LocalId>,
    pub ret_ty: MirType,
    pub blocks: Vec<MirBlock>,
    pub locals: Vec<MirLocal>,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MirLocal {
    pub name: String,
    pub ty: MirType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MirBlock {
    pub id: BlockId,
    pub name: String,
    pub insts: Vec<MirInst>,
    pub terminator: MirTerminator,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MirInst {
    Assign {
        dest: LocalId,
        value: MirValue,
        span: Span,
    },
    Call {
        dest: Option<LocalId>,
        callee: String,
        args: Vec<LocalId>,
        span: Span,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MirTerminator {
    Return { value: Option<LocalId>, span: Span },
    Jump { target: BlockId, span: Span },
    Branch { cond: LocalId, then: BlockId, else_: BlockId, span: Span },
    Unreachable,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MirValue {
    Local(LocalId),
    Int(i64),
    Float(f64),
    Bool(bool),
    String(String),
    BinOp {
        op: MirBinOp,
        lhs: LocalId,
        rhs: LocalId,
    },
    UnOp {
        op: MirUnOp,
        expr: LocalId,
    },
    GetField {
        object: LocalId,
        name: String, // Struct name
        field: String,
    },
    StructInit {
        name: String,
        fields: Vec<(String, LocalId)>,
    },
    SetField {
        object: LocalId,
        field: String,
        value: LocalId,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MirBinOp {
    Add, Sub, Mul, Div, Mod,
    // BUG-M17: dedicated float ops — backends can no longer misinterpret
    // float arithmetic as integer (the old ImmF64 heuristic was unreliable).
    FAdd, FSub, FMul, FDiv,
    Eq, Ne, Lt, Le, Gt, Ge,
    And, Or,
    BitAnd, BitOr, BitXor, Shl, Shr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MirUnOp {
    Neg, Not, BitNot,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MirType {
    I32, I64, F32, F64, Bool, String, Void,
    Named(String),
}

impl ContentHash for MirProgram {
    fn content_hash(&self) -> u64 {
        let mut h = 0u64;
        for f in &self.functions {
            h = combine_hash(h, f.content_hash());
        }
        h
    }
}

impl ContentHash for MirFunction {
    fn content_hash(&self) -> u64 {
        let mut h = self.name.content_hash();
        for b in &self.blocks {
            h = combine_hash(h, b.content_hash());
        }
        h
    }
}

impl ContentHash for MirBlock {
    fn content_hash(&self) -> u64 {
        let mut h = 0u64;
        for i in &self.insts {
            h = combine_hash(h, i.content_hash());
        }
        h = combine_hash(h, self.terminator.content_hash());
        h
    }
}

impl ContentHash for MirInst {
    fn content_hash(&self) -> u64 {
        match self {
            MirInst::Assign { dest, value, .. } => {
                combine_hash(*dest as u64, value.content_hash())
            }
            MirInst::Call { dest, callee, .. } => {
                let mut h = callee.content_hash();
                if let Some(d) = dest {
                    h = combine_hash(h, *d as u64);
                }
                h
            }
        }
    }
}

impl ContentHash for MirTerminator {
    fn content_hash(&self) -> u64 {
        match self {
            MirTerminator::Return { value, .. } => value.map(|v| v as u64).unwrap_or(0),
            MirTerminator::Jump { target, .. } => *target as u64,
            MirTerminator::Branch { cond, then, else_, .. } => {
                let mut h = *cond as u64;
                h = combine_hash(h, *then as u64);
                h = combine_hash(h, *else_ as u64);
                h
            }
            MirTerminator::Unreachable => 0,
        }
    }
}

impl ContentHash for MirValue {
    fn content_hash(&self) -> u64 {
        match self {
            MirValue::Local(id) => *id as u64,
            MirValue::Int(i) => *i as u64,
            MirValue::Float(f) => f.to_bits(),
            MirValue::Bool(b) => *b as u64,
            MirValue::String(s) => s.content_hash(),
            MirValue::BinOp { op, lhs, rhs } => {
                let mut h = *op as u64;
                h = combine_hash(h, *lhs as u64);
                h = combine_hash(h, *rhs as u64);
                h
            }
            MirValue::UnOp { op, expr } => {
                combine_hash(*op as u64, *expr as u64)
            }
            MirValue::GetField { object, name: _, field } => {
                combine_hash(*object as u64, field.content_hash())
            }
            MirValue::StructInit { name, fields } => {
                let mut h = name.content_hash();
                for (fname, fid) in fields {
                    h = combine_hash(h, fname.content_hash());
                    h = combine_hash(h, *fid as u64);
                }
                h
            }
            MirValue::SetField { object, field, value } => {
                combine_hash(combine_hash(*object as u64, field.content_hash()), *value as u64)
            }
        }
    }
}

impl std::fmt::Display for MirProgram {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for func in &self.functions {
            writeln!(f, "{func}")?;
        }
        Ok(())
    }
}

impl std::fmt::Display for MirFunction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "fn {}(", self.name)?;
        for local in &self.locals {
            writeln!(f, "  local {}: {} <- {}", 
                self.locals.iter().position(|l| l.name == local.name).unwrap_or(0),
                local.name, local.ty)?;
        }
        for block in &self.blocks {
            writeln!(f, "{block}")?;
        }
        Ok(())
    }
}

impl std::fmt::Display for MirBlock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "  block {} ({}):", self.id, self.name)?;
        for inst in &self.insts {
            writeln!(f, "    {inst}")?;
        }
        writeln!(f, "    {term}", term = self.terminator)
    }
}

impl std::fmt::Display for MirInst {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MirInst::Assign { dest, value, .. } => {
                write!(f, "%{dest} = {value}")
            }
            MirInst::Call { dest, callee, args, .. } => {
                if let Some(d) = dest {
                    write!(f, "%{d} = ")?;
                }
                write!(f, "call {callee}(")?;
                for (i, a) in args.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "%{a}")?;
                }
                write!(f, ")")
            }
        }
    }
}

impl std::fmt::Display for MirTerminator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MirTerminator::Return { value, .. } => {
                if let Some(v) = value { write!(f, "ret %{v}") }
                else { write!(f, "ret") }
            }
            MirTerminator::Jump { target, .. } => write!(f, "jmp block{target}"),
            MirTerminator::Branch { cond, then, else_, .. } => {
                write!(f, "br %{cond} ? block{then} : block{else_}")
            }
            MirTerminator::Unreachable => write!(f, "unreachable"),
        }
    }
}

impl std::fmt::Display for MirValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MirValue::Local(id) => write!(f, "%{id}"),
            MirValue::Int(i) => write!(f, "{i}"),
            MirValue::Float(fl) => write!(f, "{fl}"),
            MirValue::Bool(b) => write!(f, "{b}"),
            MirValue::String(s) => write!(f, "\"{s}\""),
            MirValue::BinOp { op, lhs, rhs } => write!(f, "(%{lhs} {op} %{rhs})"),
            MirValue::UnOp { op, expr } => write!(f, "{op}%{expr}"),
            MirValue::GetField { object, name: _, field } => write!(f, "%{object}.{field}"),
            MirValue::StructInit { name, fields } => {
                write!(f, "{} {{ ", name)?;
                for (i, (fname, fid)) in fields.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{fname}: %{fid}")?;
                }
                write!(f, " }}")
            }
            MirValue::SetField { object, field, value } => write!(f, "%{object}.{field} = %{value}"),
        }
    }
}

impl std::fmt::Display for MirBinOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MirBinOp::Add => write!(f, "+"), MirBinOp::Sub => write!(f, "-"),
            MirBinOp::Mul => write!(f, "*"), MirBinOp::Div => write!(f, "/"),
            MirBinOp::Mod => write!(f, "%"),
            MirBinOp::FAdd => write!(f, "+."), MirBinOp::FSub => write!(f, "-."),
            MirBinOp::FMul => write!(f, "*."), MirBinOp::FDiv => write!(f, "/."),
            MirBinOp::Eq => write!(f, "=="), MirBinOp::Ne => write!(f, "!="),
            MirBinOp::Lt => write!(f, "<"), MirBinOp::Le => write!(f, "<="),
            MirBinOp::Gt => write!(f, ">"), MirBinOp::Ge => write!(f, ">="),
            MirBinOp::And => write!(f, "&&"), MirBinOp::Or => write!(f, "||"),
            MirBinOp::BitAnd => write!(f, "&"), MirBinOp::BitOr => write!(f, "|"),
            MirBinOp::BitXor => write!(f, "^"),
            MirBinOp::Shl => write!(f, "<<"), MirBinOp::Shr => write!(f, ">>"),
        }
    }
}

impl std::fmt::Display for MirUnOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MirUnOp::Neg => write!(f, "-"),
            MirUnOp::Not => write!(f, "!"),
            MirUnOp::BitNot => write!(f, "~"),
        }
    }
}

impl std::fmt::Display for MirType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MirType::I32 => write!(f, "i32"), MirType::I64 => write!(f, "i64"),
            MirType::F32 => write!(f, "f32"), MirType::F64 => write!(f, "f64"),
            MirType::Bool => write!(f, "bool"), MirType::String => write!(f, "string"),
            MirType::Void => write!(f, "void"),
            MirType::Named(s) => write!(f, "{s}"),
        }
    }
}
