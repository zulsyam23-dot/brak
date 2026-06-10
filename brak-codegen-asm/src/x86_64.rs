use brak_ir_lir::lir::{LirFunction, LirInst, LirOpcode, LirOperand};

use crate::regalloc;

pub fn emit_function(func: &LirFunction, alloc: &mut regalloc::SimpleAlloc) -> String {
    let mut out = String::new();
    out.push_str(&format!("{}:\n", func.name));

    let frame_size = alloc.frame_size();
    out.push_str("  push rbp\n");
    if frame_size > 0 {
        out.push_str(&format!("  sub rsp, {frame_size}\n"));
    }

    // Brak convention: params passed in (rax, rcx, rdx, rbx, rsi, rdi, r8, r9)
    for param in &func.params {
        let p = alloc.map(*param);
        let name = regalloc::virt_to_name(p);
        let offset = frame_size as i64 - ((*param + 1) * 8) as i64;
        let offset_str = if offset >= 0 {
            format!("{offset}")
        } else {
            format!("{}", offset)
        };
        out.push_str(&format!("  mov [rsp+{offset_str}], {name}\n"));
    }

    for block in &func.blocks {
        out.push_str(&format!("block_{}:\n", block.id));
        for inst in &block.insts {
            out.push_str(&format!("  {}\n", emit_inst(inst, alloc)));
        }
    }

    if frame_size > 0 {
        out.push_str(&format!("  add rsp, {frame_size}\n"));
    }
    out.push_str("  pop rbp\n");
    out.push_str("  ret\n\n");

    out
}

fn emit_inst(inst: &LirInst, alloc: &mut regalloc::SimpleAlloc) -> String {
    let (mnemonic, _suffix) = match inst.opcode {
        LirOpcode::Mov => ("mov", ""),
        LirOpcode::Add => ("add", ""),
        LirOpcode::Sub => ("sub", ""),
        LirOpcode::Mul => ("imul", ""),
        LirOpcode::Div => ("idiv", ""),
        LirOpcode::Mod => ("idiv", ""),
        LirOpcode::Neg => ("neg", ""),
        LirOpcode::Not => ("not", ""),
        LirOpcode::And => ("and", ""),
        LirOpcode::Or => ("or", ""),
        LirOpcode::Xor => ("xor", ""),
        LirOpcode::Shl => ("shl", ""),
        LirOpcode::Shr => ("shr", ""),
        LirOpcode::Cmp => ("cmp", ""),
        LirOpcode::Jmp => ("jmp", ""),
        LirOpcode::Br => ("test", ""),
        LirOpcode::Ret => ("ret", ""),
        LirOpcode::Call => ("call", ""),
        LirOpcode::Push => ("push", ""),
        LirOpcode::Pop => ("pop", ""),
        LirOpcode::Comment => ("#", ""),
        _ => ("nop", ""),
    };

    match inst.opcode {
        LirOpcode::Ret => {
            if let Some(op) = inst.operands.first() {
                let val = format_op(op, alloc);
                format!("mov rax, {val}\n  ret")
            } else {
                "ret".to_string()
            }
        }
        LirOpcode::Comment => {
            let s = inst.operands.first()
                .map(|o| match o {
                    LirOperand::Label(l) => l.clone(),
                    _ => format!("{o}"),
                })
                .unwrap_or_default();
            format!("# {s}")
        }
        LirOpcode::Jmp => {
            let target = inst.operands.first()
                .map(|o| match o {
                    LirOperand::Label(l) => l.clone(),
                    _ => format!("{o}"),
                })
                .unwrap_or_default();
            format!("jmp {target}")
        }
        LirOpcode::Br => {
            let cond = inst.operands.first()
                .map(|o| format_reg(o, alloc))
                .unwrap_or_default();
            let then_label = inst.operands.get(1)
                .map(|o| match o {
                    LirOperand::Label(l) => l.clone(),
                    _ => format!("{o}"),
                })
                .unwrap_or_default();
            let else_label = inst.operands.get(2)
                .map(|o| match o {
                    LirOperand::Label(l) => l.clone(),
                    _ => format!("{o}"),
                })
                .unwrap_or_default();
            format!("test {cond}, {cond}\n  jnz {then_label}\n  jmp {else_label}")
        }
        LirOpcode::Mod => {
            let dest = inst.dest
                .map(|d| regalloc::virt_to_name(alloc.map(d)))
                .unwrap_or_default();
            let lhs = inst.operands.first()
                .map(|o| format_op(o, alloc))
                .unwrap_or_default();
            let rhs = inst.operands.get(1)
                .map(|o| format_op(o, alloc))
                .unwrap_or_default();
            format!("push rax\n  push rdx\n  mov rax, {lhs}\n  cqo\n  idiv {rhs}\n  mov {dest}, rdx\n  pop rdx\n  pop rax")
        }
        LirOpcode::Call => {
            let callee = inst.operands.first()
                .map(|o| match o {
                    LirOperand::Label(l) => l.clone(),
                    _ => format!("{o}"),
                })
                .unwrap_or_default();

            let dest_is_rax = inst.dest.map_or(false, |d| alloc.map(d) == 0);

            // Brak convention: move each arg to the corresponding param register
            let mut setup = String::new();
            if !dest_is_rax {
                setup.push_str("  push rax\n");
            }
            for (i, arg_op) in inst.operands.iter().skip(1).enumerate() {
                if i >= regalloc::PHYS_REGS.len() {
                    break;
                }
                let arg = format_op(arg_op, alloc);
                setup.push_str(&format!("  mov {}, {}\n", regalloc::PHYS_REGS[i], arg));
            }
            // Save return value after call
            let ret_line = if let Some(d) = inst.dest {
                let dname = regalloc::virt_to_name(alloc.map(d));
                format!("  mov {}, rax\n", dname)
            } else {
                String::new()
            };
            let teardown = if !dest_is_rax {
                "  pop rax\n"
            } else {
                ""
            };
            format!("{}  call {callee}\n{}{}",
                setup, ret_line, teardown)
        }
        LirOpcode::Cmp => {
            let lhs = inst.operands.first()
                .map(|o| format_reg(o, alloc))
                .unwrap_or_default();
            let rhs = inst.operands.get(1)
                .map(|o| format_op(o, alloc))
                .unwrap_or_default();
            format!("cmp {lhs}, {rhs}")
        }
        LirOpcode::SetEq | LirOpcode::SetNe | LirOpcode::SetLt
        | LirOpcode::SetLe | LirOpcode::SetGt | LirOpcode::SetGe => {
            let dest = inst.dest
                .map(|d| regalloc::virt_to_name(alloc.map(d)))
                .unwrap_or_default();
            let dest8 = reg_to_8bit(dest);
            let cond_code = match inst.opcode {
                LirOpcode::SetEq => "sete",
                LirOpcode::SetNe => "setne",
                LirOpcode::SetLt => "setl",
                LirOpcode::SetLe => "setle",
                LirOpcode::SetGt => "setg",
                LirOpcode::SetGe => "setge",
                _ => unreachable!(),
            };
            format!("{cond_code} {dest8}\n  movzx {dest}, {dest8}")
        }
        _ => {
            if let Some(dest) = inst.dest {
                let d = regalloc::virt_to_name(alloc.map(dest));
                if inst.operands.is_empty() {
                    format!("{mnemonic} {d}")
                } else if inst.operands.len() == 1 {
                    let op = format_op(&inst.operands[0], alloc);
                    format!("{mnemonic} {d}, {op}")
                } else {
                    let lhs = format_reg(&inst.operands[0], alloc);
                    let rhs = format_op(&inst.operands[1], alloc);
                    if d == lhs {
                        format!("{mnemonic} {d}, {rhs}")
                    } else {
                        format!("mov {d}, {lhs}\n  {mnemonic} {d}, {rhs}")
                    }
                }
            } else if inst.operands.is_empty() {
                mnemonic.to_string()
            } else if inst.operands.len() == 1 {
                let op = format_op(&inst.operands[0], alloc);
                format!("{mnemonic} {op}")
            } else {
                let lhs = format_op(&inst.operands[0], alloc);
                let rhs = format_op(&inst.operands[1], alloc);
                format!("{mnemonic} {lhs}, {rhs}")
            }
        }
    }
}

