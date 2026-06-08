use std::collections::HashSet;

use brak_core::{Diagnostic, Diagnostics};
use brak_ir_ast::ast as ast;

use crate::hir::*;

pub struct HirLower;

impl Default for HirLower {
    fn default() -> Self {
        Self::new()
    }
}

impl HirLower {
    pub fn new() -> Self {
        Self
    }

    pub fn lower(&self, program: ast::Program) -> Result<HirProgram, Diagnostics> {
        let mut diags = Diagnostics::new();
        let mut items = vec![];
        let mut seen_names: HashSet<String> = HashSet::new();
        for item in program.items {
            match &item {
                ast::Item::FnDef(f) => {
                    if !seen_names.insert(f.name.name.clone()) {
                        diags.push(
                            Diagnostic::error(format!(
                                "duplicate definition of function `{}`", f.name.name
                            )).with_span(f.name.span)
                        );
                        continue;
                    }
                }
                ast::Item::ExternFn(e) => {
                    if !seen_names.insert(e.name.name.clone()) {
                        diags.push(
                            Diagnostic::error(format!(
                                "duplicate definition of function `{}`", e.name.name
                            )).with_span(e.name.span)
                        );
                        continue;
                    }
                }
                ast::Item::Let(l) => {
                    if !seen_names.insert(l.name.name.clone()) {
                        diags.push(
                            Diagnostic::error(format!(
                                "duplicate definition of `{}`", l.name.name
                            )).with_span(l.name.span)
                        );
                        continue;
                    }
                }
                _ => {} // other items don't have duplicate checking yet
            }
            match self.lower_item(item) {
                Ok(i) => items.push(i),
                Err(d) => diags.extend(d),
            }
        }
        if diags.has_errors() {
            return Err(diags);
        }
        Ok(HirProgram { items })
    }

    fn lower_item(&self, item: ast::Item) -> Result<HirItem, Diagnostics> {
        match item {
            ast::Item::FnDef(f) => {
                let params: Vec<HirParam> = f
                    .params
                    .into_iter()
                    .map(|p| HirParam {
                        name: p.name.name,
                        ty: p.ty.map(lower_type).unwrap_or(HirType::I32),
                        span: p.span,
                    })
                    .collect();
                let ret_ty = f.ret_ty.map(lower_type).unwrap_or(HirType::Void);
                let body = self.lower_block(f.body);
                Ok(HirItem::Function(HirFunction {
                    name: f.name.name,
                    params,
                    ret_ty,
                    body,
                    span: f.span,
                }))
            }
            ast::Item::ExternFn(e) => {
                let params: Vec<HirParam> = e
                    .params
                    .into_iter()
                    .map(|p| HirParam {
                        name: p.name.name,
                        ty: p.ty.map(lower_type).unwrap_or(HirType::I32),
                        span: p.span,
                    })
                    .collect();
                let ret_ty = e.ret_ty.map(lower_type).unwrap_or(HirType::Void);
                Ok(HirItem::ExternFunction(HirExternFunction {
                    name: e.name.name,
                    params,
                    ret_ty,
                    abi: e.abi,
                    span: e.span,
                }))
            }
            ast::Item::Let(l) => {
                let value = l.value.map(|v| Box::new(self.lower_expr(*v)));
                Ok(HirItem::GlobalLet(HirGlobalLet {
                    name: l.name.name,
                    ty: l.ty.map(lower_type).unwrap_or(HirType::I32),
                    value,
                    span: l.span,
                }))
            }
            _ => {
                let mut diags = Diagnostics::new();
                diags.push(
                    Diagnostic::error("unsupported item type for lowering (struct/enum/trait/impl/use/mod/const/static)".to_string())
                );
                Err(diags)
            }
        }
    }

    pub fn lower_block(&self, block: ast::Block) -> HirBlock {
        let stmts: Vec<HirStmt> = block
            .stmts
            .into_iter()
            .map(|s| self.lower_stmt(s))
            .collect();
        HirBlock {
            stmts,
            span: block.span,
        }
    }

