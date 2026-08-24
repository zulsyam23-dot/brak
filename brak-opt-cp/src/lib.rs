use std::collections::{HashMap, HashSet};
use brak_core::Result;
use brak_ir_lir::lir::{LirProgram, LirFunction, LirInst, LirOpcode, LirOperand};
use brak_opt_traits::LirOptimizationPass;

pub struct ConstantPropagation;

/// Lattice value per register (BUG-K04: previously a single order-based
/// function-global map — a constant assigned on ONE branch was substituted on
/// ALL paths, silently miscompiling conditionals).
#[derive(Debug, Clone, PartialEq)]
enum Const {
    /// Register never defined yet on this path (optimistic TOP)
    Top,
    /// Same constant on all incoming paths
    Known(i64),
    /// Different constants / non-constant definitions merge here
    Ambig,
}

type State = HashMap<usize, Const>;

fn join(a: &Const, b: &Const) -> Const {
    match (a, b) {
        (Const::Top, x) => x.clone(),
        (x, Const::Top) => x.clone(),
        (Const::Known(x), Const::Known(y)) if x == y => Const::Known(*x),
        _ => Const::Ambig,
    }
}

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

fn transfer(inst: &LirInst, state: &mut State) {
    if let Some(dest) = inst.dest {
        let val_of = |op: &LirOperand| -> Option<i64> {
            match op {
                LirOperand::ImmI64(v) => Some(*v),
                LirOperand::Reg(r) => match state.get(r).cloned().unwrap_or(Const::Ambig) {
                    Const::Known(v) => Some(v),
                    _ => None,
                },
                _ => None,
            }
        };
        let new_val = match inst.opcode {
            LirOpcode::Mov => match inst.operands.first() {
                Some(LirOperand::ImmI64(v)) => Const::Known(*v),
                Some(LirOperand::Reg(r)) => state.get(r).cloned().unwrap_or(Const::Ambig),
                _ => Const::Ambig,
            },
            // Fold pure binops of known constants; everything else kills.
            LirOpcode::Add | LirOpcode::Sub | LirOpcode::Mul => {
                match (inst.operands.first(), inst.operands.get(1)) {
                    (Some(a), Some(b)) => match (val_of(a), val_of(b)) {
                        (Some(x), Some(y)) => Const::Known(match inst.opcode {
                            LirOpcode::Add => x.wrapping_add(y),
                            LirOpcode::Sub => x.wrapping_sub(y),
                            _ => x.wrapping_mul(y),
                        }),
                        _ => Const::Ambig,
                    },
                    _ => Const::Ambig,
                }
            }
            _ => Const::Ambig,
        };
        state.insert(dest, new_val);
    }
}

/// Apply substitution + local folding to one instruction against `state`.
fn rewrite(inst: &mut LirInst, state: &State) {
    // BUG (found via differential testing): substituting an immediate into
    // instructions whose encodings require a REGISTER operand (Neg, Not, Br
    // condition, ...) makes backends silently skip them. Only substitute where
    // every backend accepts immediates.
    let subst_ok = matches!(inst.opcode,
        LirOpcode::Mov
        | LirOpcode::Add | LirOpcode::Sub | LirOpcode::Mul | LirOpcode::Div | LirOpcode::Mod
        | LirOpcode::And | LirOpcode::Or | LirOpcode::Xor | LirOpcode::Shl | LirOpcode::Shr
        | LirOpcode::Cmp
        | LirOpcode::Ret
        | LirOpcode::Call // first operand is a Label; arg regs are fine as Imms
    );
    if subst_ok {
        for op in &mut inst.operands {
            if let LirOperand::Reg(r) = op {
                if let Some(Const::Known(v)) = state.get(r) {
                    *op = LirOperand::ImmI64(*v);
                }
            }
        }
    }
    match inst.opcode {
        LirOpcode::Add | LirOpcode::Sub | LirOpcode::Mul => {
            if let (Some(dest), [LirOperand::ImmI64(a), LirOperand::ImmI64(b)]) =
                (inst.dest, &inst.operands[..])
            {
                let v = match inst.opcode {
                    LirOpcode::Add => a.wrapping_add(*b),
                    LirOpcode::Sub => a.wrapping_sub(*b),
                    _ => a.wrapping_mul(*b),
                };
                *inst = LirInst::new(LirOpcode::Mov).with_dest(dest).with_op(LirOperand::ImmI64(v));
            }
        }
        _ => {}
    }
}

