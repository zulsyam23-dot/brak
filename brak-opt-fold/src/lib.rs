use brak_core::Result;
use brak_ir_lir::lir::{LirProgram, LirFunction, LirOpcode, LirOperand, LirInst};
use brak_opt_traits::LirOptimizationPass;

pub struct ConstantFolding;

impl LirOptimizationPass for ConstantFolding {
    fn name(&self) -> &'static str {
        "fold"
    }

    fn run(&self, mut program: LirProgram) -> Result<LirProgram> {
        for func in &mut program.functions {
            optimize_function(func);
        }
        Ok(program)
    }
}

fn optimize_function(func: &mut LirFunction) {
    for block in &mut func.blocks {
        for inst in &mut block.insts {
            match inst.opcode {
                LirOpcode::Add => {
                    if let (Some(dest), [LirOperand::ImmI64(a), LirOperand::ImmI64(b)]) = (inst.dest, &inst.operands[..]) {
                        *inst = LirInst::new(LirOpcode::Mov).with_dest(dest).with_op(LirOperand::ImmI64(a.wrapping_add(*b)));
                    } else if let [_, LirOperand::ImmI64(0)] = &inst.operands[..] {
                        inst.opcode = LirOpcode::Mov;
                        inst.operands.pop(); // x + 0 -> x
                    } else if let [LirOperand::ImmI64(0), second] = &inst.operands[..] {
                        // BUG-M11: commuted identity 0 + x -> x
                        if let Some(d) = inst.dest {
                            let x = second.clone();
                            *inst = LirInst::new(LirOpcode::Mov).with_dest(d).with_op(x);
                        }
                    }
                }
                LirOpcode::Sub => {
                    if let (Some(dest), [LirOperand::ImmI64(a), LirOperand::ImmI64(b)]) = (inst.dest, &inst.operands[..]) {
                        *inst = LirInst::new(LirOpcode::Mov).with_dest(dest).with_op(LirOperand::ImmI64(a.wrapping_sub(*b)));
                    } else if let [_, LirOperand::ImmI64(0)] = &inst.operands[..] {
                        inst.opcode = LirOpcode::Mov;
                        inst.operands.pop(); // x - 0 -> x
                    }
                }
                LirOpcode::Mul => {
                    if let (Some(dest), [LirOperand::ImmI64(a), LirOperand::ImmI64(b)]) = (inst.dest, &inst.operands[..]) {
                        *inst = LirInst::new(LirOpcode::Mov).with_dest(dest).with_op(LirOperand::ImmI64(a.wrapping_mul(*b)));
                    } else if let [_, LirOperand::ImmI64(1)] = &inst.operands[..] {
                        inst.opcode = LirOpcode::Mov;
                        inst.operands.pop(); // x * 1 -> x
                    } else if let [LirOperand::ImmI64(1), second] = &inst.operands[..] {
                        // BUG-M11: commuted identity 1 * x -> x
                        if let Some(d) = inst.dest {
                            let x = second.clone();
                            *inst = LirInst::new(LirOpcode::Mov).with_dest(d).with_op(x);
                        }
                    } else if let (Some(dest), [LirOperand::ImmI64(0), _]) = (inst.dest, &inst.operands[..]) {
                        *inst = LirInst::new(LirOpcode::Mov).with_dest(dest).with_op(LirOperand::ImmI64(0)); // 0 * x -> 0
                    } else if let (Some(dest), [_, LirOperand::ImmI64(0)]) = (inst.dest, &inst.operands[..]) {
                        *inst = LirInst::new(LirOpcode::Mov).with_dest(dest).with_op(LirOperand::ImmI64(0)); // x * 0 -> 0
                    }
                }
                LirOpcode::And => {
                    if let (Some(dest), [LirOperand::ImmI64(a), LirOperand::ImmI64(b)]) = (inst.dest, &inst.operands[..]) {
                        *inst = LirInst::new(LirOpcode::Mov).with_dest(dest).with_op(LirOperand::ImmI64(a & b));
                    }
                }
                LirOpcode::Or => {
                    if let (Some(dest), [LirOperand::ImmI64(a), LirOperand::ImmI64(b)]) = (inst.dest, &inst.operands[..]) {
                        *inst = LirInst::new(LirOpcode::Mov).with_dest(dest).with_op(LirOperand::ImmI64(a | b));
                    }
                }
                LirOpcode::Xor => {
                    if let (Some(dest), [LirOperand::ImmI64(a), LirOperand::ImmI64(b)]) = (inst.dest, &inst.operands[..]) {
                        *inst = LirInst::new(LirOpcode::Mov).with_dest(dest).with_op(LirOperand::ImmI64(a ^ b));
                    }
                }
                _ => {}
            }
        }
    }
}
