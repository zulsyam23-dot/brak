use std::collections::HashMap;

use brak_core::{Diagnostics, Span};
use brak_ir_hir::hir::*;

use crate::mir::*;

struct LoopContext {
    continue_target: usize,
    break_target: usize,
}

pub struct MirLower {
    diagnostics: Diagnostics,
    next_local: usize,
    locals: Vec<MirLocal>,
    local_map: HashMap<String, LocalId>,
    loop_stack: Vec<LoopContext>,
}

impl Default for MirLower {
    fn default() -> Self {
        Self::new()
    }
}

impl MirLower {
    pub fn new() -> Self {
        Self {
            diagnostics: Diagnostics::new(),
            next_local: 0,
            locals: vec![],
            local_map: HashMap::new(),
            loop_stack: vec![],
        }
    }

    fn fresh_local(&mut self) -> LocalId {
        let id = self.next_local;
        self.next_local += 1;
        id
    }

    fn get_or_create_local(&mut self, name: &str, ty: MirType) -> LocalId {
        if let Some(&id) = self.local_map.get(name) {
            return id;
        }
        let id = self.fresh_local();
        self.locals.push(MirLocal {
            name: name.to_string(),
            ty,
        });
        self.local_map.insert(name.to_string(), id);
        id
    }

    fn reset_function_state(&mut self) {
        self.next_local = 0;
        self.locals = vec![];
        self.local_map = HashMap::new();
        self.loop_stack = vec![];
    }

    pub fn lower(&mut self, program: HirProgram) -> Result<MirProgram, Diagnostics> {
        let mut functions = vec![];
        let mut extern_functions = vec![];
        let mut structs = vec![];
        let mut enums = vec![];
        for item in program.items {
            match item {
                HirItem::Function(f) => {
                    if let Ok(mf) = self.lower_function(f) {
                        functions.push(mf);
                    }
                }
                HirItem::ExternFunction(e) => {
                    extern_functions.push(MirExternFunction {
                        name: e.name,
                        params: e.params.into_iter().map(|p| lower_hir_type(&p.ty)).collect(),
                        ret_ty: lower_hir_type(&e.ret_ty),
                        abi: e.abi,
                        span: e.span,
                    });
                }
                HirItem::Struct(s) => {
                    structs.push(MirStruct {
                        name: s.name,
                        fields: s.fields.into_iter().map(|f| MirField {
                            name: f.name,
                            ty: lower_hir_type(&f.ty),
                            span: f.span,
                        }).collect(),
                        span: s.span,
                    });
                }
                HirItem::Enum(e) => {
                    enums.push(MirEnum {
                        name: e.name,
                        variants: e.variants.into_iter().map(|v| MirVariant {
                            name: v.name,
                            fields: v.fields.map(|fs| fs.iter().map(lower_hir_type).collect()),
                            span: v.span,
                        }).collect(),
                        span: e.span,
                    });
                }
                HirItem::GlobalLet(_) => {}
            }
        }
        if self.diagnostics.has_errors() {
            Err(std::mem::take(&mut self.diagnostics))
        } else {
            Ok(MirProgram { functions, extern_functions, structs, enums })
        }
    }

    fn lower_function(&mut self, func: HirFunction) -> Result<MirFunction, ()> {
        self.reset_function_state();
        for p in &func.params {
            self.get_or_create_local(&p.name, lower_hir_type(&p.ty));
        }
        let blocks = self.lower_block_to_cfg(&func.body)?;
        let locals = std::mem::take(&mut self.locals);
        Ok(MirFunction {
            name: func.name,
            params: (0..func.params.len()).collect(),
            ret_ty: lower_hir_type(&func.ret_ty),
            blocks,
            locals,
            span: func.span,
        })
    }

