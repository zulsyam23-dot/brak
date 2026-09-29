use super::*;
use brak_core::{Span, DUMMY_SPAN};

fn dummy_span() -> Span {
    DUMMY_SPAN
}

fn dummy_hir_fn(name: &str, stmts: Vec<HirStmt>) -> HirFunction {
    HirFunction {
        name: name.to_string(),
        params: vec![],
        ret_ty: HirType::I32,
        body: HirBlock {
            stmts,
            span: dummy_span(),
        },
        span: dummy_span(),
    }
}

#[test]
fn test_lower_simple_int_return() {
    let hir_func = dummy_hir_fn(
        "f",
        vec![HirStmt::Return(
            Some(Box::new(HirExpr::Int(42, dummy_span()))),
            dummy_span(),
        )],
    );
    let mut lowerer = MirLower::new();
    let mir_func = lowerer.lower_function(hir_func).unwrap();
    assert_eq!(mir_func.name, "f");
    assert!(mir_func.blocks.len() >= 1);
}

#[test]
fn test_lower_binary_op() {
    let hir_func = dummy_hir_fn(
        "add",
        vec![HirStmt::Return(
            Some(Box::new(HirExpr::BinOp {
                op: HirBinOp::Add,
                lhs: Box::new(HirExpr::Int(1, dummy_span())),
                rhs: Box::new(HirExpr::Int(2, dummy_span())),
                span: dummy_span(),
            })),
            dummy_span(),
        )],
    );
    let mut lowerer = MirLower::new();
    let mir_func = lowerer.lower_function(hir_func).unwrap();
    assert!(!mir_func.blocks.is_empty());
}

#[test]
fn test_lower_if_expr() {
    let hir_func = dummy_hir_fn(
        "test",
        vec![HirStmt::Return(
            Some(Box::new(HirExpr::If {
                cond: Box::new(HirExpr::Bool(true, dummy_span())),
                then: Box::new(HirExpr::Int(1, dummy_span())),
                else_: Box::new(HirExpr::Int(0, dummy_span())),
                span: dummy_span(),
            })),
            dummy_span(),
        )],
    );
    let mut lowerer = MirLower::new();
    let mir_func = lowerer.lower_function(hir_func).unwrap();
    assert!(
        mir_func.blocks.len() >= 3,
        "if should produce at least 3 blocks"
    );
    let return_local = match &mir_func.blocks.last().unwrap().terminator {
        MirTerminator::Return {
            value: Some(local), ..
        } => *local,
        other => panic!("if-expression result should be returned, got {other:?}"),
    };
    let branch_result_writes = mir_func
        .blocks
        .iter()
        .flat_map(|block| &block.insts)
        .filter(|inst| {
            matches!(inst,
                MirInst::Assign { dest, value: MirValue::Local(_), .. } if *dest == return_local
            )
        })
        .count();
    assert_eq!(
        branch_result_writes, 2,
        "both if branches must write the merge result"
    );
}

#[test]
fn test_lower_tail_if_statement_preserves_branch_value() {
    let hir_func = dummy_hir_fn(
        "test",
        vec![HirStmt::If {
            cond: Box::new(HirExpr::Bool(true, dummy_span())),
            then: HirBlock {
                stmts: vec![HirStmt::Expr(
                    Box::new(HirExpr::Int(1, dummy_span())),
                    dummy_span(),
                )],
                span: dummy_span(),
            },
            else_: Some(HirBlock {
                stmts: vec![HirStmt::Expr(
                    Box::new(HirExpr::Int(0, dummy_span())),
                    dummy_span(),
                )],
                span: dummy_span(),
            }),
            span: dummy_span(),
        }],
    );
    let mut lowerer = MirLower::new();
    let mir_func = lowerer.lower_function(hir_func).unwrap();
    let return_local = match &mir_func.blocks.last().unwrap().terminator {
        MirTerminator::Return {
            value: Some(local), ..
        } => *local,
        other => panic!("tail if statement should return its branch value, got {other:?}"),
    };
    let branch_result_writes = mir_func
        .blocks
        .iter()
        .flat_map(|block| &block.insts)
        .filter(|inst| {
            matches!(inst,
                MirInst::Assign { dest, value: MirValue::Local(_), .. } if *dest == return_local
            )
        })
        .count();
    assert_eq!(
        branch_result_writes, 2,
        "both if branches must write the merge result"
    );
}

#[test]
fn test_lower_call() {
    let hir_func = dummy_hir_fn(
        "caller",
        vec![HirStmt::Return(
            Some(Box::new(HirExpr::Call {
                callee: Box::new(HirExpr::Ident("callee".to_string(), dummy_span())),
                args: vec![HirExpr::Int(1, dummy_span())],
                span: dummy_span(),
            })),
            dummy_span(),
        )],
    );
    let mut lowerer = MirLower::new();
    let mir_func = lowerer.lower_function(hir_func).unwrap();
    assert!(!mir_func.blocks.is_empty());
}

