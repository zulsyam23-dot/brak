use std::collections::HashMap;
use brak_core::Result;
use brak_ir_lir::lir::{LirProgram, LirFunction, LirBlock, LirInst, LirOpcode, LirOperand};
#[cfg(test)]
use brak_ir_lir::lir::VirtReg;
use brak_opt_traits::LirOptimizationPass;

pub struct Inlining;

impl LirOptimizationPass for Inlining {
    fn name(&self) -> &'static str { "inline" }

    fn run(&self, mut program: LirProgram) -> Result<LirProgram> {
        let callee_map: HashMap<String, LirFunction> = program.functions.iter().cloned()
            .map(|f| (f.name.clone(), f)).collect();
        loop {
            let mut changed = false;
            for func in &mut program.functions {
                if inline_one_pass(func, &callee_map) {
                    changed = true;
                }
            }
            if !changed { break; }
        }
        Ok(program)
    }
}

fn inline_one_pass(caller: &mut LirFunction, callee_map: &HashMap<String, LirFunction>) -> bool {
    let mut new_blocks: Vec<LirBlock> = Vec::new();
    let max_id = caller.blocks.iter().map(|b| b.id).max().unwrap_or(0);
    let mut next_id = max_id + 1;
    let mut changed = false;
    // BUG-K07: blocks that get inlined are renamed to `{name}.pre`, but earlier
    // passes may have emitted labels pointing at the OLD name (e.g. a previous
    // inline iteration's `{x}.cont`). Track renames so references stay valid.
    let mut renames: HashMap<String, String> = HashMap::new();

    for block in &caller.blocks {
        let mut found = None;
        for (i, inst) in block.insts.iter().enumerate() {
            if let LirOpcode::Call = inst.opcode {
                if let Some(LirOperand::Label(name)) = inst.operands.first() {
                    if let Some(callee) = callee_map.get(name) {
                        let total: usize = callee.blocks.iter().map(|b| b.insts.len()).sum();
                        if total < 20 && name != &caller.name {
                            // BUG (found via TCO work): inlining a DIRECTLY
                            // recursive callee re-introduces the same call site
                            // every pass — infinite loop. Leave recursive
                            // functions alone (TCO optimizes them in place).
                            let self_recursive = callee.blocks.iter().any(|b|
                                b.insts.iter().any(|ci|
                                    ci.opcode == LirOpcode::Call
                                        && matches!(ci.operands.first(), Some(LirOperand::Label(l)) if l == name)
                                ));
                            if !self_recursive {
                                found = Some((i, callee.clone()));
                                break;
                            }
                        }
                    }
                }
            }
        }

        if let Some((call_idx, callee)) = found {
            changed = true;
            let call_inst = &block.insts[call_idx];
            let ro = caller.reg_count;
            renames.insert(block.name.clone(), format!("{}.pre", block.name));

            // Block A: instructions before Call
            let aid = block.id;
            new_blocks.push(LirBlock {
                id: aid,
                name: format!("{}.pre", block.name),
                insts: block.insts[..call_idx].to_vec(),
                span: block.span,
            });

            let inline_id = format!("f{}.b{}", caller.name, next_id);
            // LIR branch labels use "block_{id}" (numeric), while block names
            // are arbitrary ("entry", "unreachable", ...). Rewrite targets by
            // block id so they always match the generated block names.
            let id_to_name: HashMap<usize, String> =
                callee.blocks.iter().map(|b| (b.id, format!("{}_block_{}", inline_id, b.id))).collect();
            // BUG-K07 (part 2): only labels INTERNAL to the callee get rewritten.
            // Global symbols referenced by the callee (e.g. `Call count` inside
            // an inlined caller chain) must keep their original names — the old
            // blanket rename mangled them into unresolvable symbols.
            let callee_internal_names: std::collections::HashSet<String> = callee
                .blocks
                .iter()
                .flat_map(|b| {
                    let mapped = id_to_name.get(&b.id).cloned().into_iter();
                    mapped.chain(std::iter::once(fmt_label(&inline_id, &b.name)))
                })
                .collect();
            let rewrite_lbl = |l: &str| -> String {
                if let Some(id) = l.strip_prefix("block_").and_then(|s| s.parse::<usize>().ok()) {
                    id_to_name.get(&id).cloned().unwrap_or_else(|| l.to_string())
                } else if callee_internal_names.contains(l) {
                    fmt_label(&inline_id, l)
                } else {
                    l.to_string()
                }
            };

            // Prelude: parameter Mov + Jmp to callee entry
            let mut prelude = Vec::new();
            for (pi, &pr) in callee.params.iter().enumerate() {
                if let Some(arg) = call_inst.operands.get(pi + 1) {
                    prelude.push(LirInst::new(LirOpcode::Mov).with_dest(pr + ro).with_op(arg.clone()));
                }
            }
            let entry_lbl = id_to_name.get(&callee.blocks[0].id).cloned().unwrap_or_default();
            prelude.push(LirInst::new(LirOpcode::Jmp).with_op(LirOperand::Label(entry_lbl)));
            let pid = next_id; next_id += 1;
            new_blocks.push(LirBlock {
                id: pid,
                name: format!("{}.prelude", inline_id),
                insts: prelude,
                span: block.span,
            });

            // Continuation target name — unique per call site so repeated
            // inlining of same-named blocks can never collide (BUG-K07).
            let cid = next_id; next_id += 1;
            let cont_name = format!("{}_cont{}", block.name, cid);

            // Inline callee blocks
            for cb in &callee.blocks {
                let new_lbl = id_to_name.get(&cb.id).cloned().unwrap_or_else(|| fmt_label(&inline_id, &cb.name));
                let mut ci = Vec::new();
                for inst in &cb.insts {
                    if inst.opcode == LirOpcode::Ret {
                        if let Some(dest) = call_inst.dest {
                            if let Some(rv) = inst.operands.first() {
                                let mut fv = rv.clone();
                                if let LirOperand::Reg(r) = &mut fv { *r += ro; }
                                ci.push(LirInst::new(LirOpcode::Mov).with_dest(dest).with_op(fv));
                            }
                        }
                        ci.push(LirInst::new(LirOpcode::Jmp)
                            .with_op(LirOperand::Label(cont_name.clone())));
                        continue;
                    }
                    let mut ni = inst.clone();
                    if let Some(d) = &mut ni.dest { *d += ro; }
                    for op in &mut ni.operands {
                        match op {
                            LirOperand::Reg(r) => *r += ro,
                            LirOperand::Label(l) => *l = rewrite_lbl(l),
                            _ => {}
                        }
                    }
                    ci.push(ni);
                }
                let bid = next_id; next_id += 1;
                new_blocks.push(LirBlock { id: bid, name: new_lbl, insts: ci, span: block.span });
            }

            caller.reg_count += callee.reg_count;

            // Continuation block — ALWAYS created (BUG-K07): even when the call is
            // the last instruction, inlined `Ret`s jump here, so the label must
            // exist.
            new_blocks.push(LirBlock {
                id: cid,
                name: cont_name,
                insts: if call_idx + 1 < block.insts.len() {
                    block.insts[call_idx + 1..].to_vec()
                } else {
                    // empty: control falls through to the next block in order
                    vec![]
                },
                span: block.span,
            });
        } else {
            new_blocks.push(block.clone());
        }
    }

    if changed {
        // Apply renames across every label operand so references to old block
        // names follow the blocks into their `{name}.pre` versions.
        for nb in &mut new_blocks {
            for inst in &mut nb.insts {
                for op in &mut inst.operands {
                    if let LirOperand::Label(l) = op {
                        if let Some(new_name) = renames.get(l.as_str()) {
                            *l = new_name.clone();
                        }
                    }
                }
            }
        }
        caller.blocks = new_blocks;
    }
    changed
}