    fn lower_block_to_cfg(&mut self, block: &HirBlock) -> Result<Vec<MirBlock>, ()> {
        let mut blocks = vec![];
        let mut current_insts = vec![];
        let mut current_name = "entry".to_string();
        let mut last_expr_result: Option<LocalId> = None;
        let block_span = block.span;

        for stmt in &block.stmts {
            match stmt {
                HirStmt::Let { name, ty, value, span, .. } => {
                    last_expr_result = None;
                    let local_id = self.get_or_create_local(name, lower_hir_type(ty));
                    if let Some(v) = value {
                        let val_id = self.emit_expr(v, &mut current_insts, &mut current_name, &mut blocks)?;
                        current_insts.push(MirInst::Assign {
                            dest: local_id,
                            value: MirValue::Local(val_id),
                            span: *span,
                        });
                    }
                }
                HirStmt::Expr(e, _) => {
                    last_expr_result = Some(self.emit_expr(e, &mut current_insts, &mut current_name, &mut blocks)?);
                }
                HirStmt::Return(v, span) => {
                    let value = match v {
                        Some(v) => Some(self.emit_expr(v, &mut current_insts, &mut current_name, &mut blocks)?),
                        None => None,
                    };
                    blocks.push(MirBlock {
                        id: blocks.len(),
                        name: "unreachable".to_string(),
                        insts: current_insts,
                        terminator: MirTerminator::Return {
                            value,
                            span: *span,
                        },
                        span: block_span,
                    });
                    current_insts = vec![];
                    current_name = "unreachable".to_string();
                }
                HirStmt::If { cond, then, else_, span } => {
                    let cond_id = self.emit_expr(cond, &mut current_insts, &mut current_name, &mut blocks)?;

                    let mut then_blocks = self.lower_block_to_cfg(then)?;
                    let mut else_blocks = match else_ {
                        Some(b) => self.lower_block_to_cfg(b)?,
                        None => vec![],
                    };
                    
                    let then_start = blocks.len() + 1;
                    let else_start = then_start + then_blocks.len();
                    let after = else_start + else_blocks.len();

                    remap_block_ids(&mut then_blocks, then_start);
                    remap_block_ids(&mut else_blocks, else_start);

                    // Current block ends with branch
                    blocks.push(MirBlock {
                        id: blocks.len(),
                        name: current_name.clone(),
                        insts: current_insts,
                        terminator: MirTerminator::Branch {
                            cond: cond_id,
                            then: then_start,
                            else_: else_start,
                            span: *span,
                        },
                        span: block_span,
                    });

                    // Push then blocks
                    for mut b in then_blocks {
                        if b.name == "unreachable" {
                            // real return statement — keep terminator
                        } else {
                            b.terminator = MirTerminator::Jump { target: after, span: *span };
                        }
                        b.id = blocks.len();
                        blocks.push(b);
                    }

                    // Push else blocks
                    for mut b in else_blocks {
                        if b.name == "unreachable" {
                            // real return statement — keep terminator
                        } else {
                            b.terminator = MirTerminator::Jump { target: after, span: *span };
                        }
                        b.id = blocks.len();
                        blocks.push(b);
                    }

                    // Start the 'after' block
                    current_insts = vec![];
                    current_name = "if_merge".to_string();
                }
                HirStmt::While { cond, body, span } => {
                    let cond_header = blocks.len();
                    
                    // Current block jumps to condition
                    blocks.push(MirBlock {
                        id: blocks.len(),
                        name: current_name.clone(),
                        insts: current_insts,
                        terminator: MirTerminator::Jump {
                            target: cond_header + 1,
                            span: *span,
                        },
                        span: block_span,
                    });

                    let mut body_blocks = self.lower_block_to_cfg(body)?;
                    let body_start = cond_header + 2;
                    let after_while = body_start + body_blocks.len();

                    remap_block_ids(&mut body_blocks, body_start);

                    self.loop_stack.push(LoopContext {
                        continue_target: cond_header + 1,
                        break_target: after_while,
                    });

                    // Condition block
                    let mut cond_insts = vec![];
                    let mut dummy_name = "cond".to_string();
                    let cond_id = self.emit_expr(cond, &mut cond_insts, &mut dummy_name, &mut blocks)?;
                    
                    blocks.push(MirBlock {
                        id: cond_header + 1,
                        name: "while_cond".to_string(),
                        insts: cond_insts,
                        terminator: MirTerminator::Branch {
                            cond: cond_id,
                            then: body_start,
                            else_: after_while,
                            span: *span,
                        },
                        span: block_span,
                    });

                    // Body blocks
                    for mut b in body_blocks {
                        // Only the synthetic fall-through Return is redirected back to
                        // the condition. A block named "unreachable" holds a REAL
                        // `return` statement and must keep its Return terminator
                        // (BUG-K01: previously all Returns became `continue`).
                        if b.name != "unreachable" {
                            if let MirTerminator::Return { .. } = &b.terminator {
                                b.terminator = MirTerminator::Jump { target: cond_header + 1, span: *span };
                            }
                        }
                        // blocks with Jump, Branch, etc. keep their terminator (internal control flow)
                        b.id = blocks.len();
                        blocks.push(b);
                    }

                    self.loop_stack.pop();

                    // Start the 'after' block
                    current_insts = vec![];
                    current_name = "while_after".to_string();
                }
                HirStmt::For { var, iterable, body, span } => {
                    let for_start = blocks.len();

                    // Create local for loop variable (also serves as counter)
                    let var_local = self.get_or_create_local(var, MirType::I32);
                    // Create bound local
                    let bound_local = self.fresh_local();
                    self.locals.push(MirLocal {
                        name: format!("for_bound_{var}"),
                        ty: MirType::I32,
                    });

                    // Entry -> Jump to init
                    blocks.push(MirBlock {
                        id: blocks.len(),
                        name: current_name.clone(),
                        insts: current_insts,
                        terminator: MirTerminator::Jump {
                            target: for_start + 1,
                            span: *span,
                        },
                        span: block_span,
                    });

                    // Init block: evaluate iterable -> bound, set var = 0
                    let mut init_insts = vec![];
                    let mut dummy_name = "for_init".to_string();
                    let iter_id = self.emit_expr(iterable, &mut init_insts, &mut dummy_name, &mut blocks)?;
                    init_insts.push(MirInst::Assign {
                        dest: bound_local,
                        value: MirValue::Local(iter_id),
                        span: *span,
                    });
                    init_insts.push(MirInst::Assign {
                        dest: var_local,
                        value: MirValue::Int(0),
                        span: *span,
                    });
                    blocks.push(MirBlock {
                        id: blocks.len(),
                        name: "for_init".to_string(),
                        insts: init_insts,
                        terminator: MirTerminator::Jump {
                            target: for_start + 2,
                            span: *span,
                        },
                        span: block_span,
                    });

                    // Lower body with var in local_map
                    let mut body_blocks = self.lower_block_to_cfg(body)?;
                    let body_start = for_start + 3;
                    // Latch block holds the increment; both normal iteration end and
                    // `continue` route through it so the counter always advances.
                    let latch_id = body_start + body_blocks.len();
                    let after_for = latch_id + 1;

                    remap_block_ids(&mut body_blocks, body_start);

                    self.loop_stack.push(LoopContext {
                        continue_target: latch_id,
                        break_target: after_for,
                    });

                    // Cond block: compare var < bound
                    let mut cond_insts = vec![];
                    let cond_local = self.fresh_local();
                    self.locals.push(MirLocal {
                        name: format!("for_cond_{var}"),
                        ty: MirType::Bool,
                    });
                    cond_insts.push(MirInst::Assign {
                        dest: cond_local,
                        value: MirValue::BinOp {
                            op: MirBinOp::Lt,
                            lhs: var_local,
                            rhs: bound_local,
                        },
                        span: *span,
                    });
                    blocks.push(MirBlock {
                        id: for_start + 2,
                        name: "for_cond".to_string(),
                        insts: cond_insts,
                        terminator: MirTerminator::Branch {
                            cond: cond_local,
                            then: body_start,
                            else_: after_for,
                            span: *span,
                        },
                        span: block_span,
                    });

                    // Body blocks — synthetic fall-through jumps to the latch; a REAL
                    // `return` (block named "unreachable") keeps its Return terminator
                    // (BUG-K01).
                    for mut b in body_blocks {
                        if b.name != "unreachable" {
                            if let MirTerminator::Return { .. } = &b.terminator {
                                b.terminator = MirTerminator::Jump {
                                    target: latch_id,
                                    span: *span,
                                };
                            }
                        }
                        b.id = blocks.len();
                        blocks.push(b);
                    }

                    // Latch: var = var + 1, then back to cond
                    let one_local = self.fresh_local();
                    self.locals.push(MirLocal {
                        name: format!("for_inc_{var}"),
                        ty: MirType::I32,
                    });
                    let mut latch_insts = vec![MirInst::Assign {
                        dest: one_local,
                        value: MirValue::Int(1),
                        span: *span,
                    }];
                    latch_insts.push(MirInst::Assign {
                        dest: var_local,
                        value: MirValue::BinOp {
                            op: MirBinOp::Add,
                            lhs: var_local,
                            rhs: one_local,
                        },
                        span: *span,
                    });
                    blocks.push(MirBlock {
                        id: latch_id,
                        name: format!("for_latch_{var}"),
                        insts: latch_insts,
                        terminator: MirTerminator::Jump {
                            target: for_start + 2,
                            span: *span,
                        },
                        span: block_span,
                    });

                    self.loop_stack.pop();

                    // Start the 'after' block
                    current_insts = vec![];
                    current_name = "for_after".to_string();
                }
                HirStmt::Loop { body, span } => {
                    let loop_start = blocks.len();

                    blocks.push(MirBlock {
                        id: blocks.len(),
                        name: current_name.clone(),
                        insts: current_insts,
                        terminator: MirTerminator::Jump {
                            target: loop_start + 1,
                            span: *span,
                        },
                        span: block_span,
                    });

                    let mut body_blocks = self.lower_block_to_cfg(body)?;
                    let body_start = loop_start + 1;
                    let after_loop = body_start + body_blocks.len();

                    remap_block_ids(&mut body_blocks, body_start);

                    self.loop_stack.push(LoopContext {
                        continue_target: body_start,
                        break_target: after_loop,
                    });

                    for mut b in body_blocks {
                        // Real `return` (named "unreachable") keeps Return (BUG-K01);
                        // only the synthetic fall-through loops back.
                        if b.name != "unreachable" {
                            if let MirTerminator::Return { .. } = &b.terminator {
                                b.terminator = MirTerminator::Jump {
                                    target: body_start,
                                    span: *span,
                                };
                            }
                        }
                        b.id = blocks.len();
                        blocks.push(b);
                    }

                    self.loop_stack.pop();

                    current_insts = vec![];
                    current_name = "loop_after".to_string();
                }
                HirStmt::Break(span) => {
                    if let Some(ctx) = self.loop_stack.last() {
                        let target = ctx.break_target;
                        blocks.push(MirBlock {
                            id: blocks.len(),
                            name: current_name.clone(),
                            insts: current_insts,
                            terminator: MirTerminator::Jump {
                                target,
                                span: *span,
                            },
                            span: block_span,
                        });
                        current_insts = vec![];
                        current_name = "unreachable".to_string();
                    } else {
                        self.diagnostics.push(
                            brak_core::Diagnostic::error("`break` outside of a loop")
                                .with_span(*span),
                        );
                    }
                }
                HirStmt::Continue(span) => {
                    if let Some(ctx) = self.loop_stack.last() {
                        let target = ctx.continue_target;
                        blocks.push(MirBlock {
                            id: blocks.len(),
                            name: current_name.clone(),
                            insts: current_insts,
                            terminator: MirTerminator::Jump {
                                target,
                                span: *span,
                            },
                            span: block_span,
                        });
                        current_insts = vec![];
                        current_name = "unreachable".to_string();
                    } else {
                        self.diagnostics.push(
                            brak_core::Diagnostic::error("`continue` outside of a loop")
                                .with_span(*span),
                        );
                    }
                }
            }
        }

        // Final block if there's anything left or no blocks were pushed
        if !current_insts.is_empty() || blocks.is_empty() || current_name != "unreachable" {
            blocks.push(MirBlock {
                id: blocks.len(),
                name: current_name,
                insts: current_insts,
                terminator: MirTerminator::Return {
                    value: last_expr_result,
                    span: block_span,
                },
                span: block_span,
            });
        }

        Ok(blocks)
    }

