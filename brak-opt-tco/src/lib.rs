use brak_core::Result;
use brak_ir_lir::lir::{LirProgram, LirFunction, LirOpcode, LirOperand, LirInst};
use brak_opt_traits::LirOptimizationPass;

/// BUG-M01 FIXED: real tail-call optimization for DIRECT self-recursion.
///
/// Pattern  `Call <self>(args)` [`Mov d ← result`]? `Ret`
/// becomes  `Mov %param_i ← arg_i`*  +  `Jmp entry`.
///
/// Arguments are staged through freshly-allocated temporaries first so an arg
/// that reads a parameter cannot observe a partially-updated parameter set.
/// Mutual recursion and indirect tail calls are NOT handled (needs whole-program
/// analysis); they simply pass through unchanged.
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
    let entry_id = match func.blocks.first() {
        Some(b) => b.id,
        None => return,
    };
    let base_tmp = func.reg_count;

    for block_idx in 0..func.blocks.len() {
        let mut i = 0;
        while i < func.blocks[block_idx].insts.len() {
            let insts = &func.blocks[block_idx].insts;
            if insts[i].opcode != LirOpcode::Call {
                i += 1;
                continue;
            }
            // Only direct self-calls qualify.
            let is_self = matches!(insts[i].operands.first(), Some(LirOperand::Label(l)) if *l == func.name);
            if !is_self {
                i += 1;
                continue;
            }

            let tail = detect_tail_shape(insts, i);
            let Some(tail) = tail else { i += 1; continue };
            let args: Vec<LirOperand> = insts[i].operands[1..].to_vec();
            if args.len() > func.params.len() {
                i += 1;
                continue;
            }

            // Rewrite: stage args into temps, copy temps into param slots, jmp entry.
            let mut replacement: Vec<LirInst> = Vec::with_capacity(args.len() * 2 + 1);
            for (j, arg) in args.iter().enumerate() {
                let tmp = base_tmp + j;
                replacement.push(LirInst::new(LirOpcode::Mov).with_dest(tmp).with_op(arg.clone()));
            }
            for (j, param) in func.params.iter().enumerate() {
                if j >= args.len() { break; }
                replacement.push(
                    LirInst::new(LirOpcode::Mov)
                        .with_dest(*param)
                        .with_op(LirOperand::Reg(base_tmp + j)),
                );
            }
            replacement.push(
                LirInst::new(LirOpcode::Jmp)
                    .with_op(LirOperand::Label(format!("block_{entry_id}"))),
            );

            let _ = tail.ret_index; // Ret is subsumed by the Jmp
            let drain = tail.call_index..=tail.last_index;
            func.blocks[block_idx].insts.splice(drain, replacement);
            func.reg_count += args.len(); // room for staging temps
            // Restart scan for this block (indices shifted).
            i = 0;
        }
    }
}

struct TailShape {
    call_index: usize,
    last_index: usize,
    #[allow(dead_code)]
    ret_index: usize,
}

/// Recognize `Call` [+ `Mov d ← Reg(call_dest)`] + `Ret [Reg]`.
fn detect_tail_shape(insts: &[LirInst], call_idx: usize) -> Option<TailShape> {
    // Shape 1: Call; Ret
    if call_idx + 1 < insts.len() && insts[call_idx + 1].opcode == LirOpcode::Ret {
        return Some(TailShape { call_index: call_idx, last_index: call_idx + 1, ret_index: call_idx + 1 });
    }
    // Shape 2: Call d; Mov m ← Reg(d); Ret Reg(m)
    if call_idx + 2 < insts.len()
        && insts[call_idx + 1].opcode == LirOpcode::Mov
        && insts[call_idx + 2].opcode == LirOpcode::Ret
    {
        let call_dest = insts[call_idx].dest?;
        let mov = &insts[call_idx + 1];
        let mov_dest = mov.dest?;
        if !matches!(mov.operands.first(), Some(LirOperand::Reg(r)) if *r == call_dest) {
            return None;
        }
        if matches!(insts[call_idx + 2].operands.first(), Some(LirOperand::Reg(r)) if *r == mov_dest) {
            return Some(TailShape { call_index: call_idx, last_index: call_idx + 2, ret_index: call_idx + 2 });
        }
    }
    None
}
