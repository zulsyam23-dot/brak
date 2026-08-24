/// Analisis CFG (Control Flow Graph) dan deteksi natural loop untuk LIR.
///
/// Utilitas dibagi dalam 3 level:
/// 1. `CfgGraph` — predecessors, successors per block
/// 2. `Dominance` — dominator tree (iterative dataflow)
/// 3. `LoopInfo` — natural loop detection via back-edges

use std::collections::{HashMap, HashSet};
use brak_ir_lir::lir::{LirFunction, LirOpcode, LirOperand, BlockId};

// ── level 1: CFG ──────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct CfgGraph {
    pub block_count: usize,
    pub successors: HashMap<BlockId, Vec<BlockId>>,
    pub predecessors: HashMap<BlockId, Vec<BlockId>>,
}

pub fn build_cfg(func: &LirFunction) -> CfgGraph {
    let mut succ: HashMap<BlockId, Vec<BlockId>> = HashMap::new();
    let block_map: HashMap<&str, BlockId> = func.blocks.iter().map(|b| (b.name.as_str(), b.id)).collect();
    // BUG-K06: LIR branches normally target "block_{id}", so resolution must try
    // the id form FIRST and only fall back to block names. Name-only lookup never
    // matched real compiler output, leaving the CFG with fall-through edges only.
    let id_set: HashSet<BlockId> = func.blocks.iter().map(|b| b.id).collect();
    let resolve = |name: &str| -> Option<BlockId> {
        if let Some(id) = name.strip_prefix("block_").and_then(|s| s.parse::<BlockId>().ok()) {
            if id_set.contains(&id) {
                return Some(id);
            }
        }
        block_map.get(name).copied()
    };

    for (bi, block) in func.blocks.iter().enumerate() {
        let edges = succ.entry(block.id).or_default();
        for inst in &block.insts {
            match inst.opcode {
                LirOpcode::Jmp => {
                    if let Some(LirOperand::Label(name)) = inst.operands.first() {
                        if let Some(t) = resolve(name.as_str()) { edges.push(t); }
                    }
                }
                LirOpcode::Br => {
                    for op in &inst.operands {
                        if let LirOperand::Label(name) = op {
                            if let Some(t) = resolve(name.as_str()) { edges.push(t); }
                        }
                    }
                }
                _ => {}
            }
        }
        // Fall-through only applies when there is no explicit terminator.
        let has_terminator = block.insts.iter().any(|i|
            matches!(i.opcode, LirOpcode::Jmp | LirOpcode::Br | LirOpcode::Ret));
        if !has_terminator && bi + 1 < func.blocks.len() {
            edges.push(func.blocks[bi + 1].id);
        }
    }

    let mut pred: HashMap<BlockId, Vec<BlockId>> = HashMap::new();
    for (&from, to_list) in &succ {
        for &to in to_list {
            pred.entry(to).or_default().push(from);
        }
    }

    CfgGraph {
        block_count: func.blocks.len(),
        successors: succ,
        predecessors: pred,
    }
}

// ── level 2: Dominance ─────────────────────────────────────────

/// Iterative dominator computation (simple, correct)
#[derive(Debug, Clone)]
pub struct Dominance {
    pub idoms: HashMap<BlockId, BlockId>,   // immediate dominator
}