    fn emit_expr(
        &mut self,
        expr: &HirExpr,
        insts: &mut Vec<MirInst>,
        current_name: &mut String,
        blocks: &mut Vec<MirBlock>,
    ) -> Result<LocalId, ()> {
        let span = expr.span();
        match expr {
            HirExpr::Ident(name, _) => {
                if let Some(&id) = self.local_map.get(name) {
                    return Ok(id);
                }
                let id = self.fresh_local();
                self.locals.push(MirLocal {
                    name: format!("tmp_{id}"),
                    ty: MirType::I32,
                });
                Ok(id)
            }
            HirExpr::Assign(name, rhs, span) => {
                let rhs_id = self.emit_expr(rhs, insts, current_name, blocks)?;
                let dest_id = self.get_or_create_local(name, MirType::I32);
                insts.push(MirInst::Assign {
                    dest: dest_id,
                    value: MirValue::Local(rhs_id),
                    span: *span,
                });
                Ok(dest_id)
            }
            HirExpr::Int(i, _) => {
                let id = self.fresh_local();
                self.locals.push(MirLocal {
                    name: format!("tmp_{id}"),
                    ty: MirType::I32,
                });
                insts.push(MirInst::Assign {
                    dest: id,
                    value: MirValue::Int(*i),
                    span,
                });
                Ok(id)
            }
            HirExpr::Float(f, _) => {
                let id = self.fresh_local();
                self.locals.push(MirLocal {
                    name: format!("tmp_{id}"),
                    ty: MirType::F64,
                });
                insts.push(MirInst::Assign {
                    dest: id,
                    value: MirValue::Float(*f),
                    span,
                });
                Ok(id)
            }
            HirExpr::Bool(b, _) => {
                let id = self.fresh_local();
                self.locals.push(MirLocal {
                    name: format!("tmp_{id}"),
                    ty: MirType::Bool,
                });
                insts.push(MirInst::Assign {
                    dest: id,
                    value: MirValue::Bool(*b),
                    span,
                });
                Ok(id)
            }
            HirExpr::BinOp { op, lhs, rhs, span } => {
                let lhs_id = self.emit_expr(lhs, insts, current_name, blocks)?;
                let rhs_id = self.emit_expr(rhs, insts, current_name, blocks)?;
                let mut mir_op = match lower_mir_binop(*op) {
                    Some(op) => op,
                    None => {
                        self.diagnostics.push(
                            brak_core::Diagnostic::error(
                                "range expressions are not yet supported in this position",
                            )
                            .with_span(*span),
                        );
                        return Err(());
                    }
                };
                // BUG-M17: pick the FLOAT variant when either operand is a
                // float-typed local, so backends emit SSE/f64 arithmetic
                // instead of silently treating bits as integers.
                let is_float = |id: LocalId| matches!(
                    self.locals.get(id).map(|l| &l.ty),
                    Some(MirType::F32 | MirType::F64)
                );
                let float_arith = matches!(mir_op, MirBinOp::Add | MirBinOp::Sub | MirBinOp::Mul | MirBinOp::Div)
                    && (is_float(lhs_id) || is_float(rhs_id));
                if float_arith {
                    mir_op = match mir_op {
                        MirBinOp::Add => MirBinOp::FAdd,
                        MirBinOp::Sub => MirBinOp::FSub,
                        MirBinOp::Mul => MirBinOp::FMul,
                        MirBinOp::Div => MirBinOp::FDiv,
                        other => other,
                    };
                }
                let id = self.fresh_local();
                self.locals.push(MirLocal {
                    name: format!("tmp_{id}"),
                    ty: if float_arith { MirType::F64 } else { MirType::I32 },
                });
                insts.push(MirInst::Assign {
                    dest: id,
                    value: MirValue::BinOp {
                        op: mir_op,
                        lhs: lhs_id,
                        rhs: rhs_id,
                    },
                    span: *span,
                });
                Ok(id)
            }
            HirExpr::UnOp { op, expr, span } => {
                let inner = self.emit_expr(expr, insts, current_name, blocks)?;
                let id = self.fresh_local();
                self.locals.push(MirLocal {
                    name: format!("tmp_{id}"),
                    ty: MirType::I32,
                });
                insts.push(MirInst::Assign {
                    dest: id,
                    value: MirValue::UnOp {
                        op: lower_mir_unop(*op),
                        expr: inner,
                    },
                    span: *span,
                });
                Ok(id)
            }
            HirExpr::Call { callee, args, span } => {
                let callee_name = match callee.as_ref() {
                    HirExpr::Ident(s, _) => s.clone(),
                    _ => "unknown".to_string(),
                };
                let mut arg_ids = vec![];
                for a in args {
                    arg_ids.push(self.emit_expr(a, insts, current_name, blocks)?);
                }
                let dest = self.fresh_local();
                self.locals.push(MirLocal {
                    name: format!("tmp_{dest}"),
                    ty: MirType::I32,
                });
                insts.push(MirInst::Call {
                    dest: Some(dest),
                    callee: callee_name,
                    args: arg_ids,
                    span: *span,
                });
                Ok(dest)
            }
            HirExpr::If { cond, then, else_, .. } => {
                self.lower_if_expr(cond, then, else_, insts, current_name, blocks)
            }
            HirExpr::String(s, _) => {
                let id = self.fresh_local();
                self.locals.push(MirLocal {
                    name: format!("tmp_{id}"),
                    ty: MirType::String,
                });
                insts.push(MirInst::Assign {
                    dest: id,
                    value: MirValue::String(s.clone()),
                    span,
                });
                Ok(id)
            }
            HirExpr::Block(b) => {
                if let Some(HirStmt::Expr(e, _)) = b.stmts.last() {
                    self.emit_expr(e, insts, current_name, blocks)
                } else {
                    let id = self.fresh_local();
                    self.locals.push(MirLocal {
                        name: format!("tmp_{id}"),
                        ty: MirType::I32,
                    });
                    insts.push(MirInst::Assign {
                        dest: id,
                        value: MirValue::Int(0),
                        span: Span::new(Default::default(), Default::default()),
                    });
                    Ok(id)
                }
            }
            HirExpr::Match { expr: scrutinee, arms, span } => {
                self.lower_match_expr(scrutinee, arms, *span, insts, current_name, blocks)
            }
            HirExpr::Field { object, field, span } => {
                let obj_id = self.emit_expr(object, insts, current_name, blocks)?;
                let id = self.fresh_local();
                self.locals.push(MirLocal {
                    name: format!("tmp_{id}"),
                    ty: MirType::I32, // Simplified for now
                });
                insts.push(MirInst::Assign {
                    dest: id,
                    value: MirValue::GetField {
                        object: obj_id,
                        name: "unknown".to_string(), // TODO: Get struct name
                        field: field.clone(),
                    },
                    span: *span,
                });
                Ok(id)
            }
            HirExpr::StructInit { name, fields, span } => {
                let mut mir_fields = vec![];
                for (fname, fexpr) in fields {
                    let fid = self.emit_expr(fexpr, insts, current_name, blocks)?;
                    mir_fields.push((fname.clone(), fid));
                }
                let id = self.fresh_local();
                self.locals.push(MirLocal {
                    name: format!("tmp_{id}"),
                    ty: MirType::Named(name.clone()),
                });
                insts.push(MirInst::Assign {
                    dest: id,
                    value: MirValue::StructInit {
                        name: name.clone(),
                        fields: mir_fields,
                    },
                    span: *span,
                });
                Ok(id)
            }
            HirExpr::FieldAssign { object, field, value, span } => {
                let obj_id = self.emit_expr(object, insts, current_name, blocks)?;
                let val_id = self.emit_expr(value, insts, current_name, blocks)?;
                let id = self.fresh_local();
                self.locals.push(MirLocal {
                    name: format!("tmp_{id}"),
                    ty: MirType::I32, // Simplified
                });
                insts.push(MirInst::Assign {
                    dest: id,
                    value: MirValue::SetField {
                        object: obj_id,
                        field: field.clone(),
                        value: val_id,
                    },
                    span: *span,
                });
                Ok(id)
            }
        }
    }

