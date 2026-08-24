use brak_core::{ContentHash, combine_hash, Span};
use serde::{Deserialize, Serialize};

pub type VirtReg = usize;
pub type BlockId = usize;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LirProgram {
    pub functions: Vec<LirFunction>,
    pub extern_functions: Vec<LirExternFunction>,
    pub structs: Vec<LirStructMetadata>,
    pub enums: Vec<LirEnumMetadata>,
    pub string_table: Vec<String>,
    pub files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LirStructMetadata {
    pub name: String,
    pub fields: Vec<(String, LirType)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LirEnumMetadata {
    pub name: String,
    pub variants: Vec<(String, Option<Vec<LirType>>)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LirType {
    I32, I64, F32, F64, Bool, String, Void,
    Named(String),
    Ptr(Box<LirType>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LirExternFunction {
    pub name: String,
    pub abi: CallingConvention,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LirFunction {
    pub name: String,
    pub params: Vec<VirtReg>,
    pub blocks: Vec<LirBlock>,
    pub reg_count: usize,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LirBlock {
    pub id: BlockId,
    pub name: String,
    pub insts: Vec<LirInst>,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CallingConvention {
    Brak,
    Cdecl,
    SystemV,
    Win64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LirInst {
    pub opcode: LirOpcode,
    pub dest: Option<VirtReg>,
    pub operands: Vec<LirOperand>,
    pub call_conv: Option<CallingConvention>,
    pub debug: Span,
    pub file_id: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LirOpcode {
    Mov,
    Add, Sub, Mul, Div, Mod,
    // BUG-M17: float arithmetic (f64 semantics in every backend).
    FAdd, FSub, FMul, FDiv,
    Neg, Not,
    And, Or, Xor,
    Shl, Shr,
    Cmp,
    SetEq, SetNe, SetLt, SetLe, SetGt, SetGe,
    Load, Store,
    Alloca,
    Call,
    Ret,
    Jmp,
    Br,
    Push, Pop,
    Comment,
    GetField,
    StructInit,
    SetField,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LirOperand {
    Reg(VirtReg),
    ImmI64(i64),
    ImmF64(f64),
    Label(String),
    StackSlot(u32),
    StringRef(usize),
    Field(String),
}

impl Eq for LirOperand {}

impl std::hash::Hash for LirOperand {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        core::mem::discriminant(self).hash(state);
        match self {
            LirOperand::Reg(r) => r.hash(state),
            LirOperand::ImmI64(i) => i.hash(state),
            LirOperand::ImmF64(f) => f.to_bits().hash(state),
            LirOperand::Label(s) => s.hash(state),
            LirOperand::StackSlot(s) => s.hash(state),
            LirOperand::StringRef(i) => i.hash(state),
            LirOperand::Field(s) => s.hash(state),
        }
    }
}

impl LirInst {
    pub fn new(opcode: LirOpcode) -> Self {
        Self {
            opcode,
            dest: None,
            operands: vec![],
            call_conv: None,
            debug: Span::new(Default::default(), Default::default()),
            file_id: 0,
        }
    }

    pub fn with_call_conv(mut self, cc: CallingConvention) -> Self {
        self.call_conv = Some(cc);
        self
    }

    pub fn with_dest(mut self, dest: VirtReg) -> Self {
        self.dest = Some(dest);
        self
    }

    pub fn with_op(mut self, op: LirOperand) -> Self {
        self.operands.push(op);
        self
    }

    pub fn with_ops(mut self, ops: Vec<LirOperand>) -> Self {
        self.operands = ops;
        self
    }

    pub fn with_debug(mut self, span: Span) -> Self {
        self.debug = span;
        self
    }

    pub fn with_file(mut self, file_id: usize) -> Self {
        self.file_id = file_id;
        self
    }
}

impl ContentHash for LirProgram {
    fn content_hash(&self) -> u64 {
        let mut h = 0u64;
        for f in &self.functions {
            h = combine_hash(h, f.content_hash());
        }
        h
    }
}

impl ContentHash for LirFunction {
    fn content_hash(&self) -> u64 {
        let mut h = self.name.content_hash();
        for b in &self.blocks {
            h = combine_hash(h, b.content_hash());
        }
        h
    }
}

impl ContentHash for LirBlock {
    fn content_hash(&self) -> u64 {
        let mut h = 0u64;
        for i in &self.insts {
            h = combine_hash(h, i.content_hash());
        }
        h
    }
}

impl ContentHash for LirInst {
    fn content_hash(&self) -> u64 {
        let mut h = self.opcode as u64;
        if let Some(d) = self.dest {
            h = combine_hash(h, d as u64);
        }
        for op in &self.operands {
            h = combine_hash(h, op.content_hash());
        }
        h
    }
}

impl ContentHash for LirOperand {
    fn content_hash(&self) -> u64 {
        match self {
            LirOperand::Reg(r) => *r as u64,
            LirOperand::ImmI64(i) => *i as u64,
            LirOperand::ImmF64(f) => f.to_bits(),
            LirOperand::Label(s) => s.content_hash(),
            LirOperand::StackSlot(s) => *s as u64,
            LirOperand::StringRef(i) => *i as u64,
            LirOperand::Field(s) => s.content_hash(),
        }
    }
}

impl std::fmt::Display for LirProgram {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for func in &self.functions {
            writeln!(f, "{func}")?;
        }
        Ok(())
    }
}

impl std::fmt::Display for LirFunction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "fn {} (regs: {}):", self.name, self.reg_count)?;
        for block in &self.blocks {
            writeln!(f, "{block}")?;
        }
        Ok(())
    }
}

impl std::fmt::Display for LirBlock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "block {} ({}):", self.id, self.name)?;
        for inst in &self.insts {
            writeln!(f, "  {inst}")?;
        }
        Ok(())
    }
}

impl std::fmt::Display for LirInst {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:10}", format!("{:?}", self.opcode))?;
        if let Some(d) = self.dest {
            write!(f, " %r{d}")?;
        }
        for op in &self.operands {
            write!(f, " {op}")?;
        }
        Ok(())
    }
}

impl std::fmt::Display for LirOperand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LirOperand::Reg(r) => write!(f, "%r{r}"),
            LirOperand::ImmI64(i) => write!(f, "{i}"),
            LirOperand::ImmF64(fl) => write!(f, "{fl}"),
            LirOperand::Label(s) => write!(f, "label:{s}"),
            LirOperand::StackSlot(s) => write!(f, "stack[{s}]"),
            LirOperand::StringRef(i) => write!(f, "str[{i}]"),
            LirOperand::Field(s) => write!(f, ".{s}"),
        }
    }
}
