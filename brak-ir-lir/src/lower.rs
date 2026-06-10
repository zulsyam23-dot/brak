
use std::collections::HashMap;
use brak_core::Span;
use brak_ir_mir::mir::*;

use crate::lir::*;

pub struct LirLower {
    externs: HashMap<String, CallingConvention>,
    string_table: Vec<String>,
    current_file_id: usize,
}

impl Default for LirLower {
    fn default() -> Self {
        Self::new()
    }
}

fn lower_mir_type_to_lir(ty: &MirType) -> LirType {
    match ty {
        MirType::I32 => LirType::I32,
        MirType::I64 => LirType::I64,
        MirType::F32 => LirType::F32,
        MirType::F64 => LirType::F64,
        MirType::Bool => LirType::Bool,
        MirType::String => LirType::String,
        MirType::Void => LirType::Void,
        MirType::Named(s) => LirType::Named(s.clone()),
    }
}

impl LirLower {
    pub fn new() -> Self {
        Self {
            externs: HashMap::new(),
            string_table: Vec::new(),
            current_file_id: 0,
        }
    }

    pub fn set_file_id(&mut self, file_id: usize) {
        self.current_file_id = file_id;
    }

    fn intern_string(&mut self, s: &str) -> usize {
        if let Some(i) = self.string_table.iter().position(|x| x == s) {
            i
        } else {
            let i = self.string_table.len();
            self.string_table.push(s.to_string());
            i
        }
    }

    pub fn lower(&mut self, program: MirProgram) -> LirProgram {
        let structs = program.structs.iter().map(|s| LirStructMetadata {
            name: s.name.clone(),
            fields: s.fields.iter().map(|f| (f.name.clone(), lower_mir_type_to_lir(&f.ty))).collect(),
        }).collect();

        let enums = program.enums.iter().map(|e| LirEnumMetadata {
            name: e.name.clone(),
            variants: e.variants.iter().map(|v| (v.name.clone(), v.fields.as_ref().map(|fs| fs.iter().map(lower_mir_type_to_lir).collect()))).collect(),
        }).collect();

        let extern_functions: Vec<LirExternFunction> = program
            .extern_functions
            .into_iter()
            .map(|e| {
                let cc = match e.abi.to_lowercase().as_str() {
                    "c" | "cdecl" => CallingConvention::Cdecl,
                    "win64" => CallingConvention::Win64,
                    "systemv" => CallingConvention::SystemV,
                    _ => CallingConvention::Brak,
                };
                self.externs.insert(e.name.clone(), cc);
                LirExternFunction {
                    name: e.name,
                    abi: cc,
                    span: e.span,
                }
            })
            .collect();

        let functions = program
            .functions
            .into_iter()
            .map(|f| self.lower_function(f))
            .collect();

        let string_table = std::mem::take(&mut self.string_table);
        LirProgram { functions, extern_functions, structs, enums, string_table, files: vec![] }
    }

    fn lower_function(&mut self, func: MirFunction) -> LirFunction {
        let blocks: Vec<LirBlock> = func
            .blocks
            .iter()
            .map(|b| self.lower_block(b.clone()))
            .collect();
        LirFunction {
            name: func.name,
            params: func.params,
            reg_count: func.locals.len() + 1,
            blocks,
            span: func.span,
        }
    }

    fn lower_block(&mut self, block: MirBlock) -> LirBlock {
        let mut insts = vec![];

        for inst in &block.insts {
            match inst {
                MirInst::Assign { dest, value, span } => {
                    self.lower_assign(dest, value, *span, &mut insts);
                }
                MirInst::Call { dest, callee, args, span } => {
                    let mut lir = LirInst::new(LirOpcode::Call)
                        .with_op(LirOperand::Label(callee.clone()))
                        .with_debug(*span);
                    
                    if let Some(&cc) = self.externs.get(callee) {
                        lir = lir.with_call_conv(cc);
                    }

                    for arg in args {
                        lir = lir.with_op(LirOperand::Reg(*arg));
                    }
                    if let Some(d) = dest {
                        lir = lir.with_dest(*d);
                    }
                    insts.push(lir);
                }
            }
        }

        // Lower terminator
        match &block.terminator {
            MirTerminator::Return { value, span } => {
                if let Some(v) = value {
                    insts.push(
                        LirInst::new(LirOpcode::Ret)
                            .with_op(LirOperand::Reg(*v))
                            .with_debug(*span),
                    );
                } else {
                    insts.push(
                        LirInst::new(LirOpcode::Ret).with_debug(*span),
                    );
                }
            }
            MirTerminator::Jump { target, span } => {
                insts.push(
                    LirInst::new(LirOpcode::Jmp)
                        .with_op(LirOperand::Label(format!("block_{target}")))
                        .with_debug(*span),
                );
            }
            MirTerminator::Branch {
                cond, then, else_, span
            } => {
                insts.push(
                    LirInst::new(LirOpcode::Br)
                        .with_op(LirOperand::Reg(*cond))
                        .with_op(LirOperand::Label(format!("block_{then}")))
                        .with_op(LirOperand::Label(format!("block_{else_}")))
                        .with_debug(*span),
                );
            }
            MirTerminator::Unreachable => {}
        }

        for inst in &mut insts {
            inst.file_id = self.current_file_id;
        }

        LirBlock {
            id: block.id,
            name: block.name,
            insts,
            span: block.span,
        }
    }