fn optimize_function(func: &mut LirFunction) {
    let cfg = brak_opt_utils::build_cfg(func);

    // Map block id -> vec index for state lookup
    let id_to_idx: HashMap<usize, usize> =
        func.blocks.iter().enumerate().map(|(i, b)| (b.id, i)).collect();
    let entry_idx = func.blocks.first().map(|b| b.id).and_then(|id| id_to_idx.get(&id).copied());
    let entry_idx = match entry_idx { Some(i) => i, None => return };

    // Worklist forward dataflow. IN[block] = meet over predecessor OUTs
    // (Top-optimistic: registers are defined before use on every real path).
    let mut out_states: Vec<Option<State>> = vec![None; func.blocks.len()];
    let mut in_states: Vec<State> = vec![HashMap::new(); func.blocks.len()];
    let mut worklist: Vec<usize> = vec![entry_idx];
    let mut queued: HashSet<usize> = vec![entry_idx].into_iter().collect();

    while let Some(bi) = worklist.pop() {
        queued.remove(&bi);

        // IN = join of predecessor OUTs; unvisited predecessors contribute Top.
        let mut in_state: State = HashMap::new();
        let preds = cfg.predecessors.get(&func.blocks[bi].id).cloned().unwrap_or_default();
        for p in preds {
            let pi = match id_to_idx.get(&p) { Some(&i) => i, None => continue };
            if let Some(pout) = &out_states[pi] {
                for (r, v) in pout {
                    let e = in_state.entry(*r).or_insert(Const::Top);
                    *e = join(e, v);
                }
            }
        }
        in_states[bi] = in_state.clone();

        // Transfer through the block
        let mut st = in_state;
        for inst in &func.blocks[bi].insts {
            transfer(inst, &mut st);
        }

        if out_states[bi].as_ref() != Some(&st) {
            out_states[bi] = Some(st);
            for s in cfg.successors.get(&func.blocks[bi].id).cloned().unwrap_or_default() {
                if let Some(&si) = id_to_idx.get(&s) {
                    if queued.insert(si) {
                        worklist.push(si);
                    }
                }
            }
        }
    }

    // Rewrite using each block's IN state.
    for bi in 0..func.blocks.len() {
        let mut st = std::mem::take(&mut in_states[bi]);
        for inst in &mut func.blocks[bi].insts {
            rewrite(inst, &st);
            transfer(inst, &mut st);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use brak_core::span::DUMMY_SPAN;
    use brak_ir_lir::lir::*;

    fn mov(d: VirtReg, o: LirOperand) -> LirInst {
        LirInst::new(LirOpcode::Mov).with_dest(d).with_op(o)
    }

    /// BUG-K04 regression: `%2` gets 5 on one branch and 7 on the other; its use
    /// at the merge must NOT be substituted with either constant.
    #[test]
    fn test_cp_is_path_sensitive() {
        let mut func = LirFunction {
            name: "test".into(), params: vec![], reg_count: 4,
            blocks: vec![
                LirBlock { id: 0, name: "b0".into(),
                    insts: vec![
                        mov(1, LirOperand::ImmI64(5)),
                        mov(9, LirOperand::ImmI64(1)), // cond
                        LirInst::new(LirOpcode::Br)
                            .with_op(LirOperand::Reg(9))
                            .with_op(LirOperand::Label("b1".into()))
                            .with_op(LirOperand::Label("b2".into())),
                    ], span: DUMMY_SPAN },
                LirBlock { id: 1, name: "b1".into(),
                    insts: vec![
                        mov(2, LirOperand::ImmI64(5)),
                        LirInst::new(LirOpcode::Jmp).with_op(LirOperand::Label("b3".into())),
                    ], span: DUMMY_SPAN },
                LirBlock { id: 2, name: "b2".into(),
                    insts: vec![
                        mov(2, LirOperand::ImmI64(7)),
                        LirInst::new(LirOpcode::Jmp).with_op(LirOperand::Label("b3".into())),
                    ], span: DUMMY_SPAN },
                LirBlock { id: 3, name: "b3".into(),
                    insts: vec![
                        LirInst::new(LirOpcode::Add).with_dest(3)
                            .with_op(LirOperand::Reg(2)).with_op(LirOperand::ImmI64(1)),
                        LirInst::new(LirOpcode::Ret).with_op(LirOperand::Reg(3)),
                    ], span: DUMMY_SPAN },
            ], span: DUMMY_SPAN,
        };
        optimize_function(&mut func);
        let b3 = func.blocks.iter().find(|b| b.name == "b3").unwrap();
        assert!(b3.insts.iter().any(|i| i.operands.contains(&LirOperand::Reg(2))),
            "%2 must stay a register (path-dependent value)");
    }

    #[test]
    fn test_cp_folds_dominant_const() {
        let mut func = LirFunction {
            name: "test".into(), params: vec![], reg_count: 2,
            blocks: vec![
                LirBlock { id: 0, name: "b0".into(), insts: vec![
                    mov(0, LirOperand::ImmI64(6)),
                    LirInst::new(LirOpcode::Mul).with_dest(1)
                        .with_op(LirOperand::Reg(0)).with_op(LirOperand::ImmI64(7)),
                    LirInst::new(LirOpcode::Ret).with_op(LirOperand::Reg(1)),
                ], span: DUMMY_SPAN },
            ], span: DUMMY_SPAN,
        };
        optimize_function(&mut func);
        assert!(matches!(&func.blocks[0].insts[1],
            LirInst { opcode: LirOpcode::Mov, operands, .. }
                if operands.last() == Some(&LirOperand::ImmI64(42))),
            "6*7 must fold to Mov 42");
    }
}