pub fn compute_dominance(func: &LirFunction, cfg: &CfgGraph) -> Dominance {
    let entry = func.blocks.first().map(|b| b.id).unwrap_or(0);
    let all_blocks: HashSet<BlockId> = func.blocks.iter().map(|b| b.id).collect();

    // Initialize: entry dominates itself, all others dominated by all blocks
    let mut dom: HashMap<BlockId, HashSet<BlockId>> = HashMap::new();
    for &b in &all_blocks {
        if b == entry {
            let mut s = HashSet::new();
            s.insert(entry);
            dom.insert(entry, s);
        } else {
            dom.insert(b, all_blocks.clone());
        }
    }

    // Iterative dataflow: dom(n) = {n} ∪ (∩_{p ∈ pred(n)} dom(p))
    let mut changed = true;
    while changed {
        changed = false;
        for b in func.blocks.iter().map(|b| b.id) {
            if b == entry { continue; }
            let preds = cfg.predecessors.get(&b).cloned().unwrap_or_default();
            if preds.is_empty() { continue; }

            let mut new_dom: HashSet<BlockId> = preds.iter()
                .map(|p| dom.get(p).cloned().unwrap_or_default())
                .fold(None, |acc: Option<HashSet<BlockId>>, s| {
                    Some(acc.map_or_else(|| s.clone(), |a| a.intersection(&s).copied().collect()))
                }).unwrap_or_default();
            new_dom.insert(b);

            if new_dom != *dom.get(&b).unwrap_or(&HashSet::new()) {
                dom.insert(b, new_dom);
                changed = true;
            }
        }
    }

    // Compute immediate dominators: idom(b) = the unique node in dom(b) \ {b}
    // that is dominated by all other nodes in dom(b) \ {b}
    let mut idoms = HashMap::new();
    for &b in &all_blocks {
        if b == entry { continue; }
        let dom_set = dom.get(&b).cloned().unwrap_or_default();
        let mut candidates: Vec<BlockId> = dom_set.iter().copied().filter(|&d| d != b).collect();
        candidates.sort();
        // The immediate dominator is the node closest to b in dom(b)
        // Since we have a partial order, we pick the one that dominates all others
        let candidates_clone = candidates.clone();
        let idom = candidates.into_iter().find(|&c| {
            dom.get(&c).map(|d| {
                candidates_clone.iter().all(|&other| other == c || d.contains(&other))
            }).unwrap_or(false)
        });
        if let Some(idom) = idom {
            idoms.insert(b, idom);
        }
    }

    Dominance { idoms }
}

pub fn dominates(dom: &Dominance, a: BlockId, b: BlockId) -> bool {
    if a == b { return true; }
    let mut current = b;
    while let Some(&idom) = dom.idoms.get(&current) {
        if idom == a { return true; }
        current = idom;
    }
    false
}

// ── level 3: Natural Loops ─────────────────────────────────────

#[derive(Debug, Clone)]
pub struct NaturalLoop {
    pub header: BlockId,
    pub body: HashSet<BlockId>,
    pub back_edge_from: Vec<BlockId>,
}

pub fn find_natural_loops(_func: &LirFunction, cfg: &CfgGraph, dom: &Dominance) -> Vec<NaturalLoop> {
    let mut loops: Vec<NaturalLoop> = Vec::new();

    for (&from, to_list) in &cfg.successors {
        for &to in to_list {
            // Back-edge: target < source (in block order) AND target dominates source
            if to < from && dominates(dom, to, from) {
                // Natural loop: header = to, body = {to} ∪ {all blocks reachable from to
                // going backwards through predecessors, without passing through to}
                let mut body = HashSet::new();
                body.insert(to);
                let mut stack: Vec<BlockId> = vec![from];
                let mut visited = HashSet::new();
                visited.insert(to); // don't go past the header
                while let Some(node) = stack.pop() {
                    if !visited.insert(node) { continue; }
                    body.insert(node);
                    if let Some(preds) = cfg.predecessors.get(&node) {
                        for &p in preds {
                            if !visited.contains(&p) {
                                stack.push(p);
                            }
                        }
                    }
                }

                // Check if we already have a loop with this header
                let existing = loops.iter_mut().find(|l: &&mut NaturalLoop| l.header == to);
                if let Some(existing) = existing {
                    existing.body.extend(&body);
                    existing.back_edge_from.push(from);
                } else {
                    loops.push(NaturalLoop {
                        header: to,
                        body,
                        back_edge_from: vec![from],
                    });
                }
            }
        }
    }

    loops
}