    fn lower_stmt(&self, stmt: ast::Stmt) -> HirStmt {
        match stmt {
            ast::Stmt::Let(l) => {
                let value = l.value.map(|v| Box::new(self.lower_expr(*v)));
                HirStmt::Let {
                    name: l.name.name,
                    ty: l.ty.map(lower_type).unwrap_or(HirType::I32),
                    value,
                    span: l.span,
                }
            }
            ast::Stmt::Expr(e) => {
                let lowered = self.lower_expr(e);
                let span = lowered.span();
                HirStmt::Expr(Box::new(lowered), span)
            }
            ast::Stmt::Return(e, span) => {
                HirStmt::Return(e.map(|v| Box::new(self.lower_expr(v))), span)
            }
            ast::Stmt::If { cond, then, else_, span } => HirStmt::If {
                cond: Box::new(self.lower_expr(*cond)),
                then: self.lower_block(then),
                else_: else_.map(|b| self.lower_block(b)),
                span,
            },
            ast::Stmt::While { cond, body, span } => HirStmt::While {
                cond: Box::new(self.lower_expr(*cond)),
                body: self.lower_block(body),
                span,
            },
            ast::Stmt::Break(span) => HirStmt::Break(span),
            ast::Stmt::Continue(span) => HirStmt::Continue(span),
            ast::Stmt::Loop { body, span } => HirStmt::Loop {
                body: self.lower_block(body),
                span,
            },
            ast::Stmt::For { var, iterable, body, span } => HirStmt::For {
                var: var.name,
                iterable: Box::new(self.lower_expr(*iterable)),
                body: self.lower_block(body),
                span,
            },
        }
    }

    pub fn lower_expr(&self, expr: ast::Expr) -> HirExpr {
        match expr {
            ast::Expr::Int(i, span) => HirExpr::Int(i, span),
            ast::Expr::Float(f, span) => HirExpr::Float(f, span),
            ast::Expr::Bool(b, span) => HirExpr::Bool(b, span),
            ast::Expr::String(s, span) => HirExpr::String(s, span),
            ast::Expr::Ident(id) => HirExpr::Ident(id.name, id.span),
            ast::Expr::Assign(lhs, rhs, span) => {
                match *lhs {
                    ast::Expr::Ident(id) => HirExpr::Assign(id.name, Box::new(self.lower_expr(*rhs)), span),
                    ast::Expr::Field { object, field, .. } => {
                        let dotted = match *object {
                            ast::Expr::Ident(id) => format!("{}.{}", id.name, field.name),
                            _ => unreachable!("parser should reject non-ident field object"),
                        };
                        HirExpr::Assign(dotted, Box::new(self.lower_expr(*rhs)), span)
                    }
                    _ => unreachable!("parser should reject non-ident LHS"),
                }
            }
            ast::Expr::BinOp { op, lhs, rhs, span } => HirExpr::BinOp {
                op: lower_binop(op),
                lhs: Box::new(self.lower_expr(*lhs)),
                rhs: Box::new(self.lower_expr(*rhs)),
                span,
            },
            ast::Expr::UnOp { op, expr, span } => HirExpr::UnOp {
                op: lower_unop(op),
                expr: Box::new(self.lower_expr(*expr)),
                span,
            },
            ast::Expr::Call { callee, args, span } => HirExpr::Call {
                callee: Box::new(self.lower_expr(*callee)),
                args: args.into_iter().map(|a| self.lower_expr(a)).collect(),
                span,
            },
            ast::Expr::If { cond, then, else_, span } => HirExpr::If {
                cond: Box::new(self.lower_expr(*cond)),
                then: Box::new(self.lower_expr(*then)),
                else_: Box::new(self.lower_expr(*else_)),
                span,
            },
            ast::Expr::Block(b) => HirExpr::Block(self.lower_block(b)),
            ast::Expr::Match { expr, arms, span } => {
                let lowered_arms = arms.into_iter().map(|arm| {
                    let pat = self.lower_expr(ast::Expr::String(arm.pattern.to_string(), arm.span));
                    let body = self.lower_expr(arm.body);
                    (pat, body)
                }).collect();
                HirExpr::Match {
                    expr: Box::new(self.lower_expr(*expr)),
                    arms: lowered_arms,
                    span,
                }
            }
            ast::Expr::Field { object, field, span } => HirExpr::Field {
                object: Box::new(self.lower_expr(*object)),
                field: field.name,
                span,
            },
        }
    }
}