fn format_op(op: &LirOperand, alloc: &mut regalloc::SimpleAlloc) -> String {
    match op {
        LirOperand::Reg(r) => {
            let p = alloc.map(*r);
            regalloc::virt_to_name(p).to_string()
        }
        LirOperand::ImmI64(i) => format!("{i}"),
        LirOperand::ImmF64(f) => format!("{f}"),
        LirOperand::Label(l) => l.clone(),
        LirOperand::StackSlot(s) => {
            let offset = alloc.frame_size() as i64 - ((*s + 1) * 8) as i64;
            if offset >= 0 {
                format!("qword [rsp+{offset}]")
            } else {
                format!("qword [rsp{offset}]")
            }
        }
        LirOperand::StringRef(i) => format!("str_{i}"),
        LirOperand::Field(s) => s.clone(),
    }
}

fn format_reg(op: &LirOperand, alloc: &mut regalloc::SimpleAlloc) -> String {
    match op {
        LirOperand::Reg(r) => {
            let p = alloc.map(*r);
            regalloc::virt_to_name(p).to_string()
        }
        other => format_op(other, alloc),
    }
}

fn reg_to_8bit(name: &str) -> &str {
    match name {
        "rax" => "al",
        "rbx" => "bl",
        "rcx" => "cl",
        "rdx" => "dl",
        "rsi" => "sil",
        "rdi" => "dil",
        "rsp" => "spl",
        "rbp" => "bpl",
        "r8" => "r8b",
        "r9" => "r9b",
        "r10" => "r10b",
        "r11" => "r11b",
        "r12" => "r12b",
        "r13" => "r13b",
        "r14" => "r14b",
        "r15" => "r15b",
        other => other,
    }
}
