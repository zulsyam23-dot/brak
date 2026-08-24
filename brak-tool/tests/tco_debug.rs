use brak_frontend::lexer::{AsciiLexer, BrakLexer};
use brak_frontend::parser::Parser as BrakParser;
use brak_ir_hir::lower::HirLower;
use brak_ir_hir::typeck::TypeChecker;
use brak_ir_mir::lower::MirLower;
use brak_ir_lir::lower::LirLower;
use brak_opt_traits::PassManager;
use brak_opt_fold::ConstantFolding;
use brak_opt_cp::ConstantPropagation;
use brak_opt_inline::Inlining;
use brak_opt_gvn::GlobalValueNumbering;
use brak_opt_dce::DeadCodeElimination;
use brak_opt_jt::JumpThreading;
use brak_opt_tco::TailCallOptimization;

fn lower(src: &str) -> brak_ir_lir::lir::LirProgram {
    let sm = brak_core::SourceMap::new("t.brk", src);
    let tokens = AsciiLexer::new().lex(&sm);
    let ast = BrakParser::new().parse(&tokens).unwrap();
    let hir = HirLower::new().lower(ast).unwrap();
    TypeChecker::new().check(&hir).unwrap();
    let mir = MirLower::new().lower(hir).unwrap();
    LirLower::new().lower(mir)
}

#[test]
fn debug_tco_pipeline() {
    let src = r#"
fn count(n: i64) -> i64 {
    if n == 0 {
        return 7;
    }
    return count(n - 1);
}
fn main() -> i64 {
    return count(500000);
}
"#;
    let lir = lower(src);
    let mut pm = PassManager::default();
    pm.add_pass(Box::new(ConstantFolding));
    pm.add_pass(Box::new(ConstantPropagation));
    pm.add_pass(Box::new(Inlining));
    pm.add_pass(Box::new(GlobalValueNumbering));
    pm.add_pass(Box::new(brak_opt_licm::Licm));
    pm.add_pass(Box::new(JumpThreading));
    pm.add_pass(Box::new(TailCallOptimization));
    pm.add_pass(Box::new(DeadCodeElimination));
    let out = pm.run(lir).unwrap();
    eprintln!("functions: {:?}", out.functions.iter().map(|f| &f.name).collect::<Vec<_>>());
    for fname in ["count", "main"] {
        if let Some(f) = out.functions.iter().find(|f| f.name == fname) {
            eprintln!("=== {fname} after pipeline ===");
            for b in &f.blocks {
                eprintln!("block {} ({})", b.id, b.name);
                for i in &b.insts { eprintln!("  {:?}", i); }
            }
        }
    }
}