    /// Lower a match expression into a chain of comparison blocks (BUG-K03).
    ///
    /// Layout: dispatch → [check_i → body_i]* → merge. The first Wildcard/Binding
    /// arm catches everything after it; the last effective arm is an unconditional
    /// fallback (no check) — standard "final arm catches all" convention.
    fn lower_match_expr(
        &mut self,
        scrutinee: &HirExpr,
        arms: &[(HirPattern, HirExpr)],
        span: Span,
        insts: &mut Vec<MirInst>,
        current_name: &mut String,
        blocks: &mut Vec<MirBlock>,
    ) -> Result<LocalId, ()> {
        let dummy = || Span::new(Default::default(), Default::default());

        let result_local = self.fresh_local();
        self.locals.push(MirLocal {
            name: format!("tmp_{result_local}"),
            ty: MirType::I32,
        });

        if arms.is_empty() {
            // typeck already rejects this; emit 0 so lowering stays total.
            insts.push(MirInst::Assign {
                dest: result_local,
                value: MirValue::Int(0),
                span,
            });
            return Ok(result_local);
        }

        let scrut_id = self.emit_expr(scrutinee, insts, current_name, blocks)?;

        // Effective arms: everything up to & including first non-literal pattern.
        let mut eff_len = arms.len();
        for (i, (pat, _)) in arms.iter().enumerate() {
            if !matches!(pat, HirPattern::Literal(_)) {
                eff_len = i + 1;
                break;
            }
        }

        // Lower all effective arm bodies up-front into separate block vectors
        // (each numbered from 0) so layout offsets can be computed exactly.
        let mut body_block_sets: Vec<Vec<MirBlock>> = Vec::new();
        for (_, body) in arms.iter().take(eff_len) {
            let synth = HirBlock {
                stmts: vec![HirStmt::Expr(Box::new(body.clone()), body.span())],
                span: dummy(),
            };
            body_block_sets.push(self.lower_block_to_cfg(&synth)?);
        }

        // Compute positions: dispatch, then per-arm [check?][body], then merge.
        let dispatch_id = blocks.len();
        let mut entry = vec![0usize; eff_len];
        let mut body_pos = vec![0usize; eff_len];
        let mut cursor = dispatch_id + 1;
        for i in 0..eff_len {
            entry[i] = cursor;
            let is_last = i == eff_len - 1;
            let needs_check = !is_last && matches!(arms[i].0, HirPattern::Literal(_));
            if needs_check {
                cursor += 1;
            }
            body_pos[i] = cursor;
            cursor += body_block_sets[i].len();
        }
        let merge = cursor;

        // Dispatch block consumes whatever the caller had pending.
        blocks.push(MirBlock {
            id: dispatch_id,
            name: current_name.clone(),
            insts: std::mem::take(insts),
            terminator: MirTerminator::Jump { target: entry[0], span },
            span: dummy(),
        });

        for i in 0..eff_len {
            let is_last = i == eff_len - 1;

            // Check block: compare scrutinee against a literal pattern.
            if !is_last {
                if let HirPattern::Literal(lit) = &arms[i].0 {
                    let value = match lit {
                        HirLiteral::Int(v) => MirValue::Int(*v),
                        HirLiteral::Float(v) => MirValue::Float(*v),
                        HirLiteral::Bool(v) => MirValue::Bool(*v),
                        HirLiteral::Str(v) => MirValue::String(v.clone()),
                    };
                    // BinOp operands are locals: materialize the literal first.
                    let pat_local = self.fresh_local();
                    self.locals.push(MirLocal {
                        name: format!("match_pat_{i}"),
                        ty: MirType::I32,
                    });
                    let cond_local = self.fresh_local();
                    self.locals.push(MirLocal {
                        name: format!("match_cond_{i}"),
                        ty: MirType::Bool,
                    });
                    blocks.push(MirBlock {
                        id: entry[i],
                        name: format!("match_check_{i}"),
                        insts: vec![
                            MirInst::Assign {
                                dest: pat_local,
                                value,
                                span,
                            },
                            MirInst::Assign {
                                dest: cond_local,
                                value: MirValue::BinOp {
                                    op: MirBinOp::Eq,
                                    lhs: scrut_id,
                                    rhs: pat_local,
                                },
                                span,
                            },
                        ],
                        terminator: MirTerminator::Branch {
                            cond: cond_local,
                            then: body_pos[i],
                            else_: entry[i + 1],
                            span: span,
                        },
                        span: dummy(),
                    });
                }
            }

            // Binding pattern: assign scrutinee to the bound name before the body.
            let bind_insts: Vec<MirInst> = match &arms[i].0 {
                HirPattern::Binding(name) => {
                    let l = self.get_or_create_local(name, MirType::I32);
                    vec![MirInst::Assign {
                        dest: l,
                        value: MirValue::Local(scrut_id),
                        span,
                    }]
                }
                _ => vec![],
            };

            let mut set = std::mem::take(&mut body_block_sets[i]);
            remap_block_ids(&mut set, body_pos[i]);
            if !bind_insts.is_empty() {
                if let Some(first) = set.first_mut() {
                    let mut merged = bind_insts;
                    merged.append(&mut first.insts);
                    first.insts = merged;
                }
            }
            // Rewrite the fall-through end of the arm to produce the result and
            // jump to merge; a REAL `return` (named "unreachable") stays intact.
            if let Some(last) = set.last_mut() {
                if last.name != "unreachable" {
                    if let MirTerminator::Return { value: Some(val), .. } = &last.terminator {
                        last.insts.push(MirInst::Assign {
                            dest: result_local,
                            value: MirValue::Local(*val),
                            span,
                        });
                    }
                    last.terminator = MirTerminator::Jump { target: merge, span };
                }
            }
            for mut b in set {
                // remap_block_ids fixed terminator targets; ids must be assigned
                // at push time to match the layout positions.
                b.id = blocks.len();
                blocks.push(b);
            }
        }

        // Merge block materializes as the next block pushed by the caller
        // (same deferred-merge scheme as if-expressions — BUG-K02).
        *current_name = "match_merge".to_string();
        Ok(result_local)
    }

