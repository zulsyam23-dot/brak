use std::collections::HashMap;

use brak_core::{Diagnostic, Diagnostics};

use crate::hir::*;

pub struct TypeChecker {
    diags: Diagnostics,
    locals: HashMap<String, HirType>,
    functions: HashMap<String, (Vec<HirType>, HirType)>,
    structs: HashMap<String, HirStruct>,
    enums: HashMap<String, HirEnum>,
    current_ret_ty: Option<HirType>,
}

impl Default for TypeChecker {
    fn default() -> Self {
        Self::new()
    }
}

impl TypeChecker {
    pub fn new() -> Self {
        Self {
            diags: Diagnostics::new(),
            locals: HashMap::new(),
            functions: HashMap::new(),
            structs: HashMap::new(),
            enums: HashMap::new(),
            current_ret_ty: None,
        }
    }

    pub fn check(&mut self, program: &HirProgram) -> Result<(), Diagnostics> {
        for item in &program.items {
            match item {
                HirItem::Function(f) => {
                    let param_tys: Vec<HirType> = f.params.iter().map(|p| p.ty.clone()).collect();
                    self.functions.insert(f.name.clone(), (param_tys, f.ret_ty.clone()));
                }
                HirItem::ExternFunction(e) => {
                    let param_tys: Vec<HirType> = e.params.iter().map(|p| p.ty.clone()).collect();
                    self.functions.insert(e.name.clone(), (param_tys, e.ret_ty.clone()));
                }
                HirItem::Struct(s) => {
                    self.structs.insert(s.name.clone(), s.clone());
                }
                HirItem::Enum(e) => {
                    self.enums.insert(e.name.clone(), e.clone());
                }
                _ => {}
            }
        }

        for item in &program.items {
            if let HirItem::Function(f) = item {
                self.locals.clear();
                for p in &f.params {
                    self.locals.insert(p.name.clone(), p.ty.clone());
                }
                self.current_ret_ty = Some(f.ret_ty.clone());
                self.check_block(&f.body);
            }
        }

        if self.diags.has_errors() {
            return Err(std::mem::replace(&mut self.diags, Diagnostics::new()));
        }
        Ok(())
    }

    fn check_block(&mut self, block: &HirBlock) {
        for stmt in &block.stmts {
            match stmt {
                HirStmt::Let { name, ty, value, span, .. } => {
                    if let Some(v) = value {
                        let val_ty = self.infer_expr(v);
                        if val_ty != *ty {
                            self.diags.push(
                                Diagnostic::error(format!(
                                    "type mismatch: expected {ty}, got {val_ty}"
                                )).with_span(*span)
                            );
                        }
                    }
                    self.locals.insert(name.clone(), ty.clone());
                }
                HirStmt::Expr(e, _) => {
                    self.infer_expr(e);
                }
                HirStmt::Return(v, span) => {
                    if let Some(v) = v {
                        let val_ty = self.infer_expr(v);
                        if let Some(ret_ty) = &self.current_ret_ty {
                            if *ret_ty != HirType::Void && val_ty != *ret_ty {
                                self.diags.push(
                                    Diagnostic::error(format!(
                                        "return type mismatch: expected {ret_ty}, got {val_ty}"
                                    )).with_span(*span)
                                );
                            }
                        }
                    } else {
                        if let Some(ret_ty) = &self.current_ret_ty {
                            if *ret_ty != HirType::Void {
                                self.diags.push(
                                    Diagnostic::error(format!(
                                        "expected return value of type {ret_ty}"
                                    )).with_span(*span)
                                );
                            }
                        }
                    }
                }
                HirStmt::If { cond, then, else_, span, .. } => {
                    let cond_ty = self.infer_expr(cond);
                    if cond_ty != HirType::Bool {
                        self.diags.push(
                            Diagnostic::error(format!(
                                "if condition must be bool, got {cond_ty}"
                            )).with_span(*span)
                        );
                    }
                    self.check_block(then);
                    if let Some(else_block) = else_ {
                        self.check_block(else_block);
                    }
                }
                HirStmt::Loop { body, .. } => {
                    self.check_block(body);
                }
                HirStmt::While { cond, body, span, .. } => {
                    let cond_ty = self.infer_expr(cond);
                    if cond_ty != HirType::Bool {
                        self.diags.push(
                            Diagnostic::error(format!(
                                "while condition must be bool, got {cond_ty}"
                            )).with_span(*span)
                        );
                    }
                    self.check_block(body);
                }
                HirStmt::Break(_) | HirStmt::Continue(_) => {}
                HirStmt::For { var, iterable, body, span } => {
                    let iter_ty = self.infer_expr(iterable);
                    if iter_ty != HirType::I32 {
                        self.diags.push(
                            Diagnostic::error(format!(
                                "for expression must be i32, got {iter_ty}"
                            )).with_span(*span)
                        );
                    }
                    self.locals.insert(var.clone(), HirType::I32);
                    self.check_block(body);
                }
            }
        }
    }

