use std::collections::{HashMap, HashSet};
use brak_core::Result;
use brak_ir_lir::lir::{LirProgram, LirFunction, LirInst, LirOpcode, LirOperand, VirtReg};
use brak_opt_traits::LirOptimizationPass;

pub struct DeadCodeElimination;

impl LirOptimizationPass for DeadCodeElimination {
    fn name(&self) -> &'static str {
        "dce"
    }

    fn run(&self, mut program: LirProgram) -> Result<LirProgram> {
        remove_unused_functions(&mut program);
        for func in &mut program.functions {
            remove_dead_instructions(func);
        }
        Ok(program)
    }
}

/// Phase 1: Hapus fungsi yang tidak pernah dipanggil
fn remove_unused_functions(program: &mut LirProgram) {
    let mut used = HashSet::new();
    used.insert("main".to_string());

    for func in &program.functions {
        for block in &func.blocks {
            for inst in &block.insts {
                if let LirOpcode::Call = inst.opcode {
                    if let Some(LirOperand::Label(name)) = inst.operands.first() {
                        used.insert(name.clone());
                    }
                }
            }
        }
    }

    program.functions.retain(|f| used.contains(&f.name));
}

/// Phase 2: Hapus instruksi dead (dest register tidak pernah dipakai) dalam setiap fungsi
fn remove_dead_instructions(func: &mut LirFunction) {
    // Build CFG: successors for each block
    let successors = compute_successors(func);

    // Live-out for each block: registers used by successors
    let live_out = compute_live_out(func, &successors);

    for bi in 0..func.blocks.len() {
        let block = &func.blocks[bi];
        let mut live: HashSet<VirtReg> = live_out.get(&bi).cloned().unwrap_or_default();
        let mut kept = Vec::with_capacity(block.insts.len());

        // Scan reverse untuk deteksi dead instructions
        for inst in block.insts.iter().rev() {
            let dest = inst.dest;

            // Apakah instruksi ini bisa dihapus?
            if let Some(d) = dest {
                if !live.contains(&d) && !has_side_effect(inst) {
                    // Skip dead instruction — jangan di-push ke kept
                    // Tapi tetap update liveness dari operands
                    let srcs = source_regs(inst);
                    for r in srcs {
                        live.insert(r);
                    }
                    continue;
                }
            }

            // Instruction is live: update liveness then keep it
            let srcs = source_regs(inst);
            for r in srcs {
                live.insert(r);
            }
            if let Some(d) = dest {
                live.remove(&d);
            }
            kept.push(inst.clone());
        }

        kept.reverse();
        func.blocks[bi].insts = kept;
    }
}