    fn lower_if_expr(
        &mut self,
        cond: &HirExpr,
        then: &HirExpr,
        else_: &HirExpr,
        insts: &mut Vec<MirInst>,
        current_name: &mut String,
        blocks: &mut Vec<MirBlock>,
    ) -> Result<LocalId, ()> {
        let cond_id = self.emit_expr(cond, insts, current_name, blocks)?;

        let result_local = self.fresh_local();
        self.locals.push(MirLocal {
            name: format!("tmp_{result_local}"),
            ty: MirType::I32,
        });

        let then_synth = HirBlock {
            stmts: vec![HirStmt::Expr(Box::new(then.clone()), then.span())],
            span: Span::new(Default::default(), Default::default()),
        };
        let else_synth = HirBlock {
            stmts: vec![HirStmt::Expr(Box::new(else_.clone()), else_.span())],
            span: Span::new(Default::default(), Default::default()),
        };

        let mut raw_then_blocks = self.lower_block_to_cfg(&then_synth)?;
        let mut raw_else_blocks = self.lower_block_to_cfg(&else_synth)?;

        let then_len = raw_then_blocks.len();
        let else_len = raw_else_blocks.len();

        let then_start = blocks.len() + 1;
        let else_start = then_start + then_len;
        let merge = else_start + else_len;

        remap_block_ids(&mut raw_then_blocks, then_start);
        remap_block_ids(&mut raw_else_blocks, else_start);

        blocks.push(MirBlock {
            id: blocks.len(),
            name: current_name.clone(),
            insts: std::mem::take(insts),
            terminator: MirTerminator::Branch {
                cond: cond_id,
                then: then_start,
                else_: else_start,
                span: Span::new(Default::default(), Default::default()),
            },
            span: Span::new(Default::default(), Default::default()),
        });

        for (i, mut b) in raw_then_blocks.into_iter().enumerate() {
            if i == then_len - 1 {
                if let MirTerminator::Return { value: Some(val), .. } = &b.terminator {
                    b.insts.push(MirInst::Assign {
                        dest: result_local,
                        value: MirValue::Local(*val),
                        span: Span::new(Default::default(), Default::default()),
                    });
                }
                b.terminator = MirTerminator::Jump {
                    target: merge,
                    span: Span::new(Default::default(), Default::default()),
                };
            }
            b.id = blocks.len();
            blocks.push(b);
        }

        for (i, mut b) in raw_else_blocks.into_iter().enumerate() {
            if i == else_len - 1 {
                if let MirTerminator::Return { value: Some(val), .. } = &b.terminator {
                    b.insts.push(MirInst::Assign {
                        dest: result_local,
                        value: MirValue::Local(*val),
                        span: Span::new(Default::default(), Default::default()),
                    });
                }
                b.terminator = MirTerminator::Jump {
                    target: merge,
                    span: Span::new(Default::default(), Default::default()),
                };
            }
            b.id = blocks.len();
            blocks.push(b);
        }

        // BUG-K02: do NOT push the merge block here and do NOT terminate it with
        // Return. The merge block materializes as the NEXT block pushed by the
        // caller (id == blocks.len() right now), so statements after a mid-function
        // if-expression keep executing. This mirrors how HirStmt::If handles merges.
        *current_name = "if_expr_merge".to_string();
        // current_insts was consumed into the branch block above; leave it empty so
        // subsequent emissions land in the merge block.

        Ok(result_local)
    }
}