fn fmt_label(inline_id: &str, label: &str) -> String {
    format!("{}_{}", inline_id, label)
}

#[cfg(test)]
mod tests {
    use super::*;
    use brak_core::span::DUMMY_SPAN;

    fn mov(d: VirtReg, o: LirOperand) -> LirInst {
        LirInst::new(LirOpcode::Mov).with_dest(d).with_op(o)
    }

    fn make_program(funcs: Vec<LirFunction>) -> LirProgram {
        LirProgram {
            functions: funcs,
            extern_functions: vec![],
            structs: vec![],
            enums: vec![],
            string_table: vec![],
            files: vec![],
        }
    }

    #[test]
    fn test_inline_single_block() {
        let mut p = make_program(vec![
            LirFunction {
                name: "inc".into(), params: vec![0], reg_count: 2,
                blocks: vec![LirBlock {
                    id: 0, name: "b0".into(),
                    insts: vec![
                        LirInst::new(LirOpcode::Add).with_dest(1)
                            .with_op(LirOperand::Reg(0)).with_op(LirOperand::ImmI64(1)),
                        LirInst::new(LirOpcode::Ret).with_op(LirOperand::Reg(1)),
                    ], span: DUMMY_SPAN,
                }], span: DUMMY_SPAN,
            },
            LirFunction {
                name: "test".into(), params: vec![], reg_count: 2,
                blocks: vec![LirBlock {
                    id: 0, name: "b0".into(),
                    insts: vec![
                        mov(0, LirOperand::ImmI64(41)),
                        LirInst::new(LirOpcode::Call).with_dest(1)
                            .with_op(LirOperand::Label("inc".into()))
                            .with_op(LirOperand::Reg(0)),
                        LirInst::new(LirOpcode::Ret).with_op(LirOperand::Reg(1)),
                    ], span: DUMMY_SPAN,
                }], span: DUMMY_SPAN,
            },
        ]);
        let pass = Inlining;
        p = pass.run(p).unwrap();
        let tf = p.functions.iter().find(|f| f.name == "test").unwrap();
        assert!(tf.blocks.len() >= 3, "blocks={}", tf.blocks.len());
        assert!(tf.reg_count >= 4);
    }

