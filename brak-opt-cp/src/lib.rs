use std::collections::HashMap;
use brak_core::Result;
use brak_ir_lir::lir::{LirProgram, LirFunction, LirInst, LirOpcode, LirOperand};
use brak_opt_traits::LirOptimizationPass;

pub struct ConstantPropagation;

impl LirOptimizationPass for ConstantPropagation {
    fn name(&self) -> &'static str {
        "cp"
    }

    fn run(&self, mut program: LirProgram) -> Result<LirProgram> {
        for func in &mut program.functions {
            optimize_function(func);
        }
        Ok(program)
    }
}

fn optimize_function(func: &mut LirFunction) {
    // Global Constant Propagation — satu pass global
    let mut global_constants: HashMap<usize, LirOperand> = HashMap::new();
    let mut ambiguous_regs = std::collections::HashSet::new();

    // Pass 1: Identifikasi register yang konstan di seluruh fungsi
    for block in &func.blocks {
        for inst in &block.insts {
            if let Some(dest) = inst.dest {
                match inst.opcode {
                    LirOpcode::Mov => {
                        if let Some(LirOperand::ImmI64(val)) = inst.operands.first() {
                            if !ambiguous_regs.contains(&dest) {
                                if let Some(prev) = global_constants.get(&dest) {
                                    if let LirOperand::ImmI64(prev_val) = prev {
                                        if *prev_val != *val {
                                            global_constants.remove(&dest);
                                            ambiguous_regs.insert(dest);
                                        }
                                    }
                                } else {
                                    global_constants.insert(dest, LirOperand::ImmI64(*val));
                                }
                            }
                        } else {
                            global_constants.remove(&dest);
                            ambiguous_regs.insert(dest);
                        }
                    }
                    _ => {
                        global_constants.remove(&dest);
                        ambiguous_regs.insert(dest);
                    }
                }
            }
        }
    }

    // Pass 2: Terapkan konstanta dan lakukan folding
    for block in &mut func.blocks {
        for inst in &mut block.insts {
            for op in &mut inst.operands {
                if let LirOperand::Reg(r) = op {
                    if let Some(const_val) = global_constants.get(r) {
                        *op = const_val.clone();
                    }
                }
            }

            match inst.opcode {
                LirOpcode::Add => {
                    if let (Some(dest), [LirOperand::ImmI64(a), LirOperand::ImmI64(b)]) = (inst.dest, &inst.operands[..]) {
                        *inst = LirInst::new(LirOpcode::Mov).with_dest(dest).with_op(LirOperand::ImmI64(a + b));
                    }
                }
                LirOpcode::Sub => {
                    if let (Some(dest), [LirOperand::ImmI64(a), LirOperand::ImmI64(b)]) = (inst.dest, &inst.operands[..]) {
                        *inst = LirInst::new(LirOpcode::Mov).with_dest(dest).with_op(LirOperand::ImmI64(a - b));
                    }
                }
                LirOpcode::Mul => {
                    if let (Some(dest), [LirOperand::ImmI64(a), LirOperand::ImmI64(b)]) = (inst.dest, &inst.operands[..]) {
                        *inst = LirInst::new(LirOpcode::Mov).with_dest(dest).with_op(LirOperand::ImmI64(a * b));
                    }
                }
                _ => {}
            }
        }
    }
}
