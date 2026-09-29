use super::types::{lower_mir_binop, lower_mir_unop};
use super::utils::remap_block_ids;
use super::MirLower;
use crate::mir::*;
use brak_core::Span;
use brak_ir_hir::hir::*;

impl MirLower {
    pub(super) fn emit_expr(
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
                let is_float = |id: LocalId| {
                    matches!(
                        self.locals.get(id).map(|l| &l.ty),
                        Some(MirType::F32 | MirType::F64)
                    )
                };
                let float_arith = matches!(
                    mir_op,
                    MirBinOp::Add | MirBinOp::Sub | MirBinOp::Mul | MirBinOp::Div
                ) && (is_float(lhs_id) || is_float(rhs_id));
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
                    ty: if float_arith {
                        MirType::F64
                    } else {
                        MirType::I32
                    },
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
            HirExpr::If {
                cond, then, else_, ..
            } => self.lower_if_expr(cond, then, else_, insts, current_name, blocks),
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
            HirExpr::Match {
                expr: scrutinee,
                arms,
                span,
            } => self.lower_match_expr(scrutinee, arms, *span, insts, current_name, blocks),
            HirExpr::Field {
                object,
                field,
                span,
            } => {
                let obj_id = self.emit_expr(object, insts, current_name, blocks)?;
                // Fase 7: resolve the object's struct name from its local type
                // so backends can compute the field offset.
                let struct_name = match self.locals.get(obj_id).map(|l| &l.ty) {
                    Some(MirType::Named(n)) => n.clone(),
                    _ => "unknown".to_string(),
                };
                // ponytail: field result typed I64 (runtime slots are 64-bit);
                // per-field declared types flow through once struct metadata
                // reaches MIR lowering.
                let id = self.fresh_local();
                self.locals.push(MirLocal {
                    name: format!("tmp_{id}"),
                    ty: MirType::I64,
                });
                insts.push(MirInst::Assign {
                    dest: id,
                    value: MirValue::GetField {
                        object: obj_id,
                        name: struct_name,
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
            HirExpr::EnumInit {
                enum_name,
                variant,
                args,
                span,
            } => {
                // Fase 7: an enum value is an aggregate `[tag, payload...]`
                // built with StructInit over a synthetic per-enum struct
                // (`__enum_<Name>`); backends that support aggregates
                // allocate it, and match destructures via GetField.
                let tag = self
                    .enum_variant_tag(&enum_name, &variant)
                    .expect("typeck validated enum variant before MIR lowering");
                let id = self.fresh_local();
                self.locals.push(MirLocal {
                    name: format!("tmp_{id}"),
                    ty: MirType::Named(enum_name.clone()),
                });

                // Materialize the tag into a temp local.
                let tag_local = self.fresh_local();
                self.locals.push(MirLocal {
                    name: format!("tmp_{tag_local}"),
                    ty: MirType::I64,
                });
                insts.push(MirInst::Assign {
                    dest: tag_local,
                    value: MirValue::Int(tag as i64),
                    span: *span,
                });

                let arg_ids: Vec<LocalId> = args
                    .iter()
                    .map(|a| self.emit_expr(a, insts, current_name, blocks))
                    .collect::<Result<_, _>>()?;

                let mut fields = vec![("$tag".to_string(), tag_local)];
                for (i, a) in arg_ids.iter().enumerate() {
                    fields.push((format!("${i}"), *a));
                }
                insts.push(MirInst::Assign {
                    dest: id,
                    value: MirValue::StructInit {
                        name: format!("__enum_{enum_name}"),
                        fields,
                    },
                    span: *span,
                });
                Ok(id)
            }
            HirExpr::FieldAssign {
                object,
                field,
                value,
                span,
            } => {
                let obj_id = self.emit_expr(object, insts, current_name, blocks)?;
                let val_id = self.emit_expr(value, insts, current_name, blocks)?;
                // Fase 7: resolve the object's struct name so backends can
                // compute the field offset (was impossible before).
                let struct_name = match self.locals.get(obj_id).map(|l| &l.ty) {
                    Some(MirType::Named(n)) => n.clone(),
                    _ => "unknown".to_string(),
                };
                let id = self.fresh_local();
                self.locals.push(MirLocal {
                    name: format!("tmp_{id}"),
                    ty: MirType::I32, // Simplified
                });
                insts.push(MirInst::Assign {
                    dest: id,
                    value: MirValue::SetField {
                        object: obj_id,
                        name: struct_name,
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
                if let MirTerminator::Return {
                    value: Some(val), ..
                } = &b.terminator
                {
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
                if let MirTerminator::Return {
                    value: Some(val), ..
                } = &b.terminator
                {
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