    fn infer_expr(&mut self, expr: &HirExpr) -> HirType {
        match expr {
            HirExpr::Int(_, _) => HirType::I32,
            HirExpr::Float(_, _) => HirType::F64,
            HirExpr::Bool(_, _) => HirType::Bool,
            HirExpr::String(_, _) => HirType::String,
            HirExpr::Assign(name, rhs, span) => {
                let rhs_ty = self.infer_expr(rhs);
                if let Some(var_ty) = self.locals.get(name) {
                    if rhs_ty != *var_ty {
                        self.diags.push(
                            Diagnostic::error(format!(
                                "type mismatch in assignment: variable `{name}` is {var_ty}, got {rhs_ty}"
                            )).with_span(*span)
                        );
                    }
                } else {
                    self.diags.push(
                        Diagnostic::error(format!("undefined variable `{name}` in assignment"))
                            .with_span(*span)
                    );
                }
                rhs_ty
            }
            HirExpr::Ident(name, span) => {
                if let Some(ty) = self.locals.get(name) {
                    ty.clone()
                } else if self.functions.contains_key(name) {
                    HirType::Named(name.clone())
                } else {
                    self.diags.push(
                        Diagnostic::error(format!("undefined variable `{name}`"))
                            .with_span(*span)
                    );
                    HirType::Void
                }
            }
            HirExpr::BinOp { op, lhs, rhs, span } => {
                let lhs_ty = self.infer_expr(lhs);
                let rhs_ty = self.infer_expr(rhs);
                if lhs_ty != rhs_ty {
                    self.diags.push(
                        Diagnostic::error(format!(
                            "type mismatch in binary op: {lhs_ty} vs {rhs_ty}"
                        )).with_span(*span)
                    );
                }
                match op {
                    HirBinOp::Eq | HirBinOp::Ne | HirBinOp::Lt
                    | HirBinOp::Le | HirBinOp::Gt | HirBinOp::Ge
                    | HirBinOp::And | HirBinOp::Or => HirType::Bool,
                    _ => lhs_ty,
                }
            }
            HirExpr::UnOp { expr, .. } => self.infer_expr(expr),
            HirExpr::Call { callee, args, span } => {
                let callee_name = match callee.as_ref() {
                    HirExpr::Ident(name, _) => name.clone(),
                    _ => return HirType::Void,
                };
                if let Some((param_tys, ret_ty)) = self.functions.get(&callee_name).cloned() {
                    if args.len() != param_tys.len() {
                        self.diags.push(
                            Diagnostic::error(format!(
                                "function `{callee_name}` expects {} arguments, got {}",
                                param_tys.len(),
                                args.len()
                            )).with_span(*span)
                        );
                    }
                    for (i, arg) in args.iter().enumerate() {
                        let arg_ty = self.infer_expr(arg);
                        if i < param_tys.len() && arg_ty != param_tys[i] {
                            self.diags.push(
                                Diagnostic::error(format!(
                                    "argument {} type mismatch: expected {}, got {}",
                                    i + 1,
                                    param_tys[i],
                                    arg_ty
                                )).with_span(arg.span())
                            );
                        }
                    }
                    ret_ty
                } else {
                    self.diags.push(
                        Diagnostic::error(format!("undefined function `{callee_name}`"))
                            .with_span(*span)
                    );
                    HirType::I32
                }
            }
            HirExpr::If { cond, then, else_, span } => {
                let cond_ty = self.infer_expr(cond);
                if cond_ty != HirType::Bool {
                    self.diags.push(
                        Diagnostic::error(format!(
                            "if expression condition must be bool, got {cond_ty}"
                        )).with_span(*span)
                    );
                }
                let then_ty = self.infer_expr(then);
                let else_ty = self.infer_expr(else_);
                if then_ty != else_ty {
                    self.diags.push(
                        Diagnostic::error(format!(
                            "if/else type mismatch: then={then_ty}, else={else_ty}"
                        )).with_span(*span)
                    );
                }
                then_ty
            }
            HirExpr::Block(b) => {
                if let Some(stmt) = b.stmts.last() {
                    match stmt {
                        HirStmt::Expr(e, _) => self.infer_expr(e),
                        HirStmt::Let { ty, .. } => ty.clone(),
                        _ => HirType::Void,
                    }
                } else {
                    HirType::Void
                }
            }
            HirExpr::Match { expr, arms, span } => {
                let _expr_ty = self.infer_expr(expr);
                if arms.is_empty() {
                    self.diags.push(
                        Diagnostic::error("match must have at least one arm".to_string())
                            .with_span(*span)
                    );
                    return HirType::Void;
                }
                let arm0_ty = self.infer_expr(&arms[0].1);
                for (i, (pat, body)) in arms.iter().enumerate() {
                    let _pat_ty = self.infer_expr(pat);
                    let body_ty = self.infer_expr(body);
                    if body_ty != arm0_ty {
                        self.diags.push(
                            Diagnostic::error(format!(
                                "match arm {i} type mismatch: expected {arm0_ty}, got {body_ty}"
                            )).with_span(*span)
                        );
                    }
                }
                arm0_ty
            }
            HirExpr::Field { object, field, span } => {
                let obj_ty = self.infer_expr(object);
                
                // Try struct field access
                let struct_name = match &obj_ty {
                    HirType::Named(name) => Some(name),
                    HirType::Ptr(inner) | HirType::Ref(inner) => {
                        if let HirType::Named(name) = inner.as_ref() {
                            Some(name)
                        } else {
                            None
                        }
                    }
                    _ => None,
                };

                if let Some(name) = struct_name {
                    if let Some(s) = self.structs.get(name) {
                        if let Some(f) = s.fields.iter().find(|f| f.name == *field) {
                            return f.ty.clone();
                        }
                    }
                }

                // Fallback to dotted name (for compatibility with current lowering)
                if let HirExpr::Ident(obj_name, _) = object.as_ref() {
                    let dotted = format!("{obj_name}.{field}");
                    if let Some(ty) = self.locals.get(&dotted) {
                        return ty.clone();
                    }
                }

                self.diags.push(
                    Diagnostic::error(format!("undefined field `{field}` for type `{obj_ty}`"))
                        .with_span(*span)
                );
                HirType::Void
            }
            HirExpr::StructInit { name, fields, span } => {
                if let Some(s) = self.structs.get(name).cloned() {
                    for (fname, fexpr) in fields {
                        let fty = self.infer_expr(fexpr);
                        if let Some(field_def) = s.fields.iter().find(|f| f.name == *fname) {
                            if fty != field_def.ty {
                                self.diags.push(
                                    Diagnostic::error(format!(
                                        "field `{fname}` type mismatch: expected {}, got {fty}", field_def.ty
                                    )).with_span(fexpr.span())
                                );
                            }
                        } else {
                            self.diags.push(
                                Diagnostic::error(format!("struct `{name}` has no field `{fname}`"))
                                    .with_span(*span)
                            );
                        }
                    }
                    HirType::Named(name.clone())
                } else {
                    self.diags.push(
                        Diagnostic::error(format!("undefined struct `{name}`"))
                            .with_span(*span)
                    );
                    HirType::Void
                }
            }
            HirExpr::FieldAssign { object, field, value, span } => {
                let obj_ty = self.infer_expr(object);
                let val_ty = self.infer_expr(value);
                
                let struct_name = match &obj_ty {
                    HirType::Named(name) => Some(name),
                    HirType::Ptr(inner) | HirType::Ref(inner) => {
                        if let HirType::Named(name) = inner.as_ref() {
                            Some(name)
                        } else { None }
                    }
                    _ => None,
                };

                if let Some(name) = struct_name {
                    if let Some(s) = self.structs.get(name) {
                        if let Some(f) = s.fields.iter().find(|f| f.name == *field) {
                            if val_ty != f.ty {
                                self.diags.push(
                                    Diagnostic::error(format!(
                                        "field `{field}` type mismatch: expected {}, got {val_ty}", f.ty
                                    )).with_span(*span)
                                );
                            }
                            return f.ty.clone();
                        }
                    }
                }

                self.diags.push(
                    Diagnostic::error(format!("undefined field `{field}` for type `{obj_ty}`"))
                        .with_span(*span)
                );
                HirType::Void
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lower::HirLower;
    use brak_core::{DUMMY_SPAN, Span};
    use brak_ir_ast::ast;

    fn dummy_span() -> Span { DUMMY_SPAN }

    fn ident(name: &str) -> ast::Ident {
        ast::Ident { name: name.to_string(), span: dummy_span() }
    }

    fn check_program(prog: ast::Program) -> Result<(), Diagnostics> {
        let lowerer = HirLower::new();
        let hir = lowerer.lower(prog).unwrap();
        let mut checker = TypeChecker::new();
        checker.check(&hir)
    }

    #[test]
    fn test_typeck_undefined_variable() {
        let ast = ast::Program {
            items: vec![ast::Item::FnDef(ast::FnDef {
                name: ident("f"), params: vec![], ret_ty: Some(ast::Type::I32),
                body: ast::Block {
                    stmts: vec![ast::Stmt::Return(Some(ast::Expr::Ident(ident("x"))), dummy_span())],
                    span: dummy_span(),
                },
                span: dummy_span(),
            })],
        };
        assert!(check_program(ast).is_err());
    }

    #[test]
    fn test_typeck_undefined_function() {
        let ast = ast::Program {
            items: vec![ast::Item::FnDef(ast::FnDef {
                name: ident("f"), params: vec![], ret_ty: Some(ast::Type::I32),
                body: ast::Block {
                    stmts: vec![ast::Stmt::Return(Some(ast::Expr::Call {
                        callee: Box::new(ast::Expr::Ident(ident("nonexistent"))),
                        args: vec![],
                        span: dummy_span(),
                    }), dummy_span())],
                    span: dummy_span(),
                },
                span: dummy_span(),
            })],
        };
        assert!(check_program(ast).is_err());
    }

    #[test]
    fn test_typeck_return_type_mismatch() {
        let ast = ast::Program {
            items: vec![ast::Item::FnDef(ast::FnDef {
                name: ident("f"), params: vec![], ret_ty: Some(ast::Type::I32),
                body: ast::Block {
                    stmts: vec![ast::Stmt::Return(Some(ast::Expr::Bool(true, dummy_span())), dummy_span())],
                    span: dummy_span(),
                },
                span: dummy_span(),
            })],
        };
        assert!(check_program(ast).is_err());
    }

    #[test]
    fn test_typeck_return_no_value_in_nonvoid() {
        let ast = ast::Program {
            items: vec![ast::Item::FnDef(ast::FnDef {
                name: ident("f"), params: vec![], ret_ty: Some(ast::Type::I32),
                body: ast::Block {
                    stmts: vec![ast::Stmt::Return(None, dummy_span())],
                    span: dummy_span(),
                },
                span: dummy_span(),
            })],
        };
        assert!(check_program(ast).is_err());
    }

    #[test]
    fn test_typeck_call_arg_count() {
        let callee = ast::Item::FnDef(ast::FnDef {
            name: ident("g"), params: vec![ast::Param { name: ident("a"), ty: Some(ast::Type::I32), span: dummy_span() }],
            ret_ty: Some(ast::Type::I32),
            body: ast::Block { stmts: vec![ast::Stmt::Return(Some(ast::Expr::Int(0, dummy_span())), dummy_span())], span: dummy_span() },
            span: dummy_span(),
        });
        let caller = ast::Item::FnDef(ast::FnDef {
            name: ident("f"), params: vec![], ret_ty: Some(ast::Type::I32),
            body: ast::Block {
                stmts: vec![ast::Stmt::Return(Some(ast::Expr::Call {
                    callee: Box::new(ast::Expr::Ident(ident("g"))),
                    args: vec![ast::Expr::Int(1, dummy_span()), ast::Expr::Int(2, dummy_span())],
                    span: dummy_span(),
                }), dummy_span())],
                span: dummy_span(),
            },
            span: dummy_span(),
        });
        assert!(check_program(ast::Program { items: vec![callee, caller] }).is_err());
    }

    #[test]
    fn test_typeck_call_arg_type() {
        let callee = ast::Item::FnDef(ast::FnDef {
            name: ident("g"), params: vec![ast::Param { name: ident("a"), ty: Some(ast::Type::I32), span: dummy_span() }],
            ret_ty: Some(ast::Type::I32),
            body: ast::Block { stmts: vec![ast::Stmt::Return(Some(ast::Expr::Int(0, dummy_span())), dummy_span())], span: dummy_span() },
            span: dummy_span(),
        });
        let caller = ast::Item::FnDef(ast::FnDef {
            name: ident("f"), params: vec![], ret_ty: Some(ast::Type::Bool),
            body: ast::Block {
                stmts: vec![ast::Stmt::Return(Some(ast::Expr::Call {
                    callee: Box::new(ast::Expr::Ident(ident("g"))),
                    args: vec![ast::Expr::Bool(true, dummy_span())],
                    span: dummy_span(),
                }), dummy_span())],
                span: dummy_span(),
            },
            span: dummy_span(),
        });
        assert!(check_program(ast::Program { items: vec![callee, caller] }).is_err());
    }

    #[test]
    fn test_typeck_assignment_type_mismatch() {
        let ast = ast::Program {
            items: vec![ast::Item::FnDef(ast::FnDef {
                name: ident("f"), params: vec![ast::Param { name: ident("x"), ty: Some(ast::Type::I32), span: dummy_span() }],
                ret_ty: Some(ast::Type::I32),
                body: ast::Block {
                    stmts: vec![
                        ast::Stmt::Expr(ast::Expr::Assign(
                            Box::new(ast::Expr::Ident(ident("x"))),
                            Box::new(ast::Expr::Bool(true, dummy_span())),
                            dummy_span(),
                        )),
                        ast::Stmt::Return(Some(ast::Expr::Ident(ident("x"))), dummy_span()),
                    ],
                    span: dummy_span(),
                },
                span: dummy_span(),
            })],
        };
        assert!(check_program(ast).is_err());
    }

    #[test]
    fn test_typeck_if_condition_not_bool() {
        let ast = ast::Program {
            items: vec![ast::Item::FnDef(ast::FnDef {
                name: ident("f"), params: vec![], ret_ty: None,
                body: ast::Block {
                    stmts: vec![ast::Stmt::If {
                        cond: Box::new(ast::Expr::Int(1, dummy_span())),
                        then: ast::Block { stmts: vec![], span: dummy_span() },
                        else_: None,
                        span: dummy_span(),
                    }],
                    span: dummy_span(),
                },
                span: dummy_span(),
            })],
        };
        assert!(check_program(ast).is_err());
    }

    #[test]
    fn test_typeck_while_condition_not_bool() {
        let ast = ast::Program {
            items: vec![ast::Item::FnDef(ast::FnDef {
                name: ident("f"), params: vec![], ret_ty: None,
                body: ast::Block {
                    stmts: vec![ast::Stmt::While {
                        cond: Box::new(ast::Expr::Int(0, dummy_span())),
                        body: ast::Block { stmts: vec![], span: dummy_span() },
                        span: dummy_span(),
                    }],
                    span: dummy_span(),
                },
                span: dummy_span(),
            })],
        };
        assert!(check_program(ast).is_err());
    }

    #[test]
    fn test_typeck_binop_type_mismatch() {
        let ast = ast::Program {
            items: vec![ast::Item::FnDef(ast::FnDef {
                name: ident("f"), params: vec![], ret_ty: Some(ast::Type::I32),
                body: ast::Block {
                    stmts: vec![ast::Stmt::Return(Some(ast::Expr::BinOp {
                        op: ast::BinOp::Add,
                        lhs: Box::new(ast::Expr::Int(1, dummy_span())),
                        rhs: Box::new(ast::Expr::Bool(true, dummy_span())),
                        span: dummy_span(),
                    }), dummy_span())],
                    span: dummy_span(),
                },
                span: dummy_span(),
            })],
        };
        assert!(check_program(ast).is_err());
    }

    #[test]
    fn test_typeck_match_empty_arms() {
        let ast = ast::Program {
            items: vec![ast::Item::FnDef(ast::FnDef {
                name: ident("f"), params: vec![], ret_ty: Some(ast::Type::I32),
                body: ast::Block {
                    stmts: vec![ast::Stmt::Return(Some(ast::Expr::Match {
                        expr: Box::new(ast::Expr::Int(1, dummy_span())),
                        arms: vec![],
                        span: dummy_span(),
                    }), dummy_span())],
                    span: dummy_span(),
                },
                span: dummy_span(),
            })],
        };
        assert!(check_program(ast).is_err());
    }

    #[test]
    fn test_typeck_match_arm_valid() {
        let ast = ast::Program {
            items: vec![ast::Item::FnDef(ast::FnDef {
                name: ident("f"), params: vec![], ret_ty: Some(ast::Type::I32),
                body: ast::Block {
                    stmts: vec![ast::Stmt::Return(Some(ast::Expr::Match {
                        expr: Box::new(ast::Expr::Int(1, dummy_span())),
                        arms: vec![
                            ast::MatchArm { pattern: ast::Pattern::Wildcard(dummy_span()), body: ast::Expr::Int(42, dummy_span()), span: dummy_span() },
                        ],
                        span: dummy_span(),
                    }), dummy_span())],
                    span: dummy_span(),
                },
                span: dummy_span(),
            })],
        };
        assert!(check_program(ast).is_ok(), "single-arm match should be ok");
    }

    #[test]
    fn test_typeck_for_iterable_not_i32() {
        let ast = ast::Program {
            items: vec![ast::Item::FnDef(ast::FnDef {
                name: ident("f"), params: vec![], ret_ty: None,
                body: ast::Block {
                    stmts: vec![ast::Stmt::For {
                        var: ident("i"),
                        iterable: Box::new(ast::Expr::Bool(true, dummy_span())),
                        body: ast::Block { stmts: vec![], span: dummy_span() },
                        span: dummy_span(),
                    }],
                    span: dummy_span(),
                },
                span: dummy_span(),
            })],
        };
        assert!(check_program(ast).is_err());
    }

    #[test]
    fn test_typeck_if_expr_type_mismatch() {
        let ast = ast::Program {
            items: vec![ast::Item::FnDef(ast::FnDef {
                name: ident("f"), params: vec![], ret_ty: Some(ast::Type::I32),
                body: ast::Block {
                    stmts: vec![ast::Stmt::Return(Some(ast::Expr::If {
                        cond: Box::new(ast::Expr::Bool(true, dummy_span())),
                        then: Box::new(ast::Expr::Int(1, dummy_span())),
                        else_: Box::new(ast::Expr::Bool(false, dummy_span())),
                        span: dummy_span(),
                    }), dummy_span())],
                    span: dummy_span(),
                },
                span: dummy_span(),
            })],
        };
        assert!(check_program(ast).is_err());
    }

    #[test]
    fn test_typeck_valid_complex() {
        let ast = ast::Program {
            items: vec![ast::Item::FnDef(ast::FnDef {
                name: ident("f"),
                params: vec![
                    ast::Param { name: ident("x"), ty: Some(ast::Type::I32), span: dummy_span() },
                    ast::Param { name: ident("y"), ty: Some(ast::Type::I32), span: dummy_span() },
                ],
                ret_ty: Some(ast::Type::I32),
                body: ast::Block {
                    stmts: vec![
                        ast::Stmt::Let(ast::Let { name: ident("z"), ty: Some(ast::Type::I32), value: Some(Box::new(ast::Expr::Int(0, dummy_span()))), span: dummy_span() }),
                        ast::Stmt::If {
                            cond: Box::new(ast::Expr::BinOp {
                                op: ast::BinOp::Gt,
                                lhs: Box::new(ast::Expr::Ident(ident("x"))),
                                rhs: Box::new(ast::Expr::Int(0, dummy_span())),
                                span: dummy_span(),
                            }),
                            then: ast::Block {
                                stmts: vec![ast::Stmt::Expr(ast::Expr::Assign(
                                    Box::new(ast::Expr::Ident(ident("z"))),
                                    Box::new(ast::Expr::Ident(ident("x"))),
                                    dummy_span(),
                                ))],
                                span: dummy_span(),
                            },
                            else_: Some(ast::Block {
                                stmts: vec![ast::Stmt::Expr(ast::Expr::Assign(
                                    Box::new(ast::Expr::Ident(ident("z"))),
                                    Box::new(ast::Expr::Ident(ident("y"))),
                                    dummy_span(),
                                ))],
                                span: dummy_span(),
                            }),
                            span: dummy_span(),
                        },
                        ast::Stmt::Return(Some(ast::Expr::Ident(ident("z"))), dummy_span()),
                    ],
                    span: dummy_span(),
                },
                span: dummy_span(),
            })],
        };
        assert!(check_program(ast).is_ok(), "valid complex program should type-check");
    }
}
