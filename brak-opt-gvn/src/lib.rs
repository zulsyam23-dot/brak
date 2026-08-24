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

// ponytail: per-block CSE instead of dominator-based GVN — cross-block
// redundancy stays unexploited until SSA lands in LIR.
fn optimize_function(func: &mut LirFunction) {
    for block in &mut func.blocks {
        // value -> register holding it, valid only up to the next write
        // of any register it depends on. Without SSA we cannot know if a
        // cached value dominates later uses, so scope everything to one block.
        let mut value_table: HashMap<Value, usize> = HashMap::new();

        for inst in &mut block.insts {
            // A write to `d` invalidates: (a) values cached in d,
            // (b) values computed FROM d (their operands just changed meaning).
            if let Some(d) = inst.dest {
                value_table.retain(|v, held| {
                    *held != d && !value_uses_reg(v, d)
                });
            }

            let expr = match inst.opcode {
                op @ (LirOpcode::Add | LirOpcode::Sub | LirOpcode::Mul | LirOpcode::And | LirOpcode::Or | LirOpcode::Xor | LirOpcode::Shl | LirOpcode::Shr) => {
                    if let [lhs, rhs] = &inst.operands[..] {
                        // BUG-M12: commutative ops canonicalize operand order so
                        // `a+b` and `b+a` share a value number. Sub/Shl/Shr are
                        // NOT commutative — order preserved.
                        let (lhs, rhs) = if matches!(op,
                            LirOpcode::Add | LirOpcode::Mul | LirOpcode::And | LirOpcode::Or | LirOpcode::Xor)
                            && operand_rank(rhs) < operand_rank(lhs)
                        {
                            (rhs.clone(), lhs.clone())
                        } else {
                            (lhs.clone(), rhs.clone())
                        };
                        Some(Value::BinOp(op, lhs, rhs))
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
                    // Redundant computation: reuse the previous result.
                    *inst = LirInst::new(LirOpcode::Mov)
                        .with_dest(dest)
                        .with_op(LirOperand::Reg(prev_reg));
                    value_table.insert(value, dest);
                } else {
                    value_table.insert(value, dest);
                }
            }
        }
    }
}

fn value_uses_reg(v: &Value, reg: usize) -> bool {
    fn op_is_reg(op: &LirOperand, reg: usize) -> bool {
        matches!(op, LirOperand::Reg(r) if *r == reg)
    }
    match v {
        Value::BinOp(_, a, b) => op_is_reg(a, reg) || op_is_reg(b, reg),
        Value::UnaryOp(_, a) => op_is_reg(a, reg),
    }
}

/// Deterministic ordering key for canonicalizing commutative operands.
/// Immediates sort before registers so `Add 5, %r1` == `Add %r1, 5`.
fn operand_rank(op: &LirOperand) -> (u8, i64) {
    match op {
        LirOperand::ImmI64(v) => (0, *v),
        LirOperand::Reg(r) => (1, *r as i64),
        _ => (2, 0),
    }
}
