use std::collections::HashMap;
use std::fmt;
use brak_ir_lir::lir::{LirInst, LirOpcode, LirOperand, LirProgram, CallingConvention};
use iced_x86::code_asm::*;


#[derive(Debug, Clone)]
pub struct LineEntry {
    pub offset: usize,
    pub line: usize,
    pub column: usize,
    pub file_id: usize,
    pub end_sequence: bool,
}

#[derive(Debug)]
pub enum CodegenError {
    Iced(IcedError),
    MissingDest(LirOpcode),
    InvalidLabel(String),
    Unsupported(LirOpcode),
    TooManyParams { callee: String, got: usize, max: usize },
}

impl fmt::Display for CodegenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CodegenError::Iced(e) => write!(f, "iced error: {e}"),
            CodegenError::MissingDest(op) => write!(f, "missing destination register for {op:?}"),
            CodegenError::InvalidLabel(msg) => write!(f, "invalid label: {msg}"),
            CodegenError::Unsupported(op) => write!(
                f,
                "opcode {op:?} is not yet supported by the native obj backend \
                 (structs/pointers require layout support)"
            ),
            CodegenError::TooManyParams { callee, got, max } => write!(
                f,
                "call to '{callee}' has {got} arguments but the calling convention \
                 supports at most {max} in registers (stack args not yet supported)"
            ),
        }
    }
}

impl std::error::Error for CodegenError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            CodegenError::Iced(e) => Some(e),
            _ => None,
        }
    }
}

impl From<IcedError> for CodegenError {
    fn from(e: IcedError) -> Self {
        CodegenError::Iced(e)
    }
}

pub fn phys(reg: usize) -> usize {
    reg & 7
}

pub fn r64(reg: usize) -> AsmRegister64 {
    match phys(reg) {
        0 => rax,
        1 => rcx,
        2 => rdx,
        3 => rbx,
        4 => rsi,
        5 => rdi,
        6 => r8,
        7 => r9,
        _ => unreachable!(),
    }
}

pub fn reg8(reg: usize) -> AsmRegister8 {
    match phys(reg) {
        0 => al,
        1 => cl,
        2 => dl,
        3 => bl,
        4 => sil,
        5 => dil,
        6 => r8b,
        7 => r9b,
        _ => unreachable!(),
    }
}

pub const CALLER_SAVED: [AsmRegister64; 7] = [rax, rcx, rdx, rsi, rdi, r8, r9];

pub fn save_caller_saved(a: &mut CodeAssembler) -> Result<(), IcedError> {
    for &reg in &CALLER_SAVED {
        a.push(reg)?;
    }
    Ok(())
}

