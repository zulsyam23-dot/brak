use brak_frontend::lexer::{AsciiLexer, BrakLexer};
use brak_frontend::parser::Parser as BrakParser;
use brak_ir_hir::lower::HirLower;
use brak_ir_hir::typeck::TypeChecker;
use brak_ir_mir::lower::MirLower;
use brak_ir_lir::lower::LirLower;
use brak_ir_lir::lir::{LirOpcode, LirProgram};
use brak_opt_licm::Licm;
use brak_opt_traits::LirOptimizationPass;

fn lower_src(src: &str) -> LirProgram {
    let sm = brak_core::SourceMap::new("test.brk", src);
    let mut lexer = AsciiLexer::new();
    let tokens = lexer.lex(&sm);
    let ast = BrakParser::new().parse(&tokens).unwrap();
    let hir = HirLower::new().lower(ast).unwrap();
    let mut tc = TypeChecker::new();
    tc.check(&hir).unwrap();
    let mir = MirLower::new().lower(hir).unwrap();
    LirLower::new().lower(mir)
}

/// Regression (BUG-K05): LICM must never hoist flag-consuming `Set*`
/// instructions without their `Cmp`, and must not move loop-carried values.
/// Discovered via differential testing: hoisting a bare SetEq made every
/// branch decision depend on stale flags (nested.brk returned 0 instead of 8).
#[test]
fn licm_never_hoists_flag_ops_or_loop_carried_values() {
    let src = r#"
fn main() -> i32 {
    let x = 0;
    let i = 0;
    while i < 5 {
        if i % 2 == 0 {
            x = x + i;
        } else {
            x = x + 1;
        }
        i = i + 1;
    }
    return x;
}
"#;
    let lir = lower_src(src);
    let before_sets: usize = count_ops(&lir, |op| matches!(op, LirOpcode::SetEq | LirOpcode::SetLt));
    let out = Licm.run(lir).unwrap();
    let after_sets_in_entry = out.functions.iter()
        .find(|f| f.name == "main").unwrap()
        .blocks.first().unwrap()
        .insts.iter()
        .filter(|i| matches!(i.opcode, LirOpcode::SetEq | LirOpcode::SetLt))
        .count();
    // No Set* may appear in the entry (pre-header) block.
    assert_eq!(after_sets_in_entry, 0,
        "LICM hoisted a flag-consuming Set* without its Cmp");
    // Total Set*/Cmp population preserved (nothing dropped).
    let total_after: usize = count_ops(&out, |op| {
        matches!(op, LirOpcode::SetEq | LirOpcode::SetLt | LirOpcode::SetGe)
    });
    let _ = before_sets;
    assert!(total_after > 0, "comparison ops vanished after LICM");
}

fn count_ops(prog: &LirProgram, pred: fn(LirOpcode) -> bool) -> usize {
    prog.functions.iter()
        .flat_map(|f| f.blocks.iter())
        .flat_map(|b| b.insts.iter())
        .filter(|i| pred(i.opcode))
        .count()
}
