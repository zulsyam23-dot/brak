use brak_core::SourceMap;
use brak_frontend::lexer::{AsciiLexer, BrakLexer};
use brak_frontend::parser::Parser as BrakParser;
use brak_ir_hir::lower::HirLower;
use brak_ir_lir::lir::LirProgram;
use brak_ir_lir::lower::LirLower;
use brak_ir_mir::lower::MirLower;

fn full_pipeline(src: &str) -> LirProgram {
    let sm = SourceMap::new("test.brk", src);
    let mut lexer = AsciiLexer::new();
    let tokens = lexer.lex(&sm);
    let parser = BrakParser::new();
    let ast = parser.parse(&tokens).unwrap();
    let hir_lower = HirLower::new();
    let hir = hir_lower.lower(ast).unwrap();
    let mut mir_lower = MirLower::new();
    let mir = mir_lower.lower(hir).unwrap();
    let mut lir_lower = LirLower::new();
    lir_lower.lower(mir)
}

#[test]
fn test_pipeline_empty() {
    let lir = full_pipeline("");
    assert!(lir.functions.is_empty());
}

#[test]
fn test_pipeline_simple_fn() {
    let lir = full_pipeline("fn main() { return 42; }");
    assert_eq!(lir.functions.len(), 1);
    assert_eq!(lir.functions[0].name, "main");
}

#[test]
fn test_pipeline_fn_with_let() {
    let lir = full_pipeline("fn main() { let x = 42; return x; }");
    assert_eq!(lir.functions.len(), 1);
    let func = &lir.functions[0];
    assert!(func.reg_count > 0);
}

#[test]
fn test_pipeline_if_else() {
    let lir = full_pipeline("fn f() { if 1 { return 1; } else { return 2; } }");
    assert_eq!(lir.functions.len(), 1);
    let func = &lir.functions[0];
    // Should have multiple blocks (if, then, else, merge)
    assert!(func.blocks.len() >= 3);
    let has_branch = func.blocks.iter().any(|b| {
        b.insts.iter().any(|i| i.opcode == brak_ir_lir::lir::LirOpcode::Br)
    });
    assert!(has_branch);
}

#[test]
fn test_pipeline_while() {
    let lir = full_pipeline("fn f() { while 1 { let x = 0; x } }");
    assert_eq!(lir.functions.len(), 1);
    // While generates a back-edge branch
    let func = &lir.functions[0];
    assert!(func.blocks.len() >= 3);
}

#[test]
fn test_pipeline_binops() {
    let lir = full_pipeline("fn f() { return 1 + 2 * 3; }");
    assert_eq!(lir.functions.len(), 1);
    let func = &lir.functions[0];
    let has_mul = func.blocks.iter().any(|b| {
        b.insts.iter().any(|i| i.opcode == brak_ir_lir::lir::LirOpcode::Mul)
    });
    let has_add = func.blocks.iter().any(|b| {
        b.insts.iter().any(|i| i.opcode == brak_ir_lir::lir::LirOpcode::Add)
    });
    assert!(has_mul);
    assert!(has_add);
}

#[test]
fn test_pipeline_call() {
    let lir = full_pipeline("fn f() { other(); }");
    let func = &lir.functions[0];
    let has_call = func.blocks.iter().any(|b| {
        b.insts.iter().any(|i| i.opcode == brak_ir_lir::lir::LirOpcode::Call)
    });
    assert!(has_call);
}
