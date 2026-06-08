use brak_core::Result;
use brak_ir_lir::lir::LirProgram;

pub trait CodegenBackend: Send + Sync {
    fn name(&self) -> &'static str;
    fn emit(&self, program: &LirProgram) -> Result<Vec<u8>>;
}

pub trait CodegenExecutable: CodegenBackend {
    fn emit_executable(&self, program: &LirProgram, entry: &str) -> Result<Vec<u8>>;
}
