/// Loop Invariant Code Motion (LICM) untuk LIR.
///
/// 1. Deteksi natural loop via CFG analysis
/// 2. Identifikasi instruksi loop-invariant (fixed-point iteration)
/// 3. Hoist invariant instructions ke pre-header block

use std::collections::HashSet;
use brak_core::Result;
use brak_ir_lir::lir::{LirProgram, LirFunction, LirInst, LirOpcode, LirOperand, VirtReg, BlockId};
use brak_opt_traits::LirOptimizationPass;
use brak_opt_utils::{build_cfg, compute_dominance, dominates, find_natural_loops, CfgGraph, NaturalLoop};

pub struct Licm;

impl LirOptimizationPass for Licm {
    fn name(&self) -> &'static str { "licm" }

    fn run(&self, mut program: LirProgram) -> Result<LirProgram> {
        for func in &mut program.functions {
            apply_licm(func);
        }
        Ok(program)
    }
}

fn apply_licm(func: &mut LirFunction) {
    let cfg = build_cfg(func);
    let dom = compute_dominance(func, &cfg);
    let loops = find_natural_loops(func, &cfg, &dom);
    if loops.is_empty() { return; }

    // Process from innermost loop outward
    let mut sorted = loops.clone();
    sorted.sort_by_key(|l| l.body.len());

    for lp in &sorted {
        hoist_invariants(func, &cfg, lp);
    }
}

fn hoist_invariants(func: &mut LirFunction, cfg: &CfgGraph, lp: &NaturalLoop) {
    let header = lp.header;

    // Find pre-header: predecessor(s) of header NOT in loop body.
    let outside_preds: Vec<BlockId> = cfg.predecessors.get(&header)
        .map(|p| p.iter().copied().filter(|b| !lp.body.contains(b)).collect())
        .unwrap_or_default();
    // BUG-K05: with multiple outside predecessors there is no single pre-header
    // that dominates the header — hoisting there would execute code on paths
    // that never enter the loop (or miss domination entirely). Skip instead.
    if outside_preds.len() != 1 { return; }
    let pre_header = outside_preds[0];

    let dom = compute_dominance(func, cfg);
    if !dominates(&dom, pre_header, header) { return; }

    // Fixed-point: find all loop-invariant instructions
    // An instruction is invariant if all its register operands are defined
    // outside the loop OR by other loop-invariant instructions.
    let mut loop_defs: HashSet<VirtReg> = HashSet::new();
    for &bid in &lp.body {
        if let Some(blk) = func.blocks.iter().find(|b| b.id == bid) {
            for inst in &blk.insts {
                if let Some(d) = inst.dest { loop_defs.insert(d); }
            }
        }
    }

    let mut invariants: Vec<(BlockId, Vec<usize>)> = Vec::new(); // per-block list of invariant indices
    let mut changed = true;
    let mut removed: HashSet<VirtReg> = HashSet::new();

    while changed {
        changed = false;
        for &bid in &lp.body {
            let block_idx = func.blocks.iter().position(|b| b.id == bid).unwrap();
            let found = invariants.iter().find(|(id, _)| *id == bid);
            let mut known: HashSet<usize> = found.map(|(_, v)| v.iter().copied().collect()).unwrap_or_default();

            let blk = &func.blocks[block_idx];
            for (ii, inst) in blk.insts.iter().enumerate() {
                if known.contains(&ii) { continue; }
                if !is_eligible(inst) { continue; }

                if inst.operands.iter().all(|op| {
                    match op {
                        LirOperand::Reg(r) => !loop_defs.contains(r) || removed.contains(r),
                        _ => true, // immediate/string/stackslot is always invariant
                    }
                }) {
                    known.insert(ii);
                    if let Some(d) = inst.dest { removed.insert(d); }
                    changed = true;
                }
            }

            let new_set: Vec<usize> = known.into_iter().collect();
            if let Some(existing) = invariants.iter_mut().find(|(id, _)| *id == bid) {
                existing.1 = new_set;
            } else {
                invariants.push((bid, new_set));
            }
        }
    }

    if invariants.is_empty() { return; }

    // Hoist: remove invariants from loop blocks, add to pre-header
    let pre_idx = func.blocks.iter().position(|b| b.id == pre_header).unwrap();
    let jmp_pos = {
        let pre = &func.blocks[pre_idx];
        pre.insts.iter().rposition(|i| matches!(i.opcode, LirOpcode::Jmp | LirOpcode::Br))
            .unwrap_or(pre.insts.len())
    };
    let mut hoisted_insts: Vec<LirInst> = Vec::new();

    // Process blocks in reverse order to preserve indices when removing
    for (bid, indices) in invariants.iter() {
        let mut sorted_idx = indices.clone();
        sorted_idx.sort_unstable_by(|a, b| b.cmp(a)); // descending
        let block_idx = func.blocks.iter().position(|b| b.id == *bid).unwrap();

        for &ii in &sorted_idx {
            let inst = func.blocks[block_idx].insts.remove(ii);
            hoisted_insts.push(inst);
        }
    }

    // Insert hoisted instructions into pre-header, before the terminator
    hoisted_insts.reverse();
    for inst in hoisted_insts {
        func.blocks[pre_idx].insts.insert(jmp_pos, inst);
    }
}