fn remap_block_ids(blocks: &mut [MirBlock], offset: usize) {
    for block in blocks.iter_mut() {
        match &mut block.terminator {
            MirTerminator::Jump { target, .. } => *target += offset,
            MirTerminator::Branch { then, else_, .. } => {
                *then += offset;
                *else_ += offset;
            }
            MirTerminator::Return { .. } | MirTerminator::Unreachable => {}
        }
    }
}

fn lower_hir_type(ty: &HirType) -> MirType {
    match ty {
        HirType::I32 => MirType::I32,
        HirType::I64 => MirType::I64,
        HirType::F32 => MirType::F32,
        HirType::F64 => MirType::F64,
        HirType::Bool => MirType::Bool,
        HirType::String => MirType::String,
        HirType::Void => MirType::Void,
        HirType::Named(s) => MirType::Named(s.clone()),
        HirType::Ptr(t) => MirType::Named(format!("*{}", lower_hir_type(t))),
        HirType::Ref(t) => MirType::Named(format!("&{}", lower_hir_type(t))),
        HirType::Array(t, n) => MirType::Named(format!("[{}; {}]", lower_hir_type(t), n)),
        HirType::Slice(t) => MirType::Named(format!("[{}]", lower_hir_type(t))),
        HirType::Fn(args, ret) => {
            let args_str: Vec<String> = args.iter().map(|a| lower_hir_type(a).to_string()).collect();
            MirType::Named(format!("fn({}) -> {}", args_str.join(", "), lower_hir_type(ret)))
        }
    }
}