    #[test]
    fn test_inline_multi_block() {
        let mut p = make_program(vec![
            LirFunction {
                name: "check".into(), params: vec![0], reg_count: 3,
                blocks: vec![
                    LirBlock { id: 0, name: "b0".into(),
                        insts: vec![
                            LirInst::new(LirOpcode::SetEq).with_dest(1)
                                .with_op(LirOperand::Reg(0)).with_op(LirOperand::ImmI64(0)),
                            LirInst::new(LirOpcode::Br)
                                .with_op(LirOperand::Reg(1))
                                .with_op(LirOperand::Label("b1".into()))
                                .with_op(LirOperand::Label("b2".into())),
                        ], span: DUMMY_SPAN },
                    LirBlock { id: 1, name: "b1".into(),
                        insts: vec![
                            mov(2, LirOperand::ImmI64(1)),
                            LirInst::new(LirOpcode::Ret).with_op(LirOperand::Reg(2)),
                        ], span: DUMMY_SPAN },
                    LirBlock { id: 2, name: "b2".into(),
                        insts: vec![
                            mov(2, LirOperand::ImmI64(0)),
                            LirInst::new(LirOpcode::Ret).with_op(LirOperand::Reg(2)),
                        ], span: DUMMY_SPAN },
                ], span: DUMMY_SPAN,
            },
            LirFunction {
                name: "test".into(), params: vec![], reg_count: 2,
                blocks: vec![LirBlock {
                    id: 0, name: "b0".into(),
                    insts: vec![
                        mov(0, LirOperand::ImmI64(42)),
                        LirInst::new(LirOpcode::Call).with_dest(1)
                            .with_op(LirOperand::Label("check".into()))
                            .with_op(LirOperand::Reg(0)),
                        LirInst::new(LirOpcode::Ret).with_op(LirOperand::Reg(1)),
                    ], span: DUMMY_SPAN,
                }], span: DUMMY_SPAN,
            },
        ]);
        let pass = Inlining;
        p = pass.run(p).unwrap();
        let tf = p.functions.iter().find(|f| f.name == "test").unwrap();
        assert!(tf.blocks.len() >= 4, "blocks={}", tf.blocks.len());
    }

    #[test]
    fn test_inline_two_calls() {
        let mut p = make_program(vec![
            LirFunction {
                name: "double".into(), params: vec![0], reg_count: 2,
                blocks: vec![LirBlock {
                    id: 0, name: "b0".into(),
                    insts: vec![
                        LirInst::new(LirOpcode::Add).with_dest(1)
                            .with_op(LirOperand::Reg(0)).with_op(LirOperand::Reg(0)),
                        LirInst::new(LirOpcode::Ret).with_op(LirOperand::Reg(1)),
                    ], span: DUMMY_SPAN,
                }], span: DUMMY_SPAN,
            },
            LirFunction {
                name: "test".into(), params: vec![], reg_count: 3,
                blocks: vec![LirBlock {
                    id: 0, name: "b0".into(),
                    insts: vec![
                        LirInst::new(LirOpcode::Call).with_dest(0)
                            .with_op(LirOperand::Label("double".into()))
                            .with_op(LirOperand::ImmI64(21)),
                        LirInst::new(LirOpcode::Call).with_dest(1)
                            .with_op(LirOperand::Label("double".into()))
                            .with_op(LirOperand::Reg(0)),
                        LirInst::new(LirOpcode::Ret).with_op(LirOperand::Reg(1)),
                    ], span: DUMMY_SPAN,
                }], span: DUMMY_SPAN,
            },
        ]);
        let pass = Inlining;
        p = pass.run(p).unwrap();
        let tf = p.functions.iter().find(|f| f.name == "test").unwrap();
        assert!(tf.blocks.len() >= 6, "blocks={}", tf.blocks.len());
        for b in &tf.blocks {
            for inst in &b.insts {
                assert_ne!(inst.opcode, LirOpcode::Call, "Call remains in {}", b.name);
            }
        }
    }
}