/// Cari successor blocks: kemana Jmp/Br menunjuk.
/// Returns successors keyed by VEC INDEX (not block id) so downstream
/// liveness dataflow (which enumerates blocks) stays consistent.
fn compute_successors(func: &LirFunction) -> HashMap<usize, Vec<usize>> {
    let mut succ: HashMap<usize, Vec<usize>> = HashMap::new();
    // LIR branch labels are "block_{id}"; block names are arbitrary
    // ("entry", "while_cond", ...). Resolve by id first, name second.
    let block_ids: HashSet<usize> = func.blocks.iter().map(|b| b.id).collect();
    let block_map: HashMap<String, usize> = func.blocks.iter()
        .map(|b| (b.name.clone(), b.id))
        .collect();
    // id -> vec index mapping (BUG-K06 family: ids and indices diverge after
    // inlining/optimization; mixing them silently corrupts liveness)
    let id_to_idx: HashMap<usize, usize> =
        func.blocks.iter().enumerate().map(|(i, b)| (b.id, i)).collect();
    let resolve = |name: &str| -> Option<usize> {
        let target_id = if let Some(id) = name.strip_prefix("block_").and_then(|s| s.parse::<usize>().ok()) {
            if block_ids.contains(&id) { Some(id) } else { None }
        } else {
            block_map.get(name).copied()
        }?;
        id_to_idx.get(&target_id).copied()
    };

    for (bi, block) in func.blocks.iter().enumerate() {
        let edges = &mut succ.entry(bi).or_default();
        for inst in &block.insts {
            match inst.opcode {
                LirOpcode::Jmp => {
                    if let Some(LirOperand::Label(name)) = inst.operands.first() {
                        if let Some(target) = resolve(name) {
                            edges.push(target);
                        }
                    }
                }
                LirOpcode::Br => {
                    for op in &inst.operands {
                        if let LirOperand::Label(name) = op {
                            if let Some(target) = resolve(name) {
                                edges.push(target);
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        // If no terminator (and not an explicit Return), next block is
        // the fall-through successor.
        if edges.is_empty()
            && !block.insts.iter().any(|i| i.opcode == LirOpcode::Ret)
            && bi + 1 < func.blocks.len()
        {
            edges.push(bi + 1);
        }
    }
    succ
}

/// Compute live-out: registers used by successor blocks
fn compute_live_out(func: &LirFunction, successors: &HashMap<usize, Vec<usize>>) -> HashMap<usize, HashSet<VirtReg>> {
    // Block-level register usage summary
    let mut block_uses: HashMap<usize, HashSet<VirtReg>> = HashMap::new();
    let mut block_defs: HashMap<usize, HashSet<VirtReg>> = HashMap::new();

    for (bi, block) in func.blocks.iter().enumerate() {
        let mut uses = HashSet::new();
        let mut defs = HashSet::new();
        for inst in &block.insts {
            for r in source_regs(inst) {
                if !defs.contains(&r) {
                    uses.insert(r);
                }
            }
            if let Some(d) = inst.dest {
                defs.insert(d);
            }
        }
        block_uses.insert(bi, uses);
        block_defs.insert(bi, defs);
    }

    // Simple iterative dataflow: live-out = union of (uses ∪ (live-out - defs)) for each successor
    let mut live_out: HashMap<usize, HashSet<VirtReg>> = HashMap::new();
    let mut changed = true;
    while changed {
        changed = false;
        for bi in 0..func.blocks.len() {
            let mut new_live: HashSet<VirtReg> = HashSet::new();
            if let Some(succs) = successors.get(&bi) {
                for &s in succs {
                    let s_uses = block_uses.get(&s).cloned().unwrap_or_default();
                    let s_defs = block_defs.get(&s).cloned().unwrap_or_default();
                    let s_live_out = live_out.get(&s).cloned().unwrap_or_default();

                    // live-in of successor = uses ∪ (live-out - defs)
                    let mut live_in = s_live_out.difference(&s_defs).copied().collect::<HashSet<_>>();
                    live_in.extend(&s_uses);
                    new_live.extend(&live_in);
                }
            }
            if new_live != *live_out.get(&bi).unwrap_or(&HashSet::new()) {
                live_out.insert(bi, new_live);
                changed = true;
            }
        }
    }
    live_out
}

/// Apakah instruksi punya side effect (tidak bisa dihapus walau dest-nya dead)?
/// BUG-M10: `Div`/`Mod` ditambahkan — trap-nya (divide-by-zero) observable,
/// menghapusnya mengubah semantik program.
fn has_side_effect(inst: &LirInst) -> bool {
    matches!(inst.opcode,
        LirOpcode::Call
        | LirOpcode::Store
        | LirOpcode::Ret
        | LirOpcode::Jmp
        | LirOpcode::Br
        | LirOpcode::Push
        | LirOpcode::Pop
        | LirOpcode::Comment
        | LirOpcode::Div
        | LirOpcode::Mod
    )
}

/// Collect all register operands that are SOURCES (read, not written)
fn source_regs(inst: &LirInst) -> Vec<VirtReg> {
    let mut regs = Vec::new();
    for op in &inst.operands {
        if let LirOperand::Reg(r) = op {
            regs.push(*r);
        }
    }
    // For Call, the first operand is a label, remaining are source regs
    regs
}

#[cfg(test)]
mod tests {
    use super::*;
    use brak_ir_lir::lir::*;
    use brak_core::span::DUMMY_SPAN;

    fn mov(dest: VirtReg, op: LirOperand) -> LirInst {
        LirInst::new(LirOpcode::Mov).with_dest(dest).with_op(op)
    }

    #[test]
    fn test_simple_dead_mov() {
        let mut func = LirFunction {
            name: "test".into(),
            params: vec![],
            reg_count: 2,
            blocks: vec![LirBlock {
                id: 0,
                name: "b0".into(),
                insts: vec![
                    mov(0, LirOperand::ImmI64(42)),
                    mov(1, LirOperand::ImmI64(7)),
                    LirInst::new(LirOpcode::Ret).with_op(LirOperand::Reg(0)),
                ],
                span: DUMMY_SPAN,
            }],
            span: DUMMY_SPAN,
        };

        remove_dead_instructions(&mut func);

        assert_eq!(func.blocks[0].insts.len(), 2);
        assert_eq!(func.blocks[0].insts[0].opcode, LirOpcode::Mov);
        assert_eq!(func.blocks[0].insts[0].dest, Some(0));
        assert_eq!(func.blocks[0].insts[1].opcode, LirOpcode::Ret);
    }

    #[test]
    fn test_keep_side_effects() {
        let mut func = LirFunction {
            name: "test".into(),
            params: vec![],
            reg_count: 2,
            blocks: vec![LirBlock {
                id: 0,
                name: "b0".into(),
                insts: vec![
                    mov(0, LirOperand::ImmI64(42)),
                    LirInst::new(LirOpcode::Call).with_dest(1)
                        .with_op(LirOperand::Label("g".into()))
                        .with_op(LirOperand::Reg(0)),
                    LirInst::new(LirOpcode::Ret).with_op(LirOperand::Reg(0)),
                ],
                span: DUMMY_SPAN,
            }],
            span: DUMMY_SPAN,
        };

        remove_dead_instructions(&mut func);

        assert_eq!(func.blocks[0].insts.len(), 3);
    }

    #[test]
    fn test_multi_block_liveness() {
        let mut func = LirFunction {
            name: "test".into(),
            params: vec![],
            reg_count: 2,
            blocks: vec![
                LirBlock {
                    id: 0,
                    name: "b0".into(),
                    insts: vec![
                        mov(0, LirOperand::ImmI64(42)),
                        mov(1, LirOperand::ImmI64(7)),
                        LirInst::new(LirOpcode::Jmp).with_op(LirOperand::Label("b1".into())),
                    ],
                    span: DUMMY_SPAN,
                },
                LirBlock {
                    id: 1,
                    name: "b1".into(),
                    insts: vec![
                        LirInst::new(LirOpcode::Ret).with_op(LirOperand::Reg(0)),
                    ],
                    span: DUMMY_SPAN,
                },
            ],
            span: DUMMY_SPAN,
        };

        remove_dead_instructions(&mut func);

        assert_eq!(func.blocks[0].insts.len(), 2);
        assert_eq!(func.blocks[0].insts[0].opcode, LirOpcode::Mov);
        assert_eq!(func.blocks[0].insts[0].dest, Some(0));
        assert_eq!(func.blocks[0].insts[1].opcode, LirOpcode::Jmp);
    }
}