fn lower_mir_binop(op: HirBinOp) -> Option<MirBinOp> {
    match op {
        HirBinOp::Add => Some(MirBinOp::Add),
        HirBinOp::Sub => Some(MirBinOp::Sub),
        HirBinOp::Mul => Some(MirBinOp::Mul),
        HirBinOp::Div => Some(MirBinOp::Div),
        HirBinOp::Mod => Some(MirBinOp::Mod),
        HirBinOp::Eq => Some(MirBinOp::Eq),
        HirBinOp::Ne => Some(MirBinOp::Ne),
        HirBinOp::Lt => Some(MirBinOp::Lt),
        HirBinOp::Le => Some(MirBinOp::Le),
        HirBinOp::Gt => Some(MirBinOp::Gt),
        HirBinOp::Ge => Some(MirBinOp::Ge),
        HirBinOp::And => Some(MirBinOp::And),
        HirBinOp::Or => Some(MirBinOp::Or),
        HirBinOp::BitAnd => Some(MirBinOp::BitAnd),
        HirBinOp::BitOr => Some(MirBinOp::BitOr),
        HirBinOp::BitXor => Some(MirBinOp::BitXor),
        HirBinOp::Shl => Some(MirBinOp::Shl),
        HirBinOp::Shr => Some(MirBinOp::Shr),
        // BUG-M14: range was silently lowered to Add. For-loop ranges are handled
        // by the dedicated For lowering; a bare `a..b` is now an explicit error.
        HirBinOp::Range => None,
    }
}

