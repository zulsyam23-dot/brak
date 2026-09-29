use std::collections::HashMap;

use brak_core::Diagnostics;
use brak_ir_hir::hir::*;

use crate::mir::*;

mod expressions;
mod matches;
mod statements;
#[cfg(test)]
mod tests;
mod utils;

mod types;
use self::types::lower_hir_type;
#[cfg(test)]
use self::types::lower_mir_binop;

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
    /// Fase 7: enum name -> ordered variant names (tag = index).
    enum_tags: HashMap<String, Vec<String>>,
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
            enum_tags: HashMap::new(),
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
        // Fase 7 pre-pass: register enum variant tags before lowering any
        // function body (constructors may appear in earlier items).
        for item in &program.items {
            if let HirItem::Enum(e) = item {
                self.enum_tags.insert(
                    e.name.clone(),
                    e.variants.iter().map(|v| v.name.clone()).collect(),
                );
            }
        }
        for item in program.items {
            match item {
                HirItem::Enum(e) => {
                    enums.push(MirEnum {
                        name: e.name,
                        variants: e
                            .variants
                            .into_iter()
                            .map(|v| MirVariant {
                                name: v.name,
                                fields: v.fields.map(|fs| fs.iter().map(lower_hir_type).collect()),
                                span: v.span,
                            })
                            .collect(),
                        span: e.span,
                    });
                }
                HirItem::Function(f) => {
                    if let Ok(mf) = self.lower_function(f) {
                        functions.push(mf);
                    }
                }
                HirItem::ExternFunction(e) => {
                    extern_functions.push(MirExternFunction {
                        name: e.name,
                        params: e
                            .params
                            .into_iter()
                            .map(|p| lower_hir_type(&p.ty))
                            .collect(),
                        ret_ty: lower_hir_type(&e.ret_ty),
                        abi: e.abi,
                        span: e.span,
                    });
                }
                HirItem::Struct(s) => {
                    structs.push(MirStruct {
                        name: s.name,
                        fields: s
                            .fields
                            .into_iter()
                            .map(|f| MirField {
                                name: f.name,
                                ty: lower_hir_type(&f.ty),
                                span: f.span,
                            })
                            .collect(),
                        span: s.span,
                    });
                }
                HirItem::GlobalLet(_) => {}
            }
        }
        if self.diagnostics.has_errors() {
            Err(std::mem::take(&mut self.diagnostics))
        } else {
            Ok(MirProgram {
                functions,
                extern_functions,
                structs,
                enums,
            })
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
}
