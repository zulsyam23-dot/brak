use brak_core::Result;
use brak_ir_lir::lir::{LirProgram, LirFunction, LirOpcode, LirOperand};
use brak_opt_traits::LirOptimizationPass;
use std::collections::HashMap;

pub struct JumpThreading;

impl LirOptimizationPass for JumpThreading {
    fn name(&self) -> &'static str {
        "jt"
    }

    fn run(&self, mut program: LirProgram) -> Result<LirProgram> {
        for func in &mut program.functions {
            optimize_function(func);
        }
        Ok(program)
    }
}

fn optimize_function(func: &mut LirFunction) {
    // 1. Identifikasi "Empty Jump Blocks" (blok yang hanya berisi Jmp)
    let mut jump_map: HashMap<String, String> = HashMap::new();
    
    for block in &func.blocks {
        if block.insts.len() == 1 {
            let inst = &block.insts[0];
            if inst.opcode == LirOpcode::Jmp {
                if let Some(LirOperand::Label(target)) = inst.operands.first() {
                    // Jangan memetakan ke diri sendiri (loop tak hingga)
                    if target != &block.name {
                        jump_map.insert(block.name.clone(), target.clone());
                    }
                }
            }
        }
    }

    // Resolusi rekursif (jika A -> B dan B -> C, maka A -> C)
    let mut final_jump_map: HashMap<String, String> = HashMap::new();
    for (start, _) in &jump_map {
        let mut current = start;
        let mut visited = std::collections::HashSet::new();
        visited.insert(current.clone());

        while let Some(next) = jump_map.get(current) {
            if !visited.insert(next.clone()) { break; } // Siklus terdeteksi
            current = next;
        }
        if current != start {
            final_jump_map.insert(start.clone(), current.clone());
        }
    }

    // 2. Terapkan pemetaan ke seluruh instruksi Jmp dan Br
    for block in &mut func.blocks {
        for inst in &mut block.insts {
            if inst.opcode == LirOpcode::Jmp || inst.opcode == LirOpcode::Br {
                for op in &mut inst.operands {
                    if let LirOperand::Label(name) = op {
                        if let Some(final_target) = final_jump_map.get(name) {
                            *name = final_target.clone();
                        }
                    }
                }
            }
        }
    }
}
