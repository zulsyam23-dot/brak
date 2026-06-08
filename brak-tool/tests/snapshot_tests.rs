use brak_core::SourceMap;
use brak_frontend::lexer::{AsciiLexer, BrakLexer};
use brak_frontend::parser::Parser as BrakParser;
use brak_ir_hir::lower::HirLower;
use brak_ir_lir::lower::LirLower;
use brak_ir_mir::lower::MirLower;
use brak_test::SnapshotTester;

#[test]
fn test_snapshots() {
    let tester = SnapshotTester::new("tests/snapshots", false);
    
    let src = "fn main() { let x = 42 + 10; return x; }";
    let sm = SourceMap::new("test.brk", src);
    let mut lexer = AsciiLexer::new();
    let tokens = lexer.lex(&sm);
    
    let parser = BrakParser::new();
    let ast = parser.parse(&tokens).unwrap();
    tester.assert_snapshot("simple_ast", &ast).unwrap();
    
    let hir_lower = HirLower::new();
    let hir = hir_lower.lower(ast).unwrap();
    tester.assert_snapshot("simple_hir", &hir).unwrap();
    
    let mut mir_lower = MirLower::new();
    let mir = mir_lower.lower(hir).unwrap();
    tester.assert_snapshot("simple_mir", &mir).unwrap();
    
    let mut lir_lower = LirLower::new();
    let lir = lir_lower.lower(mir);
    tester.assert_snapshot("simple_lir", &lir).unwrap();
}
