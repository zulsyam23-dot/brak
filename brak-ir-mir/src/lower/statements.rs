use super::types::lower_hir_type;
use super::utils::remap_block_ids;
use super::{LoopContext, MirLower};
use crate::mir::*;
use brak_ir_hir::hir::*;

impl MirLower {
    pub(super) fn lower_block_to_cfg(&mut self, block: &HirBlock) -> Result<Vec<MirBlock>, ()> {
        let mut blocks = vec![];
        let mut current_insts = vec![];
        let mut current_name = "entry".to_string();
        let mut last_expr_result: Option<LocalId> = None;
        let block_span = block.span;

        for stmt in &block.stmts {
            match stmt {
                HirStmt::Let {
                    name,
                    ty,
                    value,
                    span,
                    ..
                } => {
                    last_expr_result = None;
                    let local_id = self.get_or_create_local(name, lower_hir_type(ty));
                    if let Some(v) = value {
                        let val_id =
                            self.emit_expr(v, &mut current_insts, &mut current_name, &mut blocks)?;
                        current_insts.push(MirInst::Assign {
                            dest: local_id,
                            value: MirValue::Local(val_id),
                            span: *span,
                        });
                    }
                }
                HirStmt::Expr(e, _) => {
                    last_expr_result = Some(self.emit_expr(
                        e,
                        &mut current_insts,
                        &mut current_name,
                        &mut blocks,
                    )?);
                }
                HirStmt::Return(v, span) => {
                    let value = match v {
                        Some(v) => Some(self.emit_expr(
                            v,
                            &mut current_insts,
                            &mut current_name,
                            &mut blocks,
                        )?),
                        None => None,
                    };
                    blocks.push(MirBlock {
                        id: blocks.len(),
                        name: "unreachable".to_string(),
                        insts: current_insts,
                        terminator: MirTerminator::Return { value, span: *span },
                        span: block_span,
                    });
                    current_insts = vec![];
                    current_name = "unreachable".to_string();
                }
                HirStmt::If {
                    cond,
                    then,
                    else_,
                    span,
                } => {
                    let cond_id =
                        self.emit_expr(cond, &mut current_insts, &mut current_name, &mut blocks)?;

                    let mut then_blocks = self.lower_block_to_cfg(then)?;
                    let mut else_blocks = match else_ {
                        Some(b) => self.lower_block_to_cfg(b)?,
                        None => vec![],
                    };

                    let then_falls_through = then_blocks
                        .last()
                        .map_or(false, |b| b.name != "unreachable");
                    let then_value = then_blocks.last().and_then(|b| match &b.terminator {
                        MirTerminator::Return { value, .. } => *value,
                        _ => None,
                    });
                    let else_falls_through = match else_ {
                        Some(_) => else_blocks
                            .last()
                            .map_or(false, |b| b.name != "unreachable"),
                        None => true,
                    };
                    let else_value = else_blocks.last().and_then(|b| match &b.terminator {
                        MirTerminator::Return { value, .. } => *value,
                        _ => None,
                    });
                    let has_fallthrough = then_falls_through || else_falls_through;
                    let all_fallthroughs_have_values = (!then_falls_through
                        || then_value.is_some())
                        && (!else_falls_through || else_value.is_some());
                    let result_local = if has_fallthrough && all_fallthroughs_have_values {
                        let id = self.fresh_local();
                        self.locals.push(MirLocal {
                            name: format!("tmp_{id}"),
                            ty: MirType::I32,
                        });
                        Some(id)
                    } else {
                        None
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
                            let branch_value = match &b.terminator {
                                MirTerminator::Return {
                                    value: Some(value),
                                    span,
                                } => Some((*value, *span)),
                                _ => None,
                            };
                            if let (Some(dest), Some((value, value_span))) =
                                (result_local, branch_value)
                            {
                                b.insts.push(MirInst::Assign {
                                    dest,
                                    value: MirValue::Local(value),
                                    span: value_span,
                                });
                            }
                            b.terminator = MirTerminator::Jump {
                                target: after,
                                span: *span,
                            };
                        }
                        b.id = blocks.len();
                        blocks.push(b);
                    }

                    // Push else blocks
                    for mut b in else_blocks {
                        if b.name == "unreachable" {
                            // real return statement — keep terminator
                        } else {
                            let branch_value = match &b.terminator {
                                MirTerminator::Return {
                                    value: Some(value),
                                    span,
                                } => Some((*value, *span)),
                                _ => None,
                            };
                            if let (Some(dest), Some((value, value_span))) =
                                (result_local, branch_value)
                            {
                                b.insts.push(MirInst::Assign {
                                    dest,
                                    value: MirValue::Local(value),
                                    span: value_span,
                                });
                            }
                            b.terminator = MirTerminator::Jump {
                                target: after,
                                span: *span,
                            };
                        }
                        b.id = blocks.len();
                        blocks.push(b);
                    }

                    // Start the 'after' block
                    current_insts = vec![];
                    current_name = "if_merge".to_string();
                    last_expr_result = result_local;
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
                    let cond_id =
                        self.emit_expr(cond, &mut cond_insts, &mut dummy_name, &mut blocks)?;

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
                                b.terminator = MirTerminator::Jump {
                                    target: cond_header + 1,
                                    span: *span,
                                };
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
                HirStmt::For {
                    var,
                    iterable,
                    body,
                    span,
                } => {
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
                    let iter_id =
                        self.emit_expr(iterable, &mut init_insts, &mut dummy_name, &mut blocks)?;
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
}
