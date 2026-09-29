use super::utils::remap_block_ids;
use super::MirLower;
use crate::mir::*;
use brak_core::Span;
use brak_ir_hir::hir::*;

impl MirLower {
    /// Return the 0-based tag of a variant in its enum.
    pub(super) fn enum_variant_tag(&self, enum_name: &str, variant: &str) -> Option<usize> {
        self.enum_tags
            .get(enum_name)?
            .iter()
            .position(|name| name == variant)
    }

    /// Return a comparable pattern value; `None` represents a catch-all.
    fn pattern_discriminant(&self, pattern: &HirPattern) -> Option<i64> {
        match pattern {
            HirPattern::Literal(HirLiteral::Int(value)) => Some(*value),
            HirPattern::Literal(HirLiteral::Bool(value)) => Some(*value as i64),
            HirPattern::Variant {
                enum_name, variant, ..
            } => self
                .enum_variant_tag(enum_name, variant)
                .map(|tag| tag as i64),
            _ => None,
        }
    }

    pub(super) fn lower_match_expr(
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

        // Fase 7: if any arm matches an enum variant, the scrutinee holds a
        // POINTER to [tag, payload...]; extract the tag once and compare
        // against it instead of the pointer itself.
        let enum_scrut: Option<LocalId> = {
            let first_variant = arms.iter().find_map(|(p, _)| match p {
                HirPattern::Variant { enum_name, .. } => Some(enum_name.clone()),
                _ => None,
            });
            if let Some(enum_name) = first_variant {
                let tag_local = self.fresh_local();
                self.locals.push(MirLocal {
                    name: format!("tmp_{tag_local}"),
                    ty: MirType::I64,
                });
                insts.push(MirInst::Assign {
                    dest: tag_local,
                    value: MirValue::GetField {
                        object: scrut_id,
                        name: format!("__enum_{enum_name}"),
                        field: "$tag".to_string(),
                    },
                    span,
                });
                Some(tag_local)
            } else {
                None
            }
        };

        // Fase 7: discriminants cover Int literals, Bool literals, AND enum
        // variant tags. First catch-all (wildcard/binding/other literal) ends
        // the comparable chain.
        let discs: Vec<Option<i64>> = arms
            .iter()
            .map(|(pat, _)| self.pattern_discriminant(pat))
            .collect();
        let mut eff_len = arms.len();
        for (i, d) in discs.iter().enumerate() {
            if d.is_none() {
                eff_len = i + 1;
                break;
            }
        }

        // Fase 7: BINDINGS MUST BE DECLARED BEFORE BODIES ARE LOWERED — an arm
        // body referencing its destructured variable would otherwise create
        // uninitialized temporaries.
        let arm_bind_insts: Vec<Vec<MirInst>> = arms
            .iter()
            .take(eff_len)
            .map(|(pat, _)| {
                match pat {
                    HirPattern::Binding(name) => {
                        let l = self.get_or_create_local(name, MirType::I64);
                        // NOTE: value filled in later (needs scrut_id only).
                        vec![MirInst::Assign {
                            dest: l,
                            value: MirValue::Local(scrut_id),
                            span,
                        }]
                    }
                    HirPattern::Variant {
                        enum_name,
                        variant: _,
                        bindings,
                    } => {
                        let mut insts_v = vec![];
                        for (i_payload, b) in bindings.iter().enumerate() {
                            if b == "_" {
                                continue;
                            }
                            let l = self.get_or_create_local(b, MirType::I64);
                            insts_v.push(MirInst::Assign {
                                dest: l,
                                value: MirValue::GetField {
                                    object: scrut_id,
                                    name: format!("__enum_{enum_name}"),
                                    field: format!("${i_payload}"),
                                },
                                span,
                            });
                        }
                        insts_v
                    }
                    _ => vec![],
                }
            })
            .collect();

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
            let needs_check = !is_last && discs[i].is_some();
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
            terminator: MirTerminator::Jump {
                target: entry[0],
                span,
            },
            span: dummy(),
        });

        for i in 0..eff_len {
            let is_last = i == eff_len - 1;

            // Check block: compare scrutinee against the arm's discriminant
            // (int/bool literal or enum variant tag — Fase 7).
            if let Some(disc) = discs[i] {
                if !is_last {
                    {
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
                                    value: MirValue::Int(disc),
                                    span,
                                },
                                MirInst::Assign {
                                    dest: cond_local,
                                    value: MirValue::BinOp {
                                        op: MirBinOp::Eq,
                                        // Variant arms compare the extracted TAG,
                                        // not the aggregate pointer.
                                        lhs: enum_scrut.unwrap_or(scrut_id),
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
            }

            // Binding patterns: prepend the pre-computed bind instructions.
            let bind_insts = arm_bind_insts[i].clone();

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
                    if let MirTerminator::Return {
                        value: Some(val), ..
                    } = &last.terminator
                    {
                        last.insts.push(MirInst::Assign {
                            dest: result_local,
                            value: MirValue::Local(*val),
                            span,
                        });
                    }
                    last.terminator = MirTerminator::Jump {
                        target: merge,
                        span,
                    };
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
}
