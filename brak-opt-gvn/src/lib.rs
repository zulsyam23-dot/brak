use std::collections::HashMap;
use brak_core::Result;
use brak_ir_lir::lir::{LirProgram, LirFunction, LirInst, LirOpcode, LirOperand};
use brak_opt_traits::LirOptimizationPass;

pub struct GlobalValueNumbering;

#[derive(Debug, PartialEq, Eq, Hash)]
enum Value {
    BinOp(LirOpcode, LirOperand, LirOperand),
    UnaryOp(LirOpcode, LirOperand),
}

impl LirOptimizationPass for GlobalValueNumbering {
    fn name(&self) -> &'static str {
        "gvn"
    }

    fn run(&self, mut program: LirProgram) -> Result<LirProgram> {
        for func in &mut program.functions {
            optimize_function(func);
        }
        Ok(program)
    }
}

fn optimize_function(func: &mut LirFunction) {
    let mut value_table: HashMap<Value, usize> = HashMap::new();
    let mut reg_map: HashMap<usize, usize> = HashMap::new();

    // Global Value Numbering (GVN) Sederhana:
    // Identifikasi ekspresi redundan di seluruh fungsi.
    // Karena Brak LIR belum menggunakan SSA secara penuh (phi nodes),
    // kita harus berhati-hati dengan register yang ditulis ulang.
    
    // Pass 1: Identifikasi register yang menampung hasil ekspresi yang sama
    for block in &mut func.blocks {
        for inst in &mut block.insts {
            // Identifikasi ekspresi biner/unary
            let expr = match inst.opcode {
                op @ (LirOpcode::Add | LirOpcode::Sub | LirOpcode::Mul | LirOpcode::And | LirOpcode::Or | LirOpcode::Xor | LirOpcode::Shl | LirOpcode::Shr) => {
                    if let [lhs, rhs] = &inst.operands[..] {
                        Some(Value::BinOp(op, lhs.clone(), rhs.clone()))
                    } else { None }
                }
                op @ (LirOpcode::Neg | LirOpcode::Not) => {
                    if let [operand] = &inst.operands[..] {
                        Some(Value::UnaryOp(op, operand.clone()))
                    } else { None }
                }
                _ => None,
            };

            if let (Some(value), Some(dest)) = (expr, inst.dest) {
                if let Some(&prev_reg) = value_table.get(&value) {
                    // Ganti dengan Mov dari register yang sudah ada
                    *inst = LirInst::new(LirOpcode::Mov)
                        .with_dest(dest)
                        .with_op(LirOperand::Reg(prev_reg));
                    reg_map.insert(dest, prev_reg);
                } else {
                    value_table.insert(value, dest);
                }
            } else if let (LirOpcode::Mov, Some(dest), [LirOperand::Reg(src)]) = (inst.opcode, inst.dest, &inst.operands[..]) {
                reg_map.insert(dest, *src);
            }
        }
    }

    // Pass 2: Terapkan canonical registers ke semua instruksi
    for block in &mut func.blocks {
        for inst in &mut block.insts {
            for op in &mut inst.operands {
                if let LirOperand::Reg(r) = op {
                    let mut current_r = *r;
                    while let Some(&canonical) = reg_map.get(&current_r) {
                        if canonical == current_r { break; }
                        current_r = canonical;
                    }
                    if current_r != *r {
                        *op = LirOperand::Reg(current_r);
                    }
                }
            }
        }
    }
}