#[test]
fn test_lower_float() {
    let hir_func = dummy_hir_fn(
        "f",
        vec![HirStmt::Return(
            Some(Box::new(HirExpr::Float(3.14, dummy_span()))),
            dummy_span(),
        )],
    );
    let mut lowerer = MirLower::new();
    let mir_func = lowerer.lower_function(hir_func).unwrap();
    assert!(!mir_func.locals.is_empty());
}

#[test]
fn test_lower_block_expr() {
    let hir_func = dummy_hir_fn(
        "f",
        vec![HirStmt::Return(
            Some(Box::new(HirExpr::Block(HirBlock {
                stmts: vec![HirStmt::Expr(
                    Box::new(HirExpr::Int(1, dummy_span())),
                    dummy_span(),
                )],
                span: dummy_span(),
            }))),
            dummy_span(),
        )],
    );
    let mut lowerer = MirLower::new();
    let mir_func = lowerer.lower_function(hir_func).unwrap();
    assert!(!mir_func.blocks.is_empty());
}

#[test]
fn test_lower_unary_not() {
    let hir_func = dummy_hir_fn(
        "f",
        vec![HirStmt::Return(
            Some(Box::new(HirExpr::UnOp {
                op: HirUnOp::Not,
                expr: Box::new(HirExpr::Bool(false, dummy_span())),
                span: dummy_span(),
            })),
            dummy_span(),
        )],
    );
    let mut lowerer = MirLower::new();
    let mir_func = lowerer.lower_function(hir_func).unwrap();
    assert!(!mir_func.blocks.is_empty());
}

#[test]
fn test_lower_mir_binop_all_variants() {
    use HirBinOp::*;
    let ops = [Add, Sub, Mul, Div, Mod, Eq, Ne, Lt, Le, Gt, Ge, And, Or];
    for op in &ops {
        let mir_op = lower_mir_binop(*op);
        assert!(mir_op.is_some(), "MirBinOp should support {op:?}");
    }
    assert!(
        lower_mir_binop(Range).is_none(),
        "Range must not silently become Add"
    );
}

// --- Regression tests: BUG-K01 (return inside loop must stay a return) ---

fn count_real_returns(blocks: &[MirBlock]) -> usize {
    blocks
        .iter()
        .filter(|b| matches!(b.terminator, MirTerminator::Return { .. }) && b.name == "unreachable")
        .count()
}

#[test]
fn test_return_inside_while_stays_return() {
    let hir_func = dummy_hir_fn(
        "f",
        vec![HirStmt::While {
            cond: Box::new(HirExpr::Bool(true, dummy_span())),
            body: HirBlock {
                stmts: vec![HirStmt::Return(
                    Some(Box::new(HirExpr::Int(5, dummy_span()))),
                    dummy_span(),
                )],
                span: dummy_span(),
            },
            span: dummy_span(),
        }],
    );
    let mut lowerer = MirLower::new();
    let mir_func = lowerer.lower_function(hir_func).unwrap();
    assert_eq!(
        count_real_returns(&mir_func.blocks),
        1,
        "`return` inside while must keep its Return terminator"
    );
}

#[test]
fn test_return_inside_for_stays_return() {
    let hir_func = dummy_hir_fn(
        "f",
        vec![HirStmt::For {
            var: "i".to_string(),
            iterable: Box::new(HirExpr::Int(10, dummy_span())),
            body: HirBlock {
                stmts: vec![HirStmt::Return(
                    Some(Box::new(HirExpr::Int(7, dummy_span()))),
                    dummy_span(),
                )],
                span: dummy_span(),
            },
            span: dummy_span(),
        }],
    );
    let mut lowerer = MirLower::new();
    let mir_func = lowerer.lower_function(hir_func).unwrap();
    assert_eq!(
        count_real_returns(&mir_func.blocks),
        1,
        "`return` inside for must keep its Return terminator"
    );
    // Latch block must exist and jump back to cond
    let latch = mir_func.blocks.iter().find(|b| b.name == "for_latch_i");
    assert!(latch.is_some(), "for loop must have a latch block");
}

#[test]
fn test_return_inside_loop_stays_return() {
    let hir_func = dummy_hir_fn(
        "f",
        vec![HirStmt::Loop {
            body: HirBlock {
                stmts: vec![HirStmt::Return(
                    Some(Box::new(HirExpr::Int(9, dummy_span()))),
                    dummy_span(),
                )],
                span: dummy_span(),
            },
            span: dummy_span(),
        }],
    );
    let mut lowerer = MirLower::new();
    let mir_func = lowerer.lower_function(hir_func).unwrap();
    assert_eq!(
        count_real_returns(&mir_func.blocks),
        1,
        "`return` inside loop must keep its Return terminator"
    );
}

// --- Regression test: BUG-K02 (if-expression mid-function must not hijack flow) ---

