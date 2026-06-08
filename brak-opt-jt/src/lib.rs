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
    // Map: From Block ID -> To Block ID
    let mut jump_map: HashMap<usize, usize> = HashMap::new();
    
    for block in &func.blocks {
        if block.insts.len() == 1 {
            let inst = &block.insts[0];
            if inst.opcode == LirOpcode::Jmp {
                if let Some(LirOperand::Label(target)) = inst.operands.first() {
                    // Ekstrak ID dari "block_N"
                    if let Some(target_id) = target.strip_prefix("block_").and_then(|s| s.parse::<usize>().ok()) {
                        if target_id != block.id {
                            jump_map.insert(block.id, target_id);
                        }
                    }
                }
            }
        }
    }

    // Resolusi rekursif
    let mut final_jump_map: HashMap<usize, usize> = HashMap::new();
    for (&start_id, _) in &jump_map {
        let mut current_id = start_id;
        let mut visited = std::collections::HashSet::new();
        visited.insert(current_id);

        while let Some(&next_id) = jump_map.get(&current_id) {
            if !visited.insert(next_id) { break; }
            current_id = next_id;
        }
        if current_id != start_id {
            final_jump_map.insert(start_id, current_id);
        }
    }

    // 2. Terapkan pemetaan ke seluruh instruksi Jmp dan Br
    for block in &mut func.blocks {
        for inst in &mut block.insts {
            if inst.opcode == LirOpcode::Jmp || inst.opcode == LirOpcode::Br {
                for op in &mut inst.operands {
                    if let LirOperand::Label(name) = op {
                        if let Some(target_id) = name.strip_prefix("block_").and_then(|s| s.parse::<usize>().ok()) {
                            if let Some(&final_target_id) = final_jump_map.get(&target_id) {
                                *name = format!("block_{}", final_target_id);
                            }
                        }
                    }
                }
            }
        }
    }
}
