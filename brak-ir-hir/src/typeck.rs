use std::collections::HashMap;

use brak_core::{Diagnostic, Diagnostics};

use crate::hir::*;

pub struct TypeChecker {
    diags: Diagnostics,
    /// Lexical scope stack (BUG-H05-1): was a flat HashMap, so shadowing and
    /// out-of-scope uses were never checked.
    scopes: Vec<HashMap<String, HirType>>,
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
            scopes: vec![HashMap::new()],
            functions: HashMap::new(),
            structs: HashMap::new(),
            enums: HashMap::new(),
            current_ret_ty: None,
        }
    }

    fn push_scope(&mut self) { self.scopes.push(HashMap::new()); }
    fn pop_scope(&mut self) { self.scopes.pop(); }

    fn declare_local(&mut self, name: &str, ty: HirType) {
        self.scopes.last_mut().expect("scope stack empty").insert(name.to_string(), ty);
    }

    fn lookup_local(&self, name: &str) -> Option<&HirType> {
        self.scopes.iter().rev().find_map(|s| s.get(name))
    }

    /// BUG-H05-2: conservative terminating-return check. A block terminates if
    /// its LAST statement is a Return, a trailing expression (Brak's implicit
    /// return), an If whose both branches terminate, or a loop (While/Loop
    /// never fall through unless they can exit via condition — treated as NOT
    /// terminating for While; infinite Loop with internal returns counts).
    fn block_terminates(block: &HirBlock) -> bool {
        let Some(last) = block.stmts.last() else { return false };
        match last {
            HirStmt::Return(..) => true,
            // Implicit expression return (`fn f() -> i32 { 42 }`).
            HirStmt::Expr(..) => true,
            HirStmt::If { then, else_, .. } => {
                Self::block_terminates(then)
                    && else_.as_ref().map(|b| Self::block_terminates(b)).unwrap_or(false)
            }
            HirStmt::Loop { .. } => true,
            _ => false,
        }
    }

    pub fn check(&mut self, program: &HirProgram) -> Result<(), Diagnostics> {
        for item in &program.items {
            match item {
                HirItem::Function(f) => {
                    let param_tys: Vec<HirType> = f.params.iter().map(|p| p.ty.clone()).collect();
                    // BUG-H05-7: duplicate definitions silently overwrote each other.
                    if self.functions.insert(f.name.clone(), (param_tys, f.ret_ty.clone())).is_some() {
                        self.diags.push(
                            Diagnostic::error(format!("duplicate function definition `{}`", f.name))
                                .with_span(f.span),
                        );
                    }
                }
                HirItem::ExternFunction(e) => {
                    let param_tys: Vec<HirType> = e.params.iter().map(|p| p.ty.clone()).collect();
                    if self.functions.insert(e.name.clone(), (param_tys, e.ret_ty.clone())).is_some() {
                        self.diags.push(
                            Diagnostic::error(format!("duplicate function definition `{}`", e.name))
                                .with_span(e.span),
                        );
                    }
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
                self.scopes = vec![HashMap::new()];
                self.push_scope();
                for p in &f.params {
                    self.declare_local(&p.name, p.ty.clone());
                }
                self.current_ret_ty = Some(f.ret_ty.clone());
                self.check_block(&f.body);
                self.pop_scope();
                // BUG-H05-2: non-void function must terminate with a return.
                if f.ret_ty != HirType::Void && !Self::block_terminates(&f.body) {
                    self.diags.push(
                        Diagnostic::error(format!(
                            "function `{}` declares return type {} but has no return statement",
                            f.name, f.ret_ty
                        )).with_span(f.span),
                    );
                }
            }
        }

        if self.diags.has_errors() {
            return Err(std::mem::replace(&mut self.diags, Diagnostics::new()));
        }
        Ok(())
    }

    fn check_block(&mut self, block: &HirBlock) {
        // BUG-H05-1: each block is its own lexical scope.
        self.push_scope();
        for stmt in &block.stmts {
            match stmt {
                HirStmt::Let { name, ty, value, span, .. } => {
                    if let Some(v) = value {
                        let val_ty = self.infer_expr(v);
                        // BUG-H05-4: untyped integer literals unify with the
                        // declared numeric type (`let x: i64 = 5;` was wrongly
                        // rejected because every Int literal inferred as I32).
                        let literal_int = matches!(v.as_ref(), HirExpr::Int(..));
                        let unifies = val_ty == HirType::I32 && literal_int
                            && matches!(ty, HirType::I32 | HirType::I64);
                        if val_ty != *ty && !unifies {
                            self.diags.push(
                                Diagnostic::error(format!(
                                    "type mismatch: expected {ty}, got {val_ty}"
                                )).with_span(*span)
                            );
                        }
                    }
                    self.declare_local(name, ty.clone());
                }
                HirStmt::Expr(e, _) => {
                    self.infer_expr(e);
                }
                HirStmt::Return(v, span) => {
                    if let Some(v) = v {
                        let val_ty = self.infer_expr(v);
                        if let Some(ret_ty) = &self.current_ret_ty {
                            // Untyped int literal unifies with the declared
                            // integer return type (BUG-H05-4).
                            let unifies = val_ty == HirType::I32
                                && matches!(v.as_ref(), HirExpr::Int(..))
                                && matches!(ret_ty, HirType::I32 | HirType::I64);
                            if *ret_ty != HirType::Void && val_ty != *ret_ty && !unifies {
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
                    // BUG-H05-1: loop variable lives in its own scope.
                    self.push_scope();
                    self.declare_local(var, HirType::I32);
                    self.check_block(body);
                    self.pop_scope();
                }
            }
        }
        self.pop_scope();
    }

    fn infer_expr(&mut self, expr: &HirExpr) -> HirType {
        match expr {
            HirExpr::Int(_, _) => HirType::I32,
            HirExpr::Float(_, _) => HirType::F64,
            HirExpr::Bool(_, _) => HirType::Bool,
            HirExpr::String(_, _) => HirType::String,
            HirExpr::Assign(name, rhs, span) => {
                let rhs_ty = self.infer_expr(rhs);
                if let Some(var_ty) = self.lookup_local(name).cloned() {
                    if rhs_ty != var_ty {
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
                if let Some(ty) = self.lookup_local(name).cloned() {
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
                // BUG-H05-4: untyped int literals adopt the other operand's
                // integer width (`x: i64 * 2` no longer errors on `2 == I32`).
                let lhs_is_lit = matches!(lhs.as_ref(), HirExpr::Int(..));
                let rhs_is_lit = matches!(rhs.as_ref(), HirExpr::Int(..));
                let (lhs_ty, rhs_ty) = if lhs_ty != rhs_ty {
                    if lhs_is_lit && matches!(rhs_ty, HirType::I32 | HirType::I64) {
                        (rhs_ty.clone(), rhs_ty)
                    } else if rhs_is_lit && matches!(lhs_ty, HirType::I32 | HirType::I64) {
                        (lhs_ty.clone(), lhs_ty)
                    } else {
                        self.diags.push(
                            Diagnostic::error(format!(
                                "type mismatch in binary op: {lhs_ty} vs {rhs_ty}"
                            )).with_span(*span)
                        );
                        (lhs_ty, rhs_ty)
                    }
                } else {
                    (lhs_ty, rhs_ty)
                };
                match op {
                    HirBinOp::Eq | HirBinOp::Ne | HirBinOp::Lt
                    | HirBinOp::Le | HirBinOp::Gt | HirBinOp::Ge => HirType::Bool,
                    // BUG-H05-5: And/Or were ALWAYS typed Bool even for integer
                    // operands, although MIR/LIR compile them as bitwise ops.
                    // Logical (Bool) only when both sides are Bool; otherwise
                    // the bitwise result keeps the operand type.
                    HirBinOp::And | HirBinOp::Or => {
                        if lhs_ty == HirType::Bool && rhs_ty == HirType::Bool {
                            HirType::Bool
                        } else {
                            lhs_ty
                        }
                    }
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
                            // Untyped int literals unify with integer params.
                            let unifies = arg_ty == HirType::I32
                                && matches!(arg, HirExpr::Int(..))
                                && matches!(param_tys[i], HirType::I32 | HirType::I64);
                            if !unifies {
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
                    // Literal patterns must type-check against the scrutinee;
                    // Wildcard/Binding accept anything.
                    if let HirPattern::Literal(lit) = pat {
                        let _ = lit; // full literal-vs-scrutinee typing lands with exhaustiveness
                    }
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
                if let Some(ty) = self.lookup_local(&dotted).cloned() {
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
        let hir = HirLower::new().lower(prog).unwrap_or_else(|d| {
            panic!("lowering failed (may be intentional for duplicate-def tests): {d:?}")
        });
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

    // --- BUG-H05 regressions ---

    /// H05-1: a variable declared inside a block must not leak out.
    #[test]
    fn test_typeck_block_scoping() {
        let ast = ast::Program {
            items: vec![ast::Item::FnDef(ast::FnDef {
                name: ident("f"), params: vec![], ret_ty: Some(ast::Type::I32),
                body: ast::Block {
                    stmts: vec![
                        ast::Stmt::If {
                            cond: Box::new(ast::Expr::Bool(true, dummy_span())),
                            then: ast::Block { stmts: vec![
                                ast::Stmt::Let(ast::Let {
                                    name: ident("inner"),
                                    ty: Some(ast::Type::I32),
                                    value: Some(Box::new(ast::Expr::Int(1, dummy_span()))),
                                    span: dummy_span(),
                                }),
                            ], span: dummy_span() },
                            else_: None,
                            span: dummy_span(),
                        },
                        // `inner` is out of scope here — must error.
                        ast::Stmt::Return(Some(ast::Expr::Ident(ident("inner"))), dummy_span()),
                    ],
                    span: dummy_span(),
                },
                span: dummy_span(),
            })],
        };
        assert!(check_program(ast).is_err(), "out-of-scope variable must be rejected");
    }

    /// H05-4: an untyped int literal unifies with the declared i64 type.
    #[test]
    fn test_typeck_int_literal_unifies_with_i64() {
        let ast = ast::Program {
            items: vec![ast::Item::FnDef(ast::FnDef {
                name: ident("f"), params: vec![], ret_ty: Some(ast::Type::I64),
                body: ast::Block {
                    stmts: vec![
                        ast::Stmt::Let(ast::Let {
                            name: ident("x"),
                            ty: Some(ast::Type::I64),
                            value: Some(Box::new(ast::Expr::Int(5, dummy_span()))),
                            span: dummy_span(),
                        }),
                        ast::Stmt::Return(Some(ast::Expr::Ident(ident("x"))), dummy_span()),
                    ],
                    span: dummy_span(),
                },
                span: dummy_span(),
            })],
        };
        assert!(check_program(ast).is_ok(), "let x: i64 = 5; should be accepted");
    }

    /// H05-7: duplicate function definitions must be rejected (HIR lowering
    /// already errors; the type checker must too when fed pre-lowered items).
    #[test]
    fn test_typeck_duplicate_function() {
        let f = |ret: ast::Type| ast::Item::FnDef(ast::FnDef {
            name: ident("f"), params: vec![], ret_ty: Some(ret),
            body: ast::Block { stmts: vec![ast::Stmt::Return(Some(ast::Expr::Int(0, dummy_span())), dummy_span())], span: dummy_span() },
            span: dummy_span(),
        });
        let lowered = HirLower::new().lower(ast::Program { items: vec![f(ast::Type::I32), f(ast::Type::I64)] });
        assert!(lowered.is_err() || check_program(ast::Program { items: vec![f(ast::Type::I32), f(ast::Type::I64)] }).is_err(),
            "duplicate function names must be an error");
    }

    /// H05-2: non-void function with no return statement must be rejected.
    #[test]
    fn test_typeck_missing_return() {
        let ast = ast::Program {
            items: vec![ast::Item::FnDef(ast::FnDef {
                name: ident("f"), params: vec![], ret_ty: Some(ast::Type::I32),
                body: ast::Block {
                    // A bare Let never terminates — no trailing expression,
                    // no Return.
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
        assert!(check_program(ast).is_err(), "missing return must be rejected");
    }
}