// --- Regression tests: BUG-K03 (match must compare, not always arm 0) ---

fn match_hir_fn(arms: Vec<HirPattern>) -> HirFunction {
    let bodies = vec![
        HirExpr::Int(10, dummy_span()),
        HirExpr::Int(20, dummy_span()),
        HirExpr::Int(30, dummy_span()),
    ];
    let matched: Vec<(HirPattern, HirExpr)> = arms.into_iter().zip(bodies).collect();
    dummy_hir_fn(
        "f",
        vec![HirStmt::Return(
            Some(Box::new(HirExpr::Match {
                expr: Box::new(HirExpr::Ident("x".to_string(), dummy_span())),
                arms: matched,
                span: dummy_span(),
            })),
            dummy_span(),
        )],
    )
}

#[test]
fn test_match_literal_chain_structure() {
    let hir_func = dummy_hir_fn(
        "f",
        vec![HirStmt::Return(
            Some(Box::new(HirExpr::Match {
                expr: Box::new(HirExpr::Ident("x".to_string(), dummy_span())),
                arms: vec![
                    (
                        HirPattern::Literal(brak_ir_hir::hir::HirLiteral::Int(1)),
                        HirExpr::Int(10, dummy_span()),
                    ),
                    (
                        HirPattern::Literal(brak_ir_hir::hir::HirLiteral::Int(2)),
                        HirExpr::Int(20, dummy_span()),
                    ),
                    (HirPattern::Wildcard, HirExpr::Int(30, dummy_span())),
                ],
                span: dummy_span(),
            })),
            dummy_span(),
        )],
    );
    let mut lowerer = MirLower::new();
    // bind scrutinee param
    lowerer.get_or_create_local("x", MirType::I32);
    let f = lowerer.lower_function(hir_func).unwrap();
    // Must contain comparison blocks against each literal, not just fall to arm 0.
    let eq_count = f
        .blocks
        .iter()
        .map(|b| {
            b.insts
                .iter()
                .filter(|i| {
                    matches!(
                        i,
                        MirInst::Assign {
                            value: MirValue::BinOp {
                                op: MirBinOp::Eq,
                                ..
                            },
                            ..
                        }
                    )
                })
                .count()
        })
        .sum::<usize>();
    assert_eq!(
        eq_count, 2,
        "two literal patterns need two Eq checks, got {eq_count}"
    );
    // No block may terminate Return except the final one (BUG-K02 family).
    let rets: usize = f
        .blocks
        .iter()
        .filter(|b| matches!(b.terminator, MirTerminator::Return { .. }))
        .count();
    assert_eq!(rets, 1, "match expression must not inject returns");
}

#[test]
fn test_match_wildcard_stops_chain() {
    let hir_func = match_hir_fn(vec![
        HirPattern::Binding("v".to_string()),
        HirPattern::Wildcard,
    ]);
    let mut lowerer = MirLower::new();
    lowerer.get_or_create_local("x", MirType::I32);
    let f = lowerer.lower_function(hir_func).unwrap();
    // Binding arm catches everything: no Eq checks at all.
    let eq_count = f
        .blocks
        .iter()
        .map(|b| {
            b.insts
                .iter()
                .filter(|i| {
                    matches!(
                        i,
                        MirInst::Assign {
                            value: MirValue::BinOp {
                                op: MirBinOp::Eq,
                                ..
                            },
                            ..
                        }
                    )
                })
                .count()
        })
        .sum::<usize>();
    assert_eq!(eq_count, 0);
    assert!(
        f.locals.iter().any(|l| l.name == "v"),
        "binding pattern creates local"
    );
}

#[test]
fn test_if_expr_mid_function_does_not_terminate() {
    let hir_func = dummy_hir_fn(
        "f",
        vec![
            HirStmt::Let {
                name: "y".to_string(),
                ty: HirType::I32,
                value: Some(Box::new(HirExpr::If {
                    cond: Box::new(HirExpr::Bool(true, dummy_span())),
                    then: Box::new(HirExpr::Int(1, dummy_span())),
                    else_: Box::new(HirExpr::Int(0, dummy_span())),
                    span: dummy_span(),
                })),
                span: dummy_span(),
            },
            HirStmt::Return(
                Some(Box::new(HirExpr::Ident("y".to_string(), dummy_span()))),
                dummy_span(),
            ),
        ],
    );
    let mut lowerer = MirLower::new();
    let mir_func = lowerer.lower_function(hir_func).unwrap();
    // The merge block must NOT terminate with Return — only the final
    // explicit `return y` may.
    assert_eq!(
        count_real_returns(&mir_func.blocks),
        1,
        "mid-function if-expression must not inject a synthetic function return"
    );
    let last = mir_func.blocks.last().unwrap();
    assert!(
        matches!(last.terminator, MirTerminator::Return { .. }),
        "function must end with the explicit return"
    );
}