    fn lower_assign(&mut self, dest: &usize, value: &MirValue, span: Span, insts: &mut Vec<LirInst>) {
        match value {
            MirValue::Local(src) => {
                insts.push(
                    LirInst::new(LirOpcode::Mov)
                        .with_dest(*dest)
                        .with_op(LirOperand::Reg(*src))
                        .with_debug(span),
                );
            }
            MirValue::Int(i) => {
                insts.push(
                    LirInst::new(LirOpcode::Mov)
                        .with_dest(*dest)
                        .with_op(LirOperand::ImmI64(*i))
                        .with_debug(span),
                );
            }
            MirValue::Float(f) => {
                insts.push(
                    LirInst::new(LirOpcode::Mov)
                        .with_dest(*dest)
                        .with_op(LirOperand::ImmF64(*f))
                        .with_debug(span),
                );
            }
            MirValue::Bool(b) => {
                insts.push(
                    LirInst::new(LirOpcode::Mov)
                        .with_dest(*dest)
                        .with_op(LirOperand::ImmI64(if *b { 1 } else { 0 }))
                        .with_debug(span),
                );
            }
            MirValue::String(s) => {
                let idx = self.intern_string(s);
                insts.push(
                    LirInst::new(LirOpcode::Mov)
                        .with_dest(*dest)
                        .with_op(LirOperand::StringRef(idx))
                        .with_debug(span),
                );
            }
            MirValue::BinOp { op, lhs, rhs } => {
                match op {
                    MirBinOp::Add | MirBinOp::Sub | MirBinOp::Mul | MirBinOp::Div | MirBinOp::Mod
                    | MirBinOp::And | MirBinOp::Or
                    | MirBinOp::BitAnd | MirBinOp::BitOr | MirBinOp::BitXor
                    | MirBinOp::Shl | MirBinOp::Shr => {
                        let lir_op = match op {
                            MirBinOp::Add => LirOpcode::Add,
                            MirBinOp::Sub => LirOpcode::Sub,
                            MirBinOp::Mul => LirOpcode::Mul,
                            MirBinOp::Div => LirOpcode::Div,
                            MirBinOp::Mod => LirOpcode::Mod,
                            MirBinOp::And | MirBinOp::BitAnd => LirOpcode::And,
                            MirBinOp::Or | MirBinOp::BitOr => LirOpcode::Or,
                            MirBinOp::BitXor => LirOpcode::Xor,
                            MirBinOp::Shl => LirOpcode::Shl,
                            MirBinOp::Shr => LirOpcode::Shr,
                            _ => unreachable!(),
                        };
                        insts.push(
                            LirInst::new(lir_op)
                                .with_dest(*dest)
                                .with_op(LirOperand::Reg(*lhs))
                                .with_op(LirOperand::Reg(*rhs))
                                .with_debug(span),
                        );
                    }
                    MirBinOp::Eq | MirBinOp::Ne
                    | MirBinOp::Lt | MirBinOp::Le
                    | MirBinOp::Gt | MirBinOp::Ge => {
                        let set_op = match op {
                            MirBinOp::Eq => LirOpcode::SetEq,
                            MirBinOp::Ne => LirOpcode::SetNe,
                            MirBinOp::Lt => LirOpcode::SetLt,
                            MirBinOp::Le => LirOpcode::SetLe,
                            MirBinOp::Gt => LirOpcode::SetGt,
                            MirBinOp::Ge => LirOpcode::SetGe,
                            _ => unreachable!(),
                        };
                        insts.push(
                            LirInst::new(LirOpcode::Cmp)
                                .with_op(LirOperand::Reg(*lhs))
                                .with_op(LirOperand::Reg(*rhs))
                                .with_debug(span),
                        );
                        insts.push(
                            LirInst::new(set_op)
                                .with_dest(*dest)
                                .with_debug(span),
                        );
                    }
                }
            }
            MirValue::UnOp { op, expr } => {
                let lir_op = match op {
                    MirUnOp::Neg => LirOpcode::Neg,
                    MirUnOp::Not => LirOpcode::Not,
                    MirUnOp::BitNot => LirOpcode::Not, // TODO: BitNot
                };
                insts.push(
                    LirInst::new(lir_op)
                        .with_dest(*dest)
                        .with_op(LirOperand::Reg(*expr))
                        .with_debug(span),
                );
            }
            MirValue::GetField { object, name: _, field } => {
                insts.push(
                    LirInst::new(LirOpcode::GetField)
                        .with_dest(*dest)
                        .with_op(LirOperand::Reg(*object))
                        .with_op(LirOperand::Field(field.clone()))
                        .with_debug(span),
                );
            }
            MirValue::StructInit { name, fields } => {
                let mut lir = LirInst::new(LirOpcode::StructInit)
                    .with_dest(*dest)
                    .with_op(LirOperand::Label(name.clone()))
                    .with_debug(span);
                for (fname, fval) in fields {
                    lir = lir.with_op(LirOperand::Field(fname.clone()));
                    lir = lir.with_op(LirOperand::Reg(*fval));
                }
                insts.push(lir);
            }
            MirValue::SetField { object, field, value } => {
                insts.push(
                    LirInst::new(LirOpcode::SetField)
                        .with_dest(*dest)
                        .with_op(LirOperand::Reg(*object))
                        .with_op(LirOperand::Field(field.clone()))
                        .with_op(LirOperand::Reg(*value))
                        .with_debug(span),
                );
            }
        }
    }
}
