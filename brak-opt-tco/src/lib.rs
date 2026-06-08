use brak_core::Result;
use brak_ir_lir::lir::{LirProgram, LirFunction, LirOpcode, LirOperand, LirInst};
use brak_opt_traits::LirOptimizationPass;

pub struct TailCallOptimization;

impl LirOptimizationPass for TailCallOptimization {
    fn name(&self) -> &'static str {
        "tco"
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
        let mut i = 0;
        while i < block.insts.len() {
            // Cari pola: Call, [Mov dest, result]?, Ret
            if block.insts[i].opcode == LirOpcode::Call {
                let mut found_tco = false;
                
                // Kasus 1: Call diikuti langsung oleh Ret
                if i + 1 < block.insts.len() && block.insts[i+1].opcode == LirOpcode::Ret {
                    found_tco = true;
                }
                
                // Kasus 2: Call diikuti oleh Mov (menyimpan hasil) lalu Ret
                if !found_tco && i + 2 < block.insts.len() 
                    && block.insts[i+1].opcode == LirOpcode::Mov 
                    && block.insts[i+2].opcode == LirOpcode::Ret {
                    // Pastikan yang di-return adalah hasil dari call
                    if let (Some(call_dest), Some(mov_dest)) = (block.insts[i].dest, block.insts[i+1].dest) {
                        if let Some(LirOperand::Reg(r)) = block.insts[i+1].operands.first() {
                            if *r == call_dest {
                                if let Some(LirOperand::Reg(ret_r)) = block.insts[i+2].operands.first() {
                                    if *ret_r == mov_dest {
                                        found_tco = true;
                                    }
                                }
                            }
                        }
                    }
                }

                if found_tco {
                    // Tandai sebagai tail call (menggunakan opcode Comment untuk sementara jika tidak ada TailCall)
                    // Atau kita biarkan backend yang menangani jika kita beri flag?
                    // Karena LIR tidak punya flag 'is_tail', kita bisa menggunakan Comment sebagai penanda metadata
                    block.insts.insert(i, LirInst::new(LirOpcode::Comment).with_op(LirOperand::Label("tail_call".to_string())));
                    i += 1; // Lewati comment
                }
            }
            i += 1;
        }
    }
}