fn lower_type(ty: ast::Type) -> HirType {
    match ty {
        ast::Type::I32 => HirType::I32,
        ast::Type::I64 => HirType::I64,
        ast::Type::F32 => HirType::F32,
        ast::Type::F64 => HirType::F64,
        ast::Type::Bool => HirType::Bool,
        ast::Type::String => HirType::String,
        ast::Type::Void => HirType::Void,
        ast::Type::Named(s) => HirType::Named(s),
        ast::Type::Ptr(t) => HirType::Ptr(Box::new(lower_type(*t))),
        ast::Type::Ref(t) => HirType::Ref(Box::new(lower_type(*t))),
        ast::Type::Array(t, n) => HirType::Array(Box::new(lower_type(*t)), n),
        ast::Type::Slice(t) => HirType::Slice(Box::new(lower_type(*t))),
        ast::Type::Fn(args, ret) => {
            let args: Vec<HirType> = args.into_iter().map(lower_type).collect();
            HirType::Fn(args, Box::new(lower_type(*ret)))
        }
    }
}

fn lower_binop(op: ast::BinOp) -> HirBinOp {
    match op {
        ast::BinOp::Add => HirBinOp::Add,
        ast::BinOp::Sub => HirBinOp::Sub,
        ast::BinOp::Mul => HirBinOp::Mul,
        ast::BinOp::Div => HirBinOp::Div,
        ast::BinOp::Mod => HirBinOp::Mod,
        ast::BinOp::Eq => HirBinOp::Eq,
        ast::BinOp::Ne => HirBinOp::Ne,
        ast::BinOp::Lt => HirBinOp::Lt,
        ast::BinOp::Le => HirBinOp::Le,
        ast::BinOp::Gt => HirBinOp::Gt,
        ast::BinOp::Ge => HirBinOp::Ge,
        ast::BinOp::And => HirBinOp::And,
        ast::BinOp::Or => HirBinOp::Or,
        ast::BinOp::BitAnd => HirBinOp::BitAnd,
        ast::BinOp::BitOr => HirBinOp::BitOr,
        ast::BinOp::BitXor => HirBinOp::BitXor,
        ast::BinOp::Shl => HirBinOp::Shl,
        ast::BinOp::Shr => HirBinOp::Shr,
        ast::BinOp::Range => HirBinOp::Range,
    }
}

