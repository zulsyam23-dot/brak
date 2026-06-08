use brak_core::Result;
use brak_ir_lir::lir::{LirProgram, LirInst, LirOpcode};
use brak_opt_traits::LirOptimizationPass;

pub struct CustomCommentPass;

impl LirOptimizationPass for CustomCommentPass {
    fn name(&self) -> &'static str {
        "custom-comment"
    }

    fn run(&self, mut program: LirProgram) -> Result<LirProgram> {
        for func in &mut program.functions {
            if let Some(first_block) = func.blocks.get_mut(0) {
                first_block.insts.insert(0, LirInst::new(LirOpcode::Comment));
            }
        }
        Ok(program)
    }
}

#[no_mangle]
pub extern "Rust" fn create_pass() -> Box<dyn LirOptimizationPass> {
    Box::new(CustomCommentPass)
}