/// BUG-K05: strict WHITELIST of side-effect-free, non-trapping instructions.
///
/// The old blacklist omitted `Div`/`Mod` (hoisting them out of a loop that may
/// run zero times introduces a divide-by-zero trap), `Load`, and `SetField`
/// (a mutation executed once instead of once per iteration).
///
/// NOTE: `Cmp`/`Set*` are deliberately excluded — `Set*` consumes implicit
/// flag state produced by a preceding `Cmp`; hoisting one without the other
/// (or interleaved with the loop's own comparisons) corrupts branch decisions.
fn is_eligible(inst: &LirInst) -> bool {
    matches!(inst.opcode,
        LirOpcode::Mov
        | LirOpcode::Add | LirOpcode::Sub | LirOpcode::Mul
        | LirOpcode::And | LirOpcode::Or | LirOpcode::Xor
        | LirOpcode::Shl | LirOpcode::Shr
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use brak_core::span::DUMMY_SPAN;
    use brak_ir_lir::lir::*;

    fn mov(d: VirtReg, o: LirOperand) -> LirInst {
        LirInst::new(LirOpcode::Mov).with_dest(d).with_op(o)
    }

    fn make_loop_with_invariant() -> LirFunction {
        LirFunction {
            name: "test".into(), params: vec![], reg_count: 3,
            blocks: vec![
                LirBlock { id: 0, name: "entry".into(),
                    insts: vec![
                        mov(0, LirOperand::ImmI64(0)),
                        LirInst::new(LirOpcode::Jmp).with_op(LirOperand::Label("header".into())),
                    ], span: DUMMY_SPAN },
                LirBlock { id: 1, name: "header".into(),
                    insts: vec![
                        mov(1, LirOperand::ImmI64(10)),
                        LirInst::new(LirOpcode::Br)
                            .with_op(LirOperand::Reg(1))
                            .with_op(LirOperand::Label("body".into()))
                            .with_op(LirOperand::Label("exit".into())),
                    ], span: DUMMY_SPAN },
                LirBlock { id: 2, name: "body".into(),
                    insts: vec![
                        mov(2, LirOperand::ImmI64(42)),      // INVARIANT
                        LirInst::new(LirOpcode::Jmp).with_op(LirOperand::Label("header".into())),
                    ], span: DUMMY_SPAN },
                LirBlock { id: 3, name: "exit".into(),
                    insts: vec![
                        LirInst::new(LirOpcode::Ret).with_op(LirOperand::Reg(0)),
                    ], span: DUMMY_SPAN },
            ], span: DUMMY_SPAN,
        }
    }

    #[test]
    fn test_licm_hoists_invariant() {
        let mut func = make_loop_with_invariant();
        apply_licm(&mut func);

        let entry = func.blocks.iter().find(|b| b.name == "entry").unwrap();
        assert!(entry.insts.iter().any(|i| i.opcode == LirOpcode::Mov && i.dest == Some(2)),
            "Mov %r2=42 should be hoisted to entry block");

        let body = func.blocks.iter().find(|b| b.name == "body").unwrap();
        assert!(!body.insts.iter().any(|i| i.opcode == LirOpcode::Mov && i.dest == Some(2)),
            "Mov %r2=42 should be REMOVED from body block");
    }

    #[test]
    fn test_licm_keeps_variant() {
        let mut func = LirFunction {
            name: "test".into(), params: vec![], reg_count: 3,
            blocks: vec![
                LirBlock { id: 0, name: "entry".into(),
                    insts: vec![
                        mov(0, LirOperand::ImmI64(0)),
                        LirInst::new(LirOpcode::Jmp).with_op(LirOperand::Label("header".into())),
                    ], span: DUMMY_SPAN },
                LirBlock { id: 1, name: "header".into(),
                    insts: vec![
                        LirInst::new(LirOpcode::SetEq).with_dest(1)
                            .with_op(LirOperand::Reg(0)).with_op(LirOperand::ImmI64(10)),
                        LirInst::new(LirOpcode::Br)
                            .with_op(LirOperand::Reg(1))
                            .with_op(LirOperand::Label("body".into()))
                            .with_op(LirOperand::Label("exit".into())),
                    ], span: DUMMY_SPAN },
                LirBlock { id: 2, name: "body".into(),
                    insts: vec![
                        mov(2, LirOperand::Reg(0)),  // VARIANT: uses %r0 which is defined OUTSIDE, BUT...
                        LirInst::new(LirOpcode::Add).with_dest(0) // ...%r0 is also MODIFIED inside loop!
                            .with_op(LirOperand::Reg(0)).with_op(LirOperand::ImmI64(1)),
                        LirInst::new(LirOpcode::Jmp).with_op(LirOperand::Label("header".into())),
                    ], span: DUMMY_SPAN },
                LirBlock { id: 3, name: "exit".into(),
                    insts: vec![
                        LirInst::new(LirOpcode::Ret).with_op(LirOperand::Reg(2)),
                    ], span: DUMMY_SPAN },
            ], span: DUMMY_SPAN,
        };

        apply_licm(&mut func);
        let body = func.blocks.iter().find(|b| b.name == "body").unwrap();
        assert!(body.insts.iter().any(|i| i.opcode == LirOpcode::Mov && i.dest == Some(2)),
            "Variant Mov should REMAIN in body");
    }
}