fn lower_unop(op: ast::UnOp) -> HirUnOp {
    match op {
        ast::UnOp::Neg => HirUnOp::Neg,
        ast::UnOp::Not => HirUnOp::Not,
        ast::UnOp::BitNot => HirUnOp::BitNot,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use brak_core::{Span, DUMMY_SPAN};

    fn dummy_span() -> Span { DUMMY_SPAN }

    fn ident(name: &str) -> ast::Ident {
        ast::Ident { name: name.to_string(), span: dummy_span() }
    }

    fn lower_ast_to_hir(program: ast::Program) -> HirProgram {
        let lowerer = HirLower::new();
        lowerer.lower(program).unwrap()
    }

    #[test]
    fn test_lower_fn_no_params() {
        let ast = ast::Program {
            items: vec![ast::Item::FnDef(ast::FnDef {
                name: ident("main"),
                params: vec![],
                ret_ty: None,
                body: ast::Block {
                    stmts: vec![ast::Stmt::Return(Some(ast::Expr::Int(42, dummy_span())), dummy_span())],
                    span: dummy_span(),
                },
                span: dummy_span(),
            })],
        };
        let hir = lower_ast_to_hir(ast);
        assert_eq!(hir.items.len(), 1);
        match &hir.items[0] {
            HirItem::Function(f) => {
                assert_eq!(f.name, "main");
                assert_eq!(f.ret_ty, HirType::Void);
            }
            _ => panic!("expected function"),
        }
    }

    #[test]
    fn test_lower_fn_with_ret_ty() {
        let ast = ast::Program {
            items: vec![ast::Item::FnDef(ast::FnDef {
                name: ident("add"),
                params: vec![
                    ast::Param { name: ident("a"), ty: Some(ast::Type::I32), span: dummy_span() },
                    ast::Param { name: ident("b"), ty: Some(ast::Type::I32), span: dummy_span() },
                ],
                ret_ty: Some(ast::Type::I32),
                body: ast::Block {
                    stmts: vec![
                        ast::Stmt::Return(Some(
                            ast::Expr::BinOp {
                                op: ast::BinOp::Add,
                                lhs: Box::new(ast::Expr::Ident(ident("a"))),
                                rhs: Box::new(ast::Expr::Ident(ident("b"))),
                                span: dummy_span(),
                            }
                        ), dummy_span())
                    ],
                    span: dummy_span(),
                },
                span: dummy_span(),
            })],
        };
        let hir = lower_ast_to_hir(ast);
        match &hir.items[0] {
            HirItem::Function(f) => {
                assert_eq!(f.name, "add");
                assert_eq!(f.ret_ty, HirType::I32);
                assert_eq!(f.params.len(), 2);
                assert_eq!(f.params[0].name, "a");
                assert_eq!(f.params[0].ty, HirType::I32);
            }
            _ => panic!("expected function"),
        }
    }

    #[test]
    fn test_lower_extern_fn() {
        let ast = ast::Program {
            items: vec![ast::Item::ExternFn(ast::ExternFn {
                name: ident("puts"),
                params: vec![ast::Param { name: ident("s"), ty: Some(ast::Type::String), span: dummy_span() }],
                ret_ty: Some(ast::Type::I32),
                abi: "C".into(),
                span: dummy_span(),
            })],
        };
        let hir = lower_ast_to_hir(ast);
        match &hir.items[0] {
            HirItem::ExternFunction(e) => {
                assert_eq!(e.name, "puts");
                assert_eq!(e.abi, "C");
                assert_eq!(e.params.len(), 1);
                assert_eq!(e.params[0].ty, HirType::String);
            }
            _ => panic!("expected extern function"),
        }
    }

    #[test]
    fn test_lower_global_let() {
        let ast = ast::Program {
            items: vec![ast::Item::Let(ast::Let {
                name: ident("MAX"),
                ty: Some(ast::Type::I32),
                value: Some(Box::new(ast::Expr::Int(100, dummy_span()))),
                span: dummy_span(),
            })],
        };
        let hir = lower_ast_to_hir(ast);
        match &hir.items[0] {
            HirItem::GlobalLet(l) => {
                assert_eq!(l.name, "MAX");
                assert_eq!(l.ty, HirType::I32);
                assert!(l.value.is_some());
            }
            _ => panic!("expected global let"),
        }
    }

    #[test]
    fn test_lower_all_binops() {
        use ast::BinOp::*;
        let ops = [Add, Sub, Mul, Div, Mod, Eq, Ne, Lt, Le, Gt, Ge, And, Or, BitAnd, BitOr, BitXor, Shl, Shr, Range];
        for op in &ops {
            let hir_op = lower_binop(*op);
            let s = format!("{hir_op:?}");
            assert!(!s.is_empty(), "binop should have a lowering");
        }
    }

    #[test]
    fn test_lower_all_unops() {
        use ast::UnOp::*;
        for op in &[Neg, Not, BitNot] {
            let hir_op = lower_unop(*op);
            let s = format!("{hir_op:?}");
            assert!(!s.is_empty(), "unop should have a lowering");
        }
    }

    #[test]
    fn test_lower_all_types() {
        use ast::Type::*;
        let types = vec![
            I32, I64, F32, F64, Bool, String, Void,
            Named("T".into()),
            Ptr(Box::new(I32)),
            Ref(Box::new(I64)),
            Array(Box::new(Bool), 10),
            Slice(Box::new(String)),
            Fn(vec![I32], Box::new(Void)),
        ];
        for ty in types {
            let hir_ty = lower_type(ty);
            let s = format!("{hir_ty}");
            assert!(!s.is_empty(), "type should have a lowering");
        }
    }

    #[test]
    fn test_lower_duplicate_fn_detection() {
        let ast = ast::Program {
            items: vec![
                ast::Item::FnDef(ast::FnDef { name: ident("f"), params: vec![], ret_ty: None, body: ast::Block { stmts: vec![], span: dummy_span() }, span: dummy_span() }),
                ast::Item::FnDef(ast::FnDef { name: ident("f"), params: vec![], ret_ty: None, body: ast::Block { stmts: vec![], span: dummy_span() }, span: dummy_span() }),
            ],
        };
        let lowerer = HirLower::new();
        let result = lowerer.lower(ast);
        assert!(result.is_err(), "duplicate function should error");
    }

    #[test]
    fn test_lower_stmt_if_else() {
        let ast_prog = ast::Program {
            items: vec![ast::Item::FnDef(ast::FnDef {
                name: ident("f"),
                params: vec![],
                ret_ty: None,
                body: ast::Block {
                    stmts: vec![ast::Stmt::If {
                        cond: Box::new(ast::Expr::Bool(true, dummy_span())),
                        then: ast::Block { stmts: vec![], span: dummy_span() },
                        else_: Some(ast::Block { stmts: vec![], span: dummy_span() }),
                        span: dummy_span(),
                    }],
                    span: dummy_span(),
                },
                span: dummy_span(),
            })],
        };
        let hir = lower_ast_to_hir(ast_prog);
        match &hir.items[0] {
            HirItem::Function(f) => {
                assert!(matches!(&f.body.stmts[0], HirStmt::If { .. }));
            }
            _ => panic!("expected function"),
        }
    }

    #[test]
    fn test_lower_stmt_loop() {
        let ast_prog = ast::Program {
            items: vec![ast::Item::FnDef(ast::FnDef {
                name: ident("f"),
                params: vec![],
                ret_ty: None,
                body: ast::Block {
                    stmts: vec![ast::Stmt::Loop {
                        body: ast::Block { stmts: vec![], span: dummy_span() },
                        span: dummy_span(),
                    }],
                    span: dummy_span(),
                },
                span: dummy_span(),
            })],
        };
        let hir = lower_ast_to_hir(ast_prog);
        match &hir.items[0] {
            HirItem::Function(f) => {
                assert!(matches!(&f.body.stmts[0], HirStmt::Loop { .. }));
            }
            _ => panic!("expected function"),
        }
    }

    #[test]
    fn test_type_check_valid() {
        use crate::typeck::TypeChecker;
        let ast = ast::Program {
            items: vec![ast::Item::FnDef(ast::FnDef {
                name: ident("f"),
                params: vec![ast::Param { name: ident("x"), ty: Some(ast::Type::I32), span: dummy_span() }],
                ret_ty: Some(ast::Type::I32),
                body: ast::Block {
                    stmts: vec![ast::Stmt::Return(Some(
                        ast::Expr::BinOp {
                            op: ast::BinOp::Add,
                            lhs: Box::new(ast::Expr::Ident(ident("x"))),
                            rhs: Box::new(ast::Expr::Int(1, dummy_span())),
                            span: dummy_span(),
                        }
                    ), dummy_span())],
                    span: dummy_span(),
                },
                span: dummy_span(),
            })],
        };
        let hir = lower_ast_to_hir(ast);
        let mut checker = TypeChecker::new();
        let result = checker.check(&hir);
        assert!(result.is_ok(), "valid program should type check");
    }

    #[test]
    fn test_type_check_mismatch() {
        use crate::typeck::TypeChecker;
        let ast = ast::Program {
            items: vec![ast::Item::FnDef(ast::FnDef {
                name: ident("f"),
                params: vec![ast::Param { name: ident("x"), ty: Some(ast::Type::I32), span: dummy_span() }],
                ret_ty: Some(ast::Type::I32),
                body: ast::Block {
                    stmts: vec![ast::Stmt::Let(ast::Let {
                        name: ident("y"),
                        ty: Some(ast::Type::Bool),
                        value: Some(Box::new(ast::Expr::Ident(ident("x")))),
                        span: dummy_span(),
                    })],
                    span: dummy_span(),
                },
                span: dummy_span(),
            })],
        };
        let hir = lower_ast_to_hir(ast);
        let mut checker = TypeChecker::new();
        let result = checker.check(&hir);
        assert!(result.is_err(), "type mismatch should error");
    }

    #[test]
    fn test_lower_stmt_let() {
        let ast_prog = ast::Program {
            items: vec![ast::Item::FnDef(ast::FnDef {
                name: ident("f"),
                params: vec![],
                ret_ty: None,
                body: ast::Block {
                    stmts: vec![ast::Stmt::Let(ast::Let {
                        name: ident("x"),
                        ty: Some(ast::Type::I32),
                        value: Some(Box::new(ast::Expr::Int(42, dummy_span()))),
                        span: dummy_span(),
                    })],
                    span: dummy_span(),
                },
                span: dummy_span(),
            })],
        };
        let hir = lower_ast_to_hir(ast_prog);
        match &hir.items[0] {
            HirItem::Function(f) => {
                assert!(matches!(&f.body.stmts[0], HirStmt::Let { .. }));
                if let HirStmt::Let { name, ty, value, .. } = &f.body.stmts[0] {
                    assert_eq!(name, "x");
                    assert_eq!(*ty, HirType::I32);
                    assert!(value.is_some());
                }
            }
            _ => panic!("expected function"),
        }
    }

    #[test]
    fn test_lower_stmt_while() {
        let ast_prog = ast::Program {
            items: vec![ast::Item::FnDef(ast::FnDef {
                name: ident("f"),
                params: vec![],
                ret_ty: None,
                body: ast::Block {
                    stmts: vec![ast::Stmt::While {
                        cond: Box::new(ast::Expr::Bool(true, dummy_span())),
                        body: ast::Block { stmts: vec![], span: dummy_span() },
                        span: dummy_span(),
                    }],
                    span: dummy_span(),
                },
                span: dummy_span(),
            })],
        };
        let hir = lower_ast_to_hir(ast_prog);
        match &hir.items[0] {
            HirItem::Function(f) => {
                assert!(matches!(&f.body.stmts[0], HirStmt::While { .. }));
            }
            _ => panic!("expected function"),
        }
    }

    #[test]
    fn test_lower_stmt_break_continue() {
        let ast_prog = ast::Program {
            items: vec![ast::Item::FnDef(ast::FnDef {
                name: ident("f"),
                params: vec![],
                ret_ty: None,
                body: ast::Block {
                    stmts: vec![
                        ast::Stmt::Break(dummy_span()),
                        ast::Stmt::Continue(dummy_span()),
                    ],
                    span: dummy_span(),
                },
                span: dummy_span(),
            })],
        };
        let hir = lower_ast_to_hir(ast_prog);
        match &hir.items[0] {
            HirItem::Function(f) => {
                assert!(matches!(&f.body.stmts[0], HirStmt::Break(_)));
                assert!(matches!(&f.body.stmts[1], HirStmt::Continue(_)));
            }
            _ => panic!("expected function"),
        }
    }

    #[test]
    fn test_lower_stmt_for() {
        let ast_prog = ast::Program {
            items: vec![ast::Item::FnDef(ast::FnDef {
                name: ident("f"),
                params: vec![],
                ret_ty: None,
                body: ast::Block {
                    stmts: vec![ast::Stmt::For {
                        var: ident("i"),
                        iterable: Box::new(ast::Expr::Ident(ident("range"))),
                        body: ast::Block { stmts: vec![], span: dummy_span() },
                        span: dummy_span(),
                    }],
                    span: dummy_span(),
                },
                span: dummy_span(),
            })],
        };
        let hir = lower_ast_to_hir(ast_prog);
        match &hir.items[0] {
            HirItem::Function(f) => {
                assert!(matches!(&f.body.stmts[0], HirStmt::For { .. }));
                if let HirStmt::For { var, .. } = &f.body.stmts[0] {
                    assert_eq!(var, "i");
                }
            }
            _ => panic!("expected function"),
        }
    }

    #[test]
    fn test_lower_expr_float() {
        let ast_prog = ast::Program {
            items: vec![ast::Item::FnDef(ast::FnDef {
                name: ident("f"),
                params: vec![],
                ret_ty: None,
                body: ast::Block {
                    stmts: vec![ast::Stmt::Return(Some(ast::Expr::Float(3.14, dummy_span())), dummy_span())],
                    span: dummy_span(),
                },
                span: dummy_span(),
            })],
        };
        let hir = lower_ast_to_hir(ast_prog);
        match &hir.items[0] {
            HirItem::Function(f) => {
                if let HirStmt::Return(Some(e), _) = &f.body.stmts[0] {
                    assert!(matches!(e.as_ref(), HirExpr::Float(3.14, _)));
                } else {
                    panic!("expected return with float");
                }
            }
            _ => panic!("expected function"),
        }
    }

    #[test]
    fn test_lower_expr_string() {
        let ast_prog = ast::Program {
            items: vec![ast::Item::FnDef(ast::FnDef {
                name: ident("f"),
                params: vec![],
                ret_ty: None,
                body: ast::Block {
                    stmts: vec![ast::Stmt::Return(Some(ast::Expr::String("hello".into(), dummy_span())), dummy_span())],
                    span: dummy_span(),
                },
                span: dummy_span(),
            })],
        };
        let hir = lower_ast_to_hir(ast_prog);
        match &hir.items[0] {
            HirItem::Function(f) => {
                if let HirStmt::Return(Some(e), _) = &f.body.stmts[0] {
                    assert!(matches!(e.as_ref(), HirExpr::String(s, _) if s == "hello"));
                } else {
                    panic!("expected return with string");
                }
            }
            _ => panic!("expected function"),
        }
    }

    #[test]
    fn test_lower_expr_ident() {
        let ast_prog = ast::Program {
            items: vec![ast::Item::FnDef(ast::FnDef {
                name: ident("f"),
                params: vec![ast::Param { name: ident("x"), ty: None, span: dummy_span() }],
                ret_ty: None,
                body: ast::Block {
                    stmts: vec![ast::Stmt::Return(Some(ast::Expr::Ident(ident("x"))), dummy_span())],
                    span: dummy_span(),
                },
                span: dummy_span(),
            })],
        };
        let hir = lower_ast_to_hir(ast_prog);
        match &hir.items[0] {
            HirItem::Function(f) => {
                if let HirStmt::Return(Some(e), _) = &f.body.stmts[0] {
                    assert!(matches!(e.as_ref(), HirExpr::Ident(s, _) if s == "x"));
                } else {
                    panic!("expected return with ident");
                }
            }
            _ => panic!("expected function"),
        }
    }

    #[test]
    fn test_lower_expr_assign() {
        let ast_prog = ast::Program {
            items: vec![ast::Item::FnDef(ast::FnDef {
                name: ident("f"),
                params: vec![],
                ret_ty: None,
                body: ast::Block {
                    stmts: vec![ast::Stmt::Expr(ast::Expr::Assign(
                        Box::new(ast::Expr::Ident(ident("x"))),
                        Box::new(ast::Expr::Int(42, dummy_span())),
                        dummy_span(),
                    ))],
                    span: dummy_span(),
                },
                span: dummy_span(),
            })],
        };
        let hir = lower_ast_to_hir(ast_prog);
        match &hir.items[0] {
            HirItem::Function(f) => {
                assert!(matches!(&f.body.stmts[0], HirStmt::Expr(_, _)));
            }
            _ => panic!("expected function"),
        }
    }

    #[test]
    fn test_lower_expr_call() {
        let ast_prog = ast::Program {
            items: vec![ast::Item::FnDef(ast::FnDef {
                name: ident("f"),
                params: vec![],
                ret_ty: None,
                body: ast::Block {
                    stmts: vec![ast::Stmt::Expr(ast::Expr::Call {
                        callee: Box::new(ast::Expr::Ident(ident("g"))),
                        args: vec![ast::Expr::Int(1, dummy_span())],
                        span: dummy_span(),
                    })],
                    span: dummy_span(),
                },
                span: dummy_span(),
            })],
        };
        let hir = lower_ast_to_hir(ast_prog);
        match &hir.items[0] {
            HirItem::Function(f) => {
                assert!(matches!(&f.body.stmts[0], HirStmt::Expr(_, _)));
            }
            _ => panic!("expected function"),
        }
    }

    #[test]
    fn test_lower_expr_if_expr() {
        let ast_prog = ast::Program {
            items: vec![ast::Item::FnDef(ast::FnDef {
                name: ident("f"),
                params: vec![],
                ret_ty: None,
                body: ast::Block {
                    stmts: vec![ast::Stmt::Return(Some(ast::Expr::If {
                        cond: Box::new(ast::Expr::Bool(true, dummy_span())),
                        then: Box::new(ast::Expr::Int(1, dummy_span())),
                        else_: Box::new(ast::Expr::Int(2, dummy_span())),
                        span: dummy_span(),
                    }), dummy_span())],
                    span: dummy_span(),
                },
                span: dummy_span(),
            })],
        };
        let hir = lower_ast_to_hir(ast_prog);
        match &hir.items[0] {
            HirItem::Function(f) => {
                if let HirStmt::Return(Some(e), _) = &f.body.stmts[0] {
                    assert!(matches!(e.as_ref(), HirExpr::If { .. }));
                } else {
                    panic!("expected return with if expr");
                }
            }
            _ => panic!("expected function"),
        }
    }

    #[test]
    fn test_lower_expr_block() {
        let ast_prog = ast::Program {
            items: vec![ast::Item::FnDef(ast::FnDef {
                name: ident("f"),
                params: vec![],
                ret_ty: None,
                body: ast::Block {
                    stmts: vec![ast::Stmt::Return(Some(ast::Expr::Block(ast::Block {
                        stmts: vec![ast::Stmt::Expr(ast::Expr::Int(42, dummy_span()))],
                        span: dummy_span(),
                    })), dummy_span())],
                    span: dummy_span(),
                },
                span: dummy_span(),
            })],
        };
        let hir = lower_ast_to_hir(ast_prog);
        match &hir.items[0] {
            HirItem::Function(f) => {
                if let HirStmt::Return(Some(e), _) = &f.body.stmts[0] {
                    assert!(matches!(e.as_ref(), HirExpr::Block(_)));
                } else {
                    panic!("expected return with block");
                }
            }
            _ => panic!("expected function"),
        }
    }

    #[test]
    fn test_lower_expr_match() {
        let ast_prog = ast::Program {
            items: vec![ast::Item::FnDef(ast::FnDef {
                name: ident("f"),
                params: vec![],
                ret_ty: None,
                body: ast::Block {
                    stmts: vec![ast::Stmt::Return(Some(ast::Expr::Match {
                        expr: Box::new(ast::Expr::Int(1, dummy_span())),
                        arms: vec![ast::MatchArm {
                            pattern: ast::Pattern::Wildcard(dummy_span()),
                            body: ast::Expr::Int(0, dummy_span()),
                            span: dummy_span(),
                        }],
                        span: dummy_span(),
                    }), dummy_span())],
                    span: dummy_span(),
                },
                span: dummy_span(),
            })],
        };
        let hir = lower_ast_to_hir(ast_prog);
        match &hir.items[0] {
            HirItem::Function(f) => {
                if let HirStmt::Return(Some(e), _) = &f.body.stmts[0] {
                    assert!(matches!(e.as_ref(), HirExpr::Match { .. }));
                } else {
                    panic!("expected return with match");
                }
            }
            _ => panic!("expected function"),
        }
    }

    #[test]
    fn test_lower_expr_bool() {
        let ast_prog = ast::Program {
            items: vec![ast::Item::FnDef(ast::FnDef {
                name: ident("f"),
                params: vec![],
                ret_ty: None,
                body: ast::Block {
                    stmts: vec![ast::Stmt::Return(Some(ast::Expr::Bool(false, dummy_span())), dummy_span())],
                    span: dummy_span(),
                },
                span: dummy_span(),
            })],
        };
        let hir = lower_ast_to_hir(ast_prog);
        match &hir.items[0] {
            HirItem::Function(f) => {
                if let HirStmt::Return(Some(e), _) = &f.body.stmts[0] {
                    assert!(matches!(e.as_ref(), HirExpr::Bool(false, _)));
                } else {
                    panic!("expected return with bool");
                }
            }
            _ => panic!("expected function"),
        }
    }

    #[test]
    fn test_lower_duplicate_extern_detection() {
        let ast = ast::Program {
            items: vec![
                ast::Item::ExternFn(ast::ExternFn {
                    name: ident("ext"), params: vec![], ret_ty: None, abi: "C".into(), span: dummy_span(),
                }),
                ast::Item::ExternFn(ast::ExternFn {
                    name: ident("ext"), params: vec![], ret_ty: None, abi: "C".into(), span: dummy_span(),
                }),
            ],
        };
        let lowerer = HirLower::new();
        let result = lowerer.lower(ast);
        assert!(result.is_err(), "duplicate extern function should error");
    }

    #[test]
    fn test_lower_unsupported_item_errors() {
        use ast::Item;
        let ast = ast::Program {
            items: vec![
                Item::Struct(ast::StructDef {
                    vis: ast::Visibility::Private, name: ident("S"), fields: vec![], span: dummy_span(),
                }),
            ],
        };
        let lowerer = HirLower::new();
        let result = lowerer.lower(ast);
        assert!(result.is_err(), "struct lowering should error");
    }
}
