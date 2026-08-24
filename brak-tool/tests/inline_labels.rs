use brak_frontend::lexer::{AsciiLexer, BrakLexer};
use brak_frontend::parser::Parser as BrakParser;
use brak_ir_hir::lower::HirLower;
use brak_ir_hir::typeck::TypeChecker;
use brak_ir_mir::lower::MirLower;
use brak_ir_lir::lower::LirLower;
use brak_opt_inline::Inlining;
use brak_opt_traits::LirOptimizationPass;

#[test]
fn debug_inline_dangling_cont() {
    let src = r#"
fn pick(n: i32) -> i32 {
    let i = 0;
    while true {
        if i == n {
            return i * 2;
        }
        if true {
            return 99;
        }
    }
    return 0;
}

fn mid_if(x: i32) -> i32 {
    let y = if x > 0 { 10 } else { 20 };
    return y + 1;
}

fn main() -> i32 {
    let a = mid_if(5);
    let b = mid_if(-5);
    return pick(3) + a + b;
}
"#;
    let path = "debug_inline.brk";
    std::fs::write(path, src).unwrap();

    let source = std::fs::read_to_string(path).unwrap();
    let source_map = brak_core::SourceMap::new(path, &source);
    let mut lexer = AsciiLexer::new();
    let tokens = lexer.lex(&source_map);
    let ast = BrakParser::new().parse(&tokens).unwrap();
    let hir = HirLower::new().lower(ast).unwrap();
    let mut tc = TypeChecker::new();
    tc.check(&hir).unwrap();
    let mir = MirLower::new().lower(hir).unwrap();
    let mut lower = LirLower::new();
    let lir = lower.lower(mir);

    // collect labels before
    let before = count_unresolved(&lir);
    assert!(before.is_empty(), "pre-inline LIR must have no unresolved labels: {before:?}");

    let out = Inlining.run(lir).unwrap();
    let after = count_unresolved(&out);
    assert!(after.is_empty(), "post-inline dangling labels found: {after:?}");
}

fn count_unresolved(prog: &brak_ir_lir::lir::LirProgram) -> Vec<String> {
    let mut unresolved = vec![];
    for f in &prog.functions {
        let names: std::collections::HashSet<&str> = f.blocks.iter().map(|b| b.name.as_str()).collect();
        for b in &f.blocks {
            for inst in &b.insts {
                for (i, op) in inst.operands.iter().enumerate() {
                    // Call's first operand is the callee function name
                    if inst.opcode == brak_ir_lir::lir::LirOpcode::Call && i == 0 {
                        continue;
                    }
                    if let brak_ir_lir::lir::LirOperand::Label(l) = op {
                        let ok = names.contains(l.as_str())
                            || l.strip_prefix("block_").and_then(|s| s.parse::<usize>().ok())
                                .map(|id| f.blocks.iter().any(|b| b.id == id))
                                .unwrap_or(false);
                        if !ok {
                            unresolved.push(format!("{}: {} -> {}", f.name, b.name, l));
                        }
                    }
                }
            }
        }
    }
    unresolved
}