fn lower_mir_unop(op: HirUnOp) -> MirUnOp {
    match op {
        HirUnOp::Neg => MirUnOp::Neg,
        HirUnOp::Not => MirUnOp::Not,
        HirUnOp::BitNot => MirUnOp::BitNot,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use brak_core::{Span, DUMMY_SPAN};

    fn dummy_span() -> Span { DUMMY_SPAN }

    fn dummy_hir_fn(name: &str, stmts: Vec<HirStmt>) -> HirFunction {
        HirFunction {
            name: name.to_string(),
            params: vec![],
            ret_ty: HirType::I32,
            body: HirBlock { stmts, span: dummy_span() },
            span: dummy_span(),
        }
    }

    #[test]
    fn test_lower_simple_int_return() {
        let hir_func = dummy_hir_fn("f", vec![
            HirStmt::Return(Some(Box::new(HirExpr::Int(42, dummy_span()))), dummy_span()),
        ]);
        let mut lowerer = MirLower::new();
        let mir_func = lowerer.lower_function(hir_func).unwrap();
        assert_eq!(mir_func.name, "f");
        assert!(mir_func.blocks.len() >= 1);
    }

    #[test]
    fn test_lower_binary_op() {
        let hir_func = dummy_hir_fn("add", vec![
            HirStmt::Return(
                Some(Box::new(HirExpr::BinOp {
                    op: HirBinOp::Add,
                    lhs: Box::new(HirExpr::Int(1, dummy_span())),
                    rhs: Box::new(HirExpr::Int(2, dummy_span())),
                    span: dummy_span(),
                })),
                dummy_span(),
            ),
        ]);
        let mut lowerer = MirLower::new();
        let mir_func = lowerer.lower_function(hir_func).unwrap();
        assert!(!mir_func.blocks.is_empty());
    }

    #[test]
    fn test_lower_if_expr() {
        let hir_func = dummy_hir_fn("test", vec![
            HirStmt::Return(
                Some(Box::new(HirExpr::If {
                    cond: Box::new(HirExpr::Bool(true, dummy_span())),
                    then: Box::new(HirExpr::Int(1, dummy_span())),
                    else_: Box::new(HirExpr::Int(0, dummy_span())),
                    span: dummy_span(),
                })),
                dummy_span(),
            ),
        ]);
        let mut lowerer = MirLower::new();
        let mir_func = lowerer.lower_function(hir_func).unwrap();
        assert!(mir_func.blocks.len() >= 3, "if should produce at least 3 blocks");
    }

    #[test]
    fn test_lower_call() {
        let hir_func = dummy_hir_fn("caller", vec![
            HirStmt::Return(
                Some(Box::new(HirExpr::Call {
                    callee: Box::new(HirExpr::Ident("callee".to_string(), dummy_span())),
                    args: vec![HirExpr::Int(1, dummy_span())],
                    span: dummy_span(),
                })),
                dummy_span(),
            ),
        ]);
        let mut lowerer = MirLower::new();
        let mir_func = lowerer.lower_function(hir_func).unwrap();
        assert!(!mir_func.blocks.is_empty());
    }

    #[test]
    fn test_lower_float() {
        let hir_func = dummy_hir_fn("f", vec![
            HirStmt::Return(
                Some(Box::new(HirExpr::Float(3.14, dummy_span()))),
                dummy_span(),
            ),
        ]);
        let mut lowerer = MirLower::new();
        let mir_func = lowerer.lower_function(hir_func).unwrap();
        assert!(!mir_func.locals.is_empty());
    }

    #[test]
    fn test_lower_block_expr() {
        let hir_func = dummy_hir_fn("f", vec![
            HirStmt::Return(
                Some(Box::new(HirExpr::Block(HirBlock {
                    stmts: vec![HirStmt::Expr(
                        Box::new(HirExpr::Int(1, dummy_span())),
                        dummy_span(),
                    )],
                    span: dummy_span(),
                }))),
                dummy_span(),
            ),
        ]);
        let mut lowerer = MirLower::new();
        let mir_func = lowerer.lower_function(hir_func).unwrap();
        assert!(!mir_func.blocks.is_empty());
    }

    #[test]
    fn test_lower_unary_not() {
        let hir_func = dummy_hir_fn("f", vec![
            HirStmt::Return(
                Some(Box::new(HirExpr::UnOp {
                    op: HirUnOp::Not,
                    expr: Box::new(HirExpr::Bool(false, dummy_span())),
                    span: dummy_span(),
                })),
                dummy_span(),
            ),
        ]);
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
        assert!(lower_mir_binop(Range).is_none(), "Range must not silently become Add");
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
        let hir_func = dummy_hir_fn("f", vec![
            HirStmt::While {
                cond: Box::new(HirExpr::Bool(true, dummy_span())),
                body: HirBlock {
                    stmts: vec![HirStmt::Return(
                        Some(Box::new(HirExpr::Int(5, dummy_span()))),
                        dummy_span(),
                    )],
                    span: dummy_span(),
                },
                span: dummy_span(),
            },
        ]);
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
        let hir_func = dummy_hir_fn("f", vec![
            HirStmt::For {
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
            },
        ]);
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
        let hir_func = dummy_hir_fn("f", vec![
            HirStmt::Loop {
                body: HirBlock {
                    stmts: vec![HirStmt::Return(
                        Some(Box::new(HirExpr::Int(9, dummy_span()))),
                        dummy_span(),
                    )],
                    span: dummy_span(),
                },
                span: dummy_span(),
            },
        ]);
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
        dummy_hir_fn("f", vec![
            HirStmt::Return(
                Some(Box::new(HirExpr::Match {
                    expr: Box::new(HirExpr::Ident("x".to_string(), dummy_span())),
                    arms: matched,
                    span: dummy_span(),
                })),
                dummy_span(),
            ),
        ])
    }

    #[test]
    fn test_match_literal_chain_structure() {
        let hir_func = dummy_hir_fn("f", vec![
            HirStmt::Return(
                Some(Box::new(HirExpr::Match {
                    expr: Box::new(HirExpr::Ident("x".to_string(), dummy_span())),
                    arms: vec![
                        (HirPattern::Literal(brak_ir_hir::hir::HirLiteral::Int(1)), HirExpr::Int(10, dummy_span())),
                        (HirPattern::Literal(brak_ir_hir::hir::HirLiteral::Int(2)), HirExpr::Int(20, dummy_span())),
                        (HirPattern::Wildcard, HirExpr::Int(30, dummy_span())),
                    ],
                    span: dummy_span(),
                })),
                dummy_span(),
            ),
        ]);
        let mut lowerer = MirLower::new();
        // bind scrutinee param
        lowerer.get_or_create_local("x", MirType::I32);
        let f = lowerer.lower_function(hir_func).unwrap();
        // Must contain comparison blocks against each literal, not just fall to arm 0.
        let eq_count = f.blocks.iter()
            .map(|b| b.insts.iter().filter(|i| matches!(i,
                MirInst::Assign { value: MirValue::BinOp { op: MirBinOp::Eq, .. }, .. }
            )).count())
            .sum::<usize>();
        assert_eq!(eq_count, 2, "two literal patterns need two Eq checks, got {eq_count}");
        // No block may terminate Return except the final one (BUG-K02 family).
        let rets: usize = f.blocks.iter().filter(|b|
            matches!(b.terminator, MirTerminator::Return { .. })).count();
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
        let eq_count = f.blocks.iter()
            .map(|b| b.insts.iter().filter(|i| matches!(i,
                MirInst::Assign { value: MirValue::BinOp { op: MirBinOp::Eq, .. }, .. }
            )).count())
            .sum::<usize>();
        assert_eq!(eq_count, 0);
        assert!(f.locals.iter().any(|l| l.name == "v"), "binding pattern creates local");
    }

    #[test]
    fn test_if_expr_mid_function_does_not_terminate() {
        let hir_func = dummy_hir_fn("f", vec![
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
        ]);
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
}