#[cfg(test)]
mod tests {
    use super::*;
    use brak_core::span::DUMMY_SPAN;
    use brak_ir_lir::lir::*;

    fn mov(d: VirtReg, o: LirOperand) -> LirInst {
        LirInst::new(LirOpcode::Mov).with_dest(d).with_op(o)
    }

    fn make_while_loop() -> LirFunction {
        // while-like CFG:
        // b0 (entry) -> Jmp b1
        // b1 (header) -> Br cond ? b2 : b3
        // b2 (body) -> Jmp b1 (back-edge)
        // b3 (exit) -> Ret
        LirFunction {
            name: "test".into(), params: vec![], reg_count: 2,
            blocks: vec![
                LirBlock { id: 0, name: "b0".into(),
                    insts: vec![
                        mov(0, LirOperand::ImmI64(0)),
                        LirInst::new(LirOpcode::Jmp).with_op(LirOperand::Label("b1".into())),
                    ], span: DUMMY_SPAN },
                LirBlock { id: 1, name: "b1".into(),
                    insts: vec![
                        mov(1, LirOperand::ImmI64(10)),
                        LirInst::new(LirOpcode::SetEq).with_dest(1)
                            .with_op(LirOperand::Reg(0)).with_op(LirOperand::ImmI64(10)),
                        LirInst::new(LirOpcode::Br)
                            .with_op(LirOperand::Reg(1))
                            .with_op(LirOperand::Label("b2".into()))
                            .with_op(LirOperand::Label("b3".into())),
                    ], span: DUMMY_SPAN },
                LirBlock { id: 2, name: "b2".into(),
                    insts: vec![
                        mov(0, LirOperand::ImmI64(42)),  // loop-invariant!
                        LirInst::new(LirOpcode::Jmp).with_op(LirOperand::Label("b1".into())),
                    ], span: DUMMY_SPAN },
                LirBlock { id: 3, name: "b3".into(),
                    insts: vec![
                        LirInst::new(LirOpcode::Ret).with_op(LirOperand::Reg(0)),
                    ], span: DUMMY_SPAN },
            ],
            span: DUMMY_SPAN,
        }
    }

    #[test]
    fn test_cfg_successors() {
        let f = make_while_loop();
        let cfg = build_cfg(&f);
        assert_eq!(cfg.successors.get(&0).unwrap(), &[1]);
        assert_eq!(cfg.successors.get(&1).unwrap().len(), 2); // b2 and b3
        assert!(cfg.successors.get(&1).unwrap().contains(&2));
        assert!(cfg.successors.get(&1).unwrap().contains(&3));
        assert_eq!(cfg.successors.get(&2).unwrap(), &[1]); // back-edge
    }

    #[test]
    fn test_dominance() {
        let f = make_while_loop();
        let cfg = build_cfg(&f);
        let dom = compute_dominance(&f, &cfg);
        // b0 dominates everything
        assert!(dominates(&dom, 0, 0));
        assert!(dominates(&dom, 0, 1));
        assert!(dominates(&dom, 0, 2));
        assert!(dominates(&dom, 0, 3));
        // b1 dominates b2 and b3
        assert!(dominates(&dom, 1, 2));
        assert!(dominates(&dom, 1, 3));
        // b1 does NOT dominate b0
        assert!(!dominates(&dom, 1, 0));
    }

    #[test]
    fn test_loop_detection() {
        let f = make_while_loop();
        let cfg = build_cfg(&f);
        let dom = compute_dominance(&f, &cfg);
        let loops = find_natural_loops(&f, &cfg, &dom);
        assert_eq!(loops.len(), 1, "expected 1 loop, got {}", loops.len());
        assert_eq!(loops[0].header, 1);
        assert!(loops[0].body.contains(&1));
        assert!(loops[0].body.contains(&2));
        assert!(!loops[0].body.contains(&3));
        assert!(!loops[0].body.contains(&0));
    }
}
