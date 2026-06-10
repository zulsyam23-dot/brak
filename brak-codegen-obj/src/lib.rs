pub mod elf;
pub mod coff;
pub mod macho_obj;
pub mod x86_64;
pub mod dwarf;
pub mod codeview;

use brak_codegen_traits::{CodegenBackend, CodegenExecutable};
use brak_core::Result;
use brak_ir_lir::lir::LirProgram;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectFormat {
    Elf,
    Coff,
    Macho,
}

pub struct ObjBackend {
    pub format: ObjectFormat,
}

impl Default for ObjBackend {
    fn default() -> Self {
        let format = if cfg!(target_os = "windows") {
            ObjectFormat::Coff
        } else if cfg!(target_os = "macos") {
            ObjectFormat::Macho
        } else {
            ObjectFormat::Elf
        };
        Self { format }
    }
}

impl CodegenBackend for ObjBackend {
    fn name(&self) -> &'static str {
        "obj"
    }

    fn emit(&self, program: &LirProgram) -> Result<Vec<u8>> {
        match self.format {
            ObjectFormat::Elf => Ok(elf::write_elf(program)?),
            ObjectFormat::Coff => Ok(coff::write_coff(program)?),
            ObjectFormat::Macho => Ok(macho_obj::write_macho(program)?),
        }
    }
}

/// Deprecated: use `NativeLinker` from `brak-link-native` instead.
/// Kept for backward compatibility during Phase 3 migration.
impl CodegenExecutable for ObjBackend {
    fn emit_executable(&self, program: &LirProgram, entry: &str) -> Result<Vec<u8>> {
        match self.format {
            ObjectFormat::Elf => Ok(elf::write_elf_executable(program, entry)?),
            _ => Err(format!(
                "emit_executable not supported for {:?} via this trait. Use brak-link-native.",
                self.format
            )
            .into()),
        }
    }
}

pub fn emit_obj(program: &LirProgram) -> Result<Vec<u8>> {
    let backend = ObjBackend::default();
    backend.emit(program)
}

#[cfg(test)]
mod tests {
    use brak_ir_lir::lir::{LirBlock, LirFunction, LirInst, LirOpcode, LirOperand};
    use iced_x86::Decoder;
    use iced_x86::DecoderOptions;

    fn make_program() -> super::LirProgram {
        let no_span = brak_core::DUMMY_SPAN;

        // fn add(a, b) { return a + b; }
        let add = LirFunction {
            name: "add".into(),
            params: vec![0, 1],
            blocks: vec![LirBlock {
                id: 0,
                name: "entry".into(),
                insts: vec![
                    LirInst::new(LirOpcode::Add).with_dest(2).with_op(LirOperand::Reg(0)).with_op(LirOperand::Reg(1)),
                    LirInst::new(LirOpcode::Ret).with_op(LirOperand::Reg(2)),
                ],
                span: no_span,
            }],
            reg_count: 4,
            span: no_span,
        };

        // fn main() { return add(40, 2); }
        let main = LirFunction {
            name: "main".into(),
            params: vec![],
            blocks: vec![LirBlock {
                id: 0,
                name: "entry".into(),
                insts: vec![
                    LirInst::new(LirOpcode::Mov).with_dest(0).with_op(LirOperand::ImmI64(40)),
                    LirInst::new(LirOpcode::Mov).with_dest(1).with_op(LirOperand::ImmI64(2)),
                    LirInst::new(LirOpcode::Call).with_dest(2).with_op(LirOperand::Label("add".into())).with_op(LirOperand::Reg(0)).with_op(LirOperand::Reg(1)),
                    LirInst::new(LirOpcode::Ret).with_op(LirOperand::Reg(2)),
                ],
                span: no_span,
            }],
            reg_count: 4,
            span: no_span,
        };

        super::LirProgram { 
            functions: vec![main, add],
            extern_functions: vec![],
            structs: vec![],
            enums: vec![],
            string_table: vec![],
            files: vec![],
        }
    }

    #[test]
    fn test_call_emits_mov_args_call_mov_retval() {
        let program = make_program();
        let (text, _, _) = super::x86_64::emit_text(&program).unwrap();

        let mut decoder = Decoder::new(64, &text, DecoderOptions::NONE);
        decoder.set_ip(0);
        let mut instrs: Vec<String> = Vec::new();
        while decoder.can_decode() {
            let instr = decoder.decode();
            instrs.push(instr.to_string());
        }

        // Verify main function: mov args to rdi/rsi, call add, mov rax to rcx
        let idx_main_call = instrs.iter().position(|i| i.contains("call"));
        assert!(idx_main_call.is_some(), "should have a call instruction:\n{instrs:#?}");
        let idx = idx_main_call.unwrap();
        let pre_call = &instrs[..idx];
        let mov_rdi = pre_call.iter().position(|i| i.starts_with("mov rdi,"));
        let mov_rsi = pre_call.iter().position(|i| i.starts_with("mov rsi,"));
        assert!(mov_rdi.is_some(), "expected mov rdi before call:\n{instrs:#?}");
        assert!(mov_rsi.is_some(), "expected mov rsi before call:\n{instrs:#?}");
        assert!(
            mov_rdi.unwrap() < mov_rsi.unwrap(),
            "mov rdi should come before mov rsi"
        );
        // dest is virt reg 2 → vreg_ptr(2) = [rbp-18h]
        let post_call = &instrs[idx + 1..];
        let mov_dest = post_call.iter().position(|i| i.contains("rbp-18h") && i.starts_with("mov"));
        assert!(
            mov_dest.is_some(),
            "expected mov [rbp-18h],rax after call, got:\n{instrs:#?}"
        );
    }
}
