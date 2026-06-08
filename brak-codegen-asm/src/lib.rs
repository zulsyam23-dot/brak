pub mod regalloc;
pub mod x86_64;

use brak_codegen_traits::CodegenBackend;
use brak_core::Result;
use brak_ir_lir::lir::LirProgram;

pub struct AsmBackend;

impl CodegenBackend for AsmBackend {
    fn name(&self) -> &'static str {
        "asm"
    }

    fn emit(&self, program: &LirProgram) -> Result<Vec<u8>> {
        Ok(emit_asm(program).into_bytes())
    }
}

pub fn emit_asm(program: &LirProgram) -> String {
    let mut out = String::new();
    out.push_str("section .text\n");
    out.push_str("global _start\n\n");
    for func in &program.functions {
        let func_asm = x86_64::emit_function(func, &mut regalloc::SimpleAlloc::new(func.reg_count));
        out.push_str(&func_asm);
    }
    out
}