pub fn restore_caller_saved(a: &mut CodeAssembler) -> Result<(), IcedError> {
    for &reg in CALLER_SAVED.iter().rev() {
        a.pop(reg)?;
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub struct Reloc {
    pub offset: usize,
    pub target_name: String,
    pub is_relative: bool,
}

struct PendingReloc {
    target_name: String,
    is_relative: bool,
}

pub fn emit_text(program: &LirProgram) -> Result<(Vec<u8>, Vec<Reloc>, Vec<LineEntry>), CodegenError> {
    let mut func_labels: HashMap<String, CodeLabel> = HashMap::new();
    let mut block_labels: HashMap<(String, usize), CodeLabel> = HashMap::new();
    let mut block_name_labels: HashMap<(String, String), CodeLabel> = HashMap::new();

    let mut buf = Vec::new();
    let mut all_relocs = Vec::new();
    let mut all_lines = Vec::new();
    let mut current_offset = 0;

    for func in &program.functions {
        let mut func_lines = Vec::new();
        let (code, relocs) = emit_function(func, &mut func_labels, &mut block_labels, &mut block_name_labels, &mut func_lines)?;
        for entry in &mut func_lines {
            entry.offset += current_offset;
        }
        all_lines.extend(func_lines);
        for mut r in relocs {
            r.offset += current_offset;
            all_relocs.push(r);
        }
        current_offset += code.len();
        buf.extend(code);
    }
    Ok((buf, all_relocs, all_lines))
}

pub fn emit_function(
    func: &brak_ir_lir::lir::LirFunction,
    func_labels: &mut HashMap<String, CodeLabel>,
    block_labels: &mut HashMap<(String, usize), CodeLabel>,
    block_name_labels: &mut HashMap<(String, String), CodeLabel>,
    line_entries: &mut Vec<LineEntry>,
) -> Result<(Vec<u8>, Vec<Reloc>), CodegenError> {
    let mut a = CodeAssembler::new(64)?;
    let mut relocs: Vec<Reloc> = Vec::new();
    let mut pending_relocs = Vec::new();
    let mut lir_sizes: Vec<usize> = Vec::new();
    let mut lir_line_map: Vec<(usize, usize, usize, bool)> = Vec::new();

    func_labels.insert(func.name.clone(), a.create_label());
    for block in &func.blocks {
        let label = a.create_label();
        block_labels.insert((func.name.clone(), block.id), label.clone());
        block_name_labels.insert((func.name.clone(), block.name.clone()), label);
    }

    let flabel = func_labels.get_mut(&func.name).ok_or_else(|| {
        CodegenError::InvalidLabel(format!("function label '{}' not found", func.name))
    })?;
    a.set_label(flabel)?;

    a.push(rbp)?;
    a.mov(rbp, rsp)?;
    let stack = func.reg_count * 8;
    if stack > 0 {
        a.sub(rsp, stack as i32)?;
    }

    for (i, param) in func.params.iter().enumerate() {
        // Parameter registers follow the HOST calling convention so exported
        // functions are directly callable from C/FFI (BUG: was hardcoded SysV).
        let reg = if native_call_conv() == CallingConvention::Win64 {
            match i { 0 => rcx, 1 => rdx, 2 => r8, 3 => r9, _ => continue }
        } else {
            match i { 0 => rdi, 1 => rsi, 2 => rdx, 3 => rcx, 4 => r8, 5 => r9, _ => continue }
        };
        a.mov(vreg_ptr(*param), reg)?;
    }

    for block in &func.blocks {
        let key = (func.name.clone(), block.id);
        let blabel = block_labels.get_mut(&key).ok_or_else(|| {
            CodegenError::InvalidLabel(format!("block label '{}.{}' not found", func.name, block.id))
        })?;
        a.set_label(blabel)?;

        // Ensure every block has at least one instruction to anchor the label.
        // If the block is empty or contains only non-emitting opcodes (like Comment),
        // we add a NOP.
        let mut emitted = false;
        for inst in &block.insts {
            let before = a.instructions().len();
            emit_inst(&mut a, inst, block_labels, block_name_labels, func_labels, &func.name, &mut pending_relocs)?;
            let after = a.instructions().len();
            if after > before {
                emitted = true;
            }
            lir_sizes.push(after - before);
            if inst.debug.start.line > 0 || inst.debug.start.offset > 0 {
                lir_line_map.push((inst.debug.start.line, inst.debug.start.column, inst.file_id, false));
            } else {
                lir_line_map.push((0, 0, 0, false));
            }
        }

        if !emitted {
            a.nop()?;
            lir_sizes.push(1);
            lir_line_map.push((0, 0, 0, false));
        }
    }

    a.mov(rsp, rbp)?;
    a.pop(rbp)?;
    a.ret()?;

    let code = a.assemble(0x0)?;

    // BUG-L06: locate call placeholders deterministically. Each pending reloc is
    // emitted as `E8 <u32 index>` where index is its 1-based position in
    // pending_relocs — an immediate collision with a real E8 instruction is
    // effectively impossible, unlike the old scan for `E8 00 00 00 00`.
    let mut relocs = Vec::with_capacity(pending_relocs.len());
    'relocs: for (idx, pr) in pending_relocs.iter().enumerate() {
        let marker = (idx as u32 + 1).to_le_bytes();
        let mut byte_pos = 0;
        while byte_pos + 5 <= code.len() {
            if code[byte_pos] == 0xE8 && &code[byte_pos + 1..byte_pos + 5] == &marker {
                relocs.push(Reloc {
                    offset: byte_pos + 1, // rel32 field starts after E8 opcode
                    target_name: pr.target_name.clone(),
                    is_relative: pr.is_relative,
                });
                continue 'relocs;
            }
            byte_pos += 1;
        }
        return Err(CodegenError::InvalidLabel(format!(
            "call placeholder for '{}' not found in encoded output", pr.target_name
        )));
    }

    // Decode instructions to get byte sizes (insts_slice[].len() is always 0 even after assemble())
    let mut dec = iced_x86::Decoder::new(64, &code, iced_x86::DecoderOptions::NONE);
    dec.set_ip(0);
    let mut inst_sizes: Vec<usize> = Vec::new();
    while dec.can_decode() {
        let inst = dec.decode();
        inst_sizes.push(inst.len());
    }

    let mut func_offset: usize = 0;
    let mut asm_idx: usize = 0;
    for (&size, &(line, col, fid, _)) in lir_sizes.iter().zip(lir_line_map.iter()) {
        let mut group_size = 0;
        for _ in 0..size {
            if asm_idx < inst_sizes.len() {
                group_size += inst_sizes[asm_idx];
                asm_idx += 1;
            }
        }
        if line > 0 {
            line_entries.push(LineEntry {
                offset: func_offset,
                line,
                column: col,
                file_id: fid,
                end_sequence: false,
            });
        }
        func_offset += group_size;
    }
    if let Some(last) = line_entries.last_mut() {
        last.end_sequence = true;
    }

    Ok((code, relocs))
}

pub fn vreg_ptr(reg: usize) -> iced_x86::code_asm::AsmMemoryOperand {
    qword_ptr(rbp - ((reg + 1) * 8) as i32)
}

fn emit_inst(
    a: &mut CodeAssembler,
    inst: &LirInst,
    block_labels: &mut HashMap<(String, usize), CodeLabel>,
    block_name_labels: &mut HashMap<(String, String), CodeLabel>,
    func_labels: &mut HashMap<String, CodeLabel>,
    func_name: &String,
    pending_relocs: &mut Vec<PendingReloc>,
) -> Result<(), CodegenError> {
    match inst.opcode {
        LirOpcode::Comment => {}

        LirOpcode::Mov => match (inst.dest, &inst.operands[..]) {
            (Some(d), [LirOperand::ImmI64(val)]) => {
                a.mov(rax, *val as u64)?;
                a.mov(vreg_ptr(d), rax)?;
            }
            (Some(d), [LirOperand::ImmF64(val)]) => {
                a.mov(rax, val.to_bits())?;
                a.mov(vreg_ptr(d), rax)?;
            }
            (Some(d), [LirOperand::Reg(src)]) => {
                a.mov(rax, vreg_ptr(*src))?;
                a.mov(vreg_ptr(d), rax)?;
            }
            (Some(d), [LirOperand::StackSlot(slot)]) => {
                let offset = (*slot + 1) * 8;
                a.mov(rax, qword_ptr(rbp - offset))?;
                a.mov(vreg_ptr(d), rax)?;
            }
            (Some(d), [LirOperand::StringRef(_)]) => {
                // BUG-K09: was a silent `mov rax, 0` — a null-pointer time bomb.
                // Real .rodata emission needs section support in elf/coff/macho;
                // failing loudly beats handing out null pointers.
                return Err(CodegenError::Unsupported(LirOpcode::Mov));
            }
            _ => {}
        },

        LirOpcode::Add => emit_binop(a, inst)?,
        LirOpcode::Sub => emit_binop(a, inst)?,
        LirOpcode::Mul => emit_binop(a, inst)?,
        LirOpcode::And => emit_binop(a, inst)?,
        LirOpcode::Or => emit_binop(a, inst)?,
        LirOpcode::Xor => emit_binop(a, inst)?,
        // BUG-K09: Shl/Shr existed in emit_binop but were never routed here —
        // shift instructions were silently discarded (`_ => {}`).
        LirOpcode::Shl => emit_binop(a, inst)?,
        LirOpcode::Shr => emit_binop(a, inst)?,

        // BUG-M17: f64 arithmetic via SSE2. Values live in stack slots as raw
        // bits, so load/store through movsd on xmm registers.
        LirOpcode::FAdd | LirOpcode::FSub | LirOpcode::FMul | LirOpcode::FDiv => {
            let d = inst.dest.ok_or(CodegenError::MissingDest(inst.opcode))?;
            let lhs = match inst.operands.first() {
                Some(LirOperand::Reg(r)) => vreg_ptr(*r),
                _ => return Ok(()),
            };
            a.movq(xmm0, lhs)?;
            match inst.operands.get(1) {
                Some(LirOperand::Reg(r)) => {
                    a.movq(xmm1, vreg_ptr(*r))?;
                }
                Some(LirOperand::ImmF64(f)) => {
                    // Materialize immediate bits into rax, then bit-cast to xmm1
                    // (no extra stack slot needed).
                    a.mov(rax, f.to_bits())?;
                    a.movq(xmm1, rax)?;
                }
                _ => return Ok(()),
            }
            match inst.opcode {
                LirOpcode::FAdd => a.addsd(xmm0, xmm1)?,
                LirOpcode::FSub => a.subsd(xmm0, xmm1)?,
                LirOpcode::FMul => a.mulsd(xmm0, xmm1)?,
                LirOpcode::FDiv => a.divsd(xmm0, xmm1)?,
                _ => {}
            }
            a.movq(vreg_ptr(d), xmm0)?;
        }

        LirOpcode::Div | LirOpcode::Mod => {
            let d = inst.dest.ok_or(CodegenError::MissingDest(inst.opcode))?;

            if !load_operand(a, inst.operands.first())? {
                return Ok(());
            }
            a.cqo()?;

            match inst.operands.get(1) {
                Some(LirOperand::Reg(r)) => {
                    a.idiv(vreg_ptr(*r))?;
                }
                Some(LirOperand::ImmI64(val)) => {
                    a.mov(rcx, *val as u64)?;
                    a.idiv(rcx)?;
                }
                _ => return Ok(()),
            }

            match inst.opcode {
                LirOpcode::Div => a.mov(vreg_ptr(d), rax)?,
                LirOpcode::Mod => a.mov(vreg_ptr(d), rdx)?,
                _ => {}
            }
        }

        LirOpcode::Jmp => {
            if let Some(LirOperand::Label(target)) = inst.operands.first() {
                // 1. Try numeric ID (block_N)
                if let Some(target_id) = target.strip_prefix("block_").and_then(|s| s.parse::<usize>().ok()) {
                    let key = (func_name.clone(), target_id);
                    let label = block_labels.get_mut(&key).ok_or_else(|| {
                        CodegenError::InvalidLabel(format!("target block ID '{target}' not found"))
                    })?;
                    a.jmp(label.clone())?;
                }
                // 2. Try raw name
                else if let Some(label) = block_name_labels.get_mut(&(func_name.clone(), target.clone())) {
                    a.jmp(label.clone())?;
                }
                else {
                    return Err(CodegenError::InvalidLabel(format!("Jmp target '{target}' not found as ID or name")));
                }
            }
        }

        LirOpcode::Br => {
            if let [LirOperand::Reg(cond), LirOperand::Label(then_label), LirOperand::Label(else_label)] = &inst.operands[..] {
                a.mov(rax, vreg_ptr(*cond))?;
                a.test(rax, rax)?;

                // Resolve then label
                let t_label = {
                    let lbl = if let Some(target_id) = then_label.strip_prefix("block_").and_then(|s| s.parse::<usize>().ok()) {
                        block_labels.get_mut(&(func_name.clone(), target_id))
                    } else {
                        block_name_labels.get_mut(&(func_name.clone(), then_label.clone()))
                    }.ok_or_else(|| CodegenError::InvalidLabel(format!("Br then target '{then_label}' not found")))?;
                    lbl.clone()
                };

                // Resolve else label
                let e_label = {
                    let lbl = if let Some(target_id) = else_label.strip_prefix("block_").and_then(|s| s.parse::<usize>().ok()) {
                        block_labels.get_mut(&(func_name.clone(), target_id))
                    } else {
                        block_name_labels.get_mut(&(func_name.clone(), else_label.clone()))
                    }.ok_or_else(|| CodegenError::InvalidLabel(format!("Br else target '{else_label}' not found")))?;
                    lbl.clone()
                };

                a.jnz(t_label)?;
                a.jmp(e_label)?;
            }
        }

        LirOpcode::Cmp => {
            if !load_operand(a, inst.operands.first())? {
                return Ok(());
            }
            match inst.operands.get(1) {
                Some(LirOperand::Reg(r)) => {
                    a.cmp(rax, vreg_ptr(*r))?;
                }
                Some(LirOperand::ImmI64(val)) => {
                    a.mov(rcx, *val as u64)?;
                    a.cmp(rax, rcx)?;
                }
                _ => return Ok(()),
            }
        }

        LirOpcode::SetEq | LirOpcode::SetNe | LirOpcode::SetLt | LirOpcode::SetLe | LirOpcode::SetGt | LirOpcode::SetGe => {
            if let Some(d) = inst.dest {
                match inst.opcode {
                    LirOpcode::SetEq => a.sete(al)?,
                    LirOpcode::SetNe => a.setne(al)?,
                    LirOpcode::SetLt => a.setl(al)?,
                    LirOpcode::SetLe => a.setle(al)?,
                    LirOpcode::SetGt => a.setg(al)?,
                    LirOpcode::SetGe => a.setge(al)?,
                    _ => {}
                }
                a.movzx(rax, al)?;
                a.mov(vreg_ptr(d), rax)?;
            }
        }

        LirOpcode::Neg => {
            if let (Some(d), [LirOperand::Reg(src)]) = (inst.dest, &inst.operands[..]) {
                a.mov(rax, vreg_ptr(*src))?;
                a.neg(rax)?;
                a.mov(vreg_ptr(d), rax)?;
            }
        }

        LirOpcode::Not => {
            if let (Some(d), [LirOperand::Reg(src)]) = (inst.dest, &inst.operands[..]) {
                a.mov(rax, vreg_ptr(*src))?;
                a.not(rax)?;
                a.mov(vreg_ptr(d), rax)?;
            }
        }

        LirOpcode::Ret => {
            if let Some(LirOperand::Reg(r)) = inst.operands.first() {
                a.mov(rax, vreg_ptr(*r))?;
            } else if let Some(LirOperand::ImmI64(val)) = inst.operands.first() {
                a.mov(rax, *val as u64)?;
            }
            a.mov(rsp, rbp)?;
            a.pop(rbp)?;
            a.ret()?;
        }

        LirOpcode::Call => {
            let callee = match inst.operands.first() {
                Some(LirOperand::Label(l)) => l.clone(),
                _ => return Ok(()),
            };

            let conv = inst.call_conv.unwrap_or_else(native_call_conv);

            save_caller_saved(a)?;

            if conv == CallingConvention::Win64 {
                a.sub(rsp, 32)?;
            }

            let args: Vec<&LirOperand> = inst.operands.iter().skip(1).collect();
            // BUG-L07: arguments beyond the register window were silently dropped.
            let max_args = if conv == CallingConvention::Win64 { 4 } else { 6 };
            if args.len() > max_args {
                return Err(CodegenError::TooManyParams {
                    callee: callee.clone(),
                    got: args.len(),
                    max: max_args,
                });
            }

            for (i, arg) in args.into_iter().enumerate() {
                let reg = match conv {
                    CallingConvention::Win64 => match i {
                        0 => Some(rcx),
                        1 => Some(rdx),
                        2 => Some(r8),
                        3 => Some(r9),
                        _ => None,
                    },
                    _ => match i {
                        0 => Some(rdi),
                        1 => Some(rsi),
                        2 => Some(rdx),
                        3 => Some(rcx),
                        4 => Some(r8),
                        5 => Some(r9),
                        _ => None,
                    },
                };
                if let Some(r) = reg {
                    match arg {
                        LirOperand::Reg(rn) => { a.mov(r, vreg_ptr(*rn))?; }
                        LirOperand::ImmI64(val) => { a.mov(r, *val as u64)?; }
                        _ => {}
                    }
                }
            }

            if callee == *func_name {
                if let Some(label) = func_labels.get(&callee) {
                    a.call(label.clone())?;
                } else {
                    pending_relocs.push(PendingReloc {
                        target_name: callee.clone(),
                        is_relative: true,
                    });
                    emit_call_placeholder(a, pending_relocs.len())?;
                }
            } else {
                pending_relocs.push(PendingReloc {
                    target_name: callee.clone(),
                    is_relative: true,
                });
                emit_call_placeholder(a, pending_relocs.len())?;
            }

            if conv == CallingConvention::Win64 {
                a.add(rsp, 32)?;
            }

            let dest = inst.dest;
            if dest.is_some() {
                a.mov(vreg_ptr(dest.unwrap()), rax)?;
            }
            restore_caller_saved(a)?;
        }

        // BUG-K09: never silently drop opcodes. These require struct/pointer
        // layout support that the obj backend does not have yet — failing at
        // compile time beats emitting wrong machine code.
        LirOpcode::Load | LirOpcode::Store | LirOpcode::Alloca
        | LirOpcode::GetField | LirOpcode::SetField | LirOpcode::StructInit => {
            return Err(CodegenError::Unsupported(inst.opcode));
        }

        _ => return Err(CodegenError::Unsupported(inst.opcode)),
    }
    Ok(())
}

/// Emit `E8 <u32 marker>` — a call placeholder whose rel32 field carries the
/// 1-based reloc index so it can be located deterministically after assembly.
fn emit_call_placeholder(a: &mut CodeAssembler, reloc_index_1based: usize) -> Result<(), CodegenError> {
    let marker = (reloc_index_1based as u32).to_le_bytes();
    a.db(&[0xE8])?;
    a.db(&[marker[0], marker[1], marker[2], marker[3]])?;
    Ok(())
}

pub fn load_operand(a: &mut CodeAssembler, op: Option<&LirOperand>) -> Result<bool, CodegenError> {
    match op {
        Some(LirOperand::Reg(r)) => {
            a.mov(rax, vreg_ptr(*r))?;
            Ok(true)
        }
        Some(LirOperand::ImmI64(val)) => {
            a.mov(rax, *val as u64)?;
            Ok(true)
        }
        _ => Ok(false),
    }
}

pub fn load_operand_rcx(a: &mut CodeAssembler, op: Option<&LirOperand>) -> Result<bool, CodegenError> {
    match op {
        Some(LirOperand::Reg(r)) => {
            a.mov(rcx, vreg_ptr(*r))?;
            Ok(true)
        }
        Some(LirOperand::ImmI64(val)) => {
            a.mov(rcx, *val as u64)?;
            Ok(true)
        }
        _ => Ok(false),
    }
}

pub fn emit_binop(a: &mut CodeAssembler, inst: &LirInst) -> Result<(), CodegenError> {
    let d = inst.dest.ok_or(CodegenError::MissingDest(inst.opcode))?;

    if !load_operand(a, inst.operands.first())? {
        return Ok(());
    }
    if !load_operand_rcx(a, inst.operands.get(1))? {
        return Ok(());
    }

    match inst.opcode {
        LirOpcode::Add => a.add(rax, rcx)?,
        LirOpcode::Sub => a.sub(rax, rcx)?,
        LirOpcode::Mul => a.imul_2(rax, rcx)?,
        LirOpcode::And => a.and(rax, rcx)?,
        LirOpcode::Or => a.or(rax, rcx)?,
        LirOpcode::Xor => a.xor(rax, rcx)?,
        LirOpcode::Shl => {
            a.shl(rax, cl)?;
        }
        LirOpcode::Shr => {
            a.sar(rax, cl)?;
        }
        _ => {}
    }

    a.mov(vreg_ptr(d), rax)?;

    Ok(())
}

/// The default calling convention for Brak-internal functions: Win64 on
/// Windows, SystemV elsewhere. Using the HOST convention by default keeps
/// exported symbols (DLLs / shared libs) directly callable from C/FFI without
/// thunks — the old hardcoded SysV order produced garbage when a Windows
/// caller invoked an exported function via rcx/rdx.
pub fn native_call_conv() -> CallingConvention {
    if cfg!(target_os = "windows") { CallingConvention::Win64 } else { CallingConvention::SystemV }
}

