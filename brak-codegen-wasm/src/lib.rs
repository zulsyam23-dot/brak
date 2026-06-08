use std::collections::HashSet;
use brak_codegen_traits::CodegenBackend;
use brak_core::Result;
use brak_ir_lir::lir::{
    LirOpcode, LirOperand, LirProgram, LirFunction, LirInst, VirtReg,
};

pub struct WasmBackend;

impl CodegenBackend for WasmBackend {
    fn name(&self) -> &'static str {
        "wasm"
    }
    fn emit(&self, program: &LirProgram) -> Result<Vec<u8>> {
        Ok(emit_wasm(program).into_bytes())
    }
}

pub fn emit_wasm(program: &LirProgram) -> String {
    let mut w = WasmWriter::new(program);
    w.emit_module();
    w.finish()
}

struct WasmWriter<'a> {
    output: String,
    indent: usize,
    program: &'a LirProgram,
    float_regs: HashSet<VirtReg>,
    last_cmp_lhs: Option<VirtReg>,
    last_cmp_rhs: Option<VirtReg>,
}

impl<'a> WasmWriter<'a> {
    fn new(program: &'a LirProgram) -> Self {
        Self {
            output: String::new(),
            indent: 0,
            program,
            float_regs: HashSet::new(),
            last_cmp_lhs: None,
            last_cmp_rhs: None,
        }
    }

    fn finish(self) -> String { self.output }

    fn emit_module(&mut self) {
        self.emit_line("(module");
        self.indent += 1;

        // Memory for string table
        self.emit_data_segments();
        if !self.program.string_table.is_empty() {
            self.emit_line("(memory (export \"memory\") 1)");
        }

        // External function imports
        for ext in &self.program.extern_functions {
            self.emit_line(&format!(
                "(import \"env\" \"{}\" (func ${} (param i64 i64 i64 i64 i64 i64) (result i64)))",
                ext.name, ext.name
            ));
        }

        // Functions
        for func in &self.program.functions {
            self.analyze_function(func);
            self.emit_function(func);
        }

        self.indent -= 1;
        self.emit_line(")");
    }

    fn emit_data_segments(&mut self) {
        if self.program.string_table.is_empty() {
            return;
        }
        self.emit_line("(data (i32.const 0) \"brak_strings\")");
        let mut offset: u32 = 0;
        let segs: Vec<(u32, String)> = self.program.string_table.iter().map(|s| {
            let start = offset;
            let bytes = s.as_bytes();
            offset += bytes.len() as u32 + 1;
            let escaped = s
                .replace('\\', "\\\\")
                .replace('"', "\\\"")
                .replace('\n', "\\n")
                .replace('\r', "\\r")
                .replace('\t', "\\t");
            (start, format!("{}\0", escaped))
        }).collect();
        for (offset, content) in segs {
            self.emit_line(&format!("(data (i32.const {}) \"{}\")", offset + 12, content));
        }
    }

    fn analyze_function(&mut self, func: &LirFunction) {
        self.float_regs.clear();
        for block in &func.blocks {
            for inst in &block.insts {
                for op in &inst.operands {
                    if let LirOperand::ImmF64(_) = op {
                        if let Some(d) = inst.dest { self.float_regs.insert(d); }
                    }
                }
            }
        }
        for param in &func.params { self.float_regs.remove(param); }
    }

    fn emit_function(&mut self, func: &LirFunction) {
        let mut regs_used: HashSet<VirtReg> = HashSet::new();
        let mut max_reg = 0usize;
        for block in &func.blocks {
            for inst in &block.insts {
                if let Some(d) = inst.dest {
                    regs_used.insert(d);
                    max_reg = max_reg.max(d);
                }
                for op in &inst.operands {
                    if let LirOperand::Reg(r) = op {
                        regs_used.insert(*r);
                        max_reg = max_reg.max(*r);
                    }
                }
            }
        }

        let param_locals: Vec<String> = (0..func.params.len())
            .map(|i| {
                let ty = if self.float_regs.contains(&func.params[i]) { "f64" } else { "i64" };
                format!("(param ${} {})", self.reg_name(func.params[i]), ty)
            })
            .collect();
        let extra_locals: Vec<String> = (0..=max_reg)
            .filter(|r| !func.params.contains(r) && regs_used.contains(r))
            .map(|r| {
                let ty = if self.float_regs.contains(&r) { "f64" } else { "i64" };
                format!("(local ${} {})", self.reg_name(r), ty)
            })
            .collect();

        let params = param_locals.join(" ");

        self.emit_line(&format!(
            "(func ${} {} (result i64)",
            func.name, params
        ));
        self.indent += 1;

        if !extra_locals.is_empty() {
            for l in &extra_locals {
                self.emit_line(l);
            }
        }

        // Dispatch loop
        self.emit_line("(local $__block i32)");
        self.emit_line("i32.const 0");
        self.emit_line("local.set $__block");
        self.emit_line("block $__done");
        self.indent += 1;
        self.emit_line("loop $__dispatch");
        self.indent += 1;
        self.emit_line("local.get $__block");

        // Each block
        for block in &func.blocks {
            self.emit_line(&format!("i32.const {}", block.id));
            self.emit_line("i32.eq");
            self.emit_line("if");
            self.indent += 1;

            self.last_cmp_lhs = None;
            self.last_cmp_rhs = None;
            for inst in &block.insts {
                self.emit_inst(inst, func);
            }

            self.indent -= 1;
            self.emit_line("end");
        }

        // Fallback: exit
        self.emit_line("br $__done");
        self.indent -= 1;
        self.emit_line("end");
        self.indent -= 1;
        self.emit_line("end");

        self.indent -= 1;
        self.emit_line(")");
    }

    fn emit_inst(&mut self, inst: &LirInst, func: &LirFunction) {
        match inst.opcode {
            LirOpcode::Cmp => {
                self.last_cmp_lhs = self.operand_reg(0, inst);
                self.last_cmp_rhs = self.operand_reg(1, inst);
            }
            LirOpcode::SetEq => self.emit_cmp("i64.eq", inst),
            LirOpcode::SetNe => self.emit_cmp("i64.ne", inst),
            LirOpcode::SetLt => self.emit_cmp("i64.lt_s", inst),
            LirOpcode::SetLe => self.emit_cmp("i64.le_s", inst),
            LirOpcode::SetGt => self.emit_cmp("i64.gt_s", inst),
            LirOpcode::SetGe => self.emit_cmp("i64.ge_s", inst),
            LirOpcode::Mov => self.emit_mov(inst),
            LirOpcode::Add => self.emit_binop("i64.add", inst),
            LirOpcode::Sub => self.emit_binop("i64.sub", inst),
            LirOpcode::Mul => self.emit_binop("i64.mul", inst),
            LirOpcode::Div => self.emit_binop("i64.div_s", inst),
            LirOpcode::Mod => self.emit_binop("i64.rem_s", inst),
            LirOpcode::Neg => self.emit_unop("i64.sub", inst),
            LirOpcode::Not => self.emit_unop("i64.eqz", inst),
            LirOpcode::And => self.emit_binop("i64.and", inst),
            LirOpcode::Or => self.emit_binop("i64.or", inst),
            LirOpcode::Xor => self.emit_binop("i64.xor", inst),
            LirOpcode::Shl => self.emit_binop("i64.shl", inst),
            LirOpcode::Shr => self.emit_binop("i64.shr_s", inst),
            LirOpcode::Load => self.emit_load(inst),
            LirOpcode::Store => self.emit_store(inst),
            LirOpcode::Alloca => self.emit_alloca(inst),
            LirOpcode::Call => self.emit_call(inst, func),
            LirOpcode::Ret => self.emit_ret(inst),
            LirOpcode::Jmp => self.emit_jmp(inst),
            LirOpcode::Br => self.emit_br(inst),
            LirOpcode::Push | LirOpcode::Pop | LirOpcode::Comment => {}
        }
    }

    fn emit_cmp(&mut self, wasm_op: &str, inst: &LirInst) {
        if let Some(lhs) = self.last_cmp_lhs {
            self.push_reg(lhs);
        } else { self.emit_line("i64.const 0"); }
        if let Some(rhs) = self.last_cmp_rhs {
            self.push_reg(rhs);
        } else { self.emit_line("i64.const 0"); }
        self.emit_line(wasm_op);
        if let Some(d) = inst.dest {
            self.emit_line(&format!("local.set ${}", self.reg_name(d)));
        }
    }

    fn emit_mov(&mut self, inst: &LirInst) {
        let dest = inst.dest;
        let op = &inst.operands[0];
        match op {
            LirOperand::Reg(r) => {
                self.push_reg(*r);
                if let Some(d) = dest {
                    self.emit_line(&format!("local.set ${}", self.reg_name(d)));
                }
            }
            LirOperand::ImmI64(i) => {
                self.emit_line(&format!("i64.const {}", i));
                if let Some(d) = dest {
                    self.emit_line(&format!("local.set ${}", self.reg_name(d)));
                }
            }
            LirOperand::ImmF64(f) => {
                let val_str = if f.is_nan() { "nan:0x1".to_string() } else { format!("{:.20}", f) };
                self.emit_line("i64.reinterpret_f64");
                self.emit_line(&format!("f64.const {}", val_str));
                if let Some(d) = dest {
                    self.emit_line(&format!("local.set ${}", self.reg_name(d)));
                }
            }
            LirOperand::StringRef(idx) => {
                let offset = self.string_offset(*idx);
                self.emit_line(&format!("i32.const {}", offset + 12));
                self.emit_line("i64.extend_i32_u");
                if let Some(d) = dest {
                    self.emit_line(&format!("local.set ${}", self.reg_name(d)));
                }
            }
            _ => {
                let s = self.operand_str(op);
                self.emit_line(&format!("i64.const 0 ;; unhandled mov from {}", s));
                if let Some(d) = dest {
                    self.emit_line(&format!("local.set ${}", self.reg_name(d)));
                }
            }
        }
    }

    fn emit_binop(&mut self, wasm_op: &str, inst: &LirInst) {
        self.push_operand(0, inst);
        self.push_operand(1, inst);
        self.emit_line(wasm_op);
        if let Some(d) = inst.dest {
            self.emit_line(&format!("local.set ${}", self.reg_name(d)));
        }
    }

    fn emit_unop(&mut self, wasm_op: &str, inst: &LirInst) {
        if wasm_op == "i64.sub" {
            // Neg: 0 - val
            self.emit_line("i64.const 0");
            self.push_operand(0, inst);
            self.emit_line("i64.sub");
        } else if wasm_op == "i64.eqz" {
            // Not: val == 0 ? 1 : 0 → eqz
            self.push_operand(0, inst);
            self.emit_line("i64.eqz");
        }
        if let Some(d) = inst.dest {
            self.emit_line(&format!("local.set ${}", self.reg_name(d)));
        }
    }

    fn emit_load(&mut self, inst: &LirInst) {
        self.push_operand(0, inst);
        let is_float = inst.dest.map(|d| self.float_regs.contains(&d)).unwrap_or(false);
        if is_float {
            self.emit_line("f64.load");
        } else {
            self.emit_line("i64.load");
        }
        if let Some(d) = inst.dest {
            self.emit_line(&format!("local.set ${}", self.reg_name(d)));
        }
    }

    fn emit_store(&mut self, inst: &LirInst) {
        self.push_operand(0, inst);
        self.push_operand(1, inst);
        self.emit_line("i64.store");
    }

    fn emit_alloca(&mut self, inst: &LirInst) {
        // Use memory.grow scaled
        self.push_operand(0, inst);
        self.emit_line("memory.grow");
        self.emit_line("i64.extend_i32_u");
        self.emit_line("i32.const 65536");
        self.emit_line("i64.extend_i32_u");
        self.emit_line("i64.mul");
        if let Some(d) = inst.dest {
            self.emit_line(&format!("local.set ${}", self.reg_name(d)));
        }
    }

    fn emit_call(&mut self, inst: &LirInst, _func: &LirFunction) {
        let callee = match inst.operands.first() {
            Some(LirOperand::Label(name)) => name.clone(),
            _ => return,
        };

        // Push args (skip the label operand)
        for op in &inst.operands[1..] {
            match op {
                LirOperand::Reg(r) => self.push_reg(*r),
                LirOperand::ImmI64(i) => self.emit_line(&format!("i64.const {}", i)),
                _ => self.emit_line("i64.const 0"),
            }
        }
        // Pad args to 6 for extern functions
        let is_extern = self.program.extern_functions.iter().any(|e| e.name == callee);
        let arg_count = inst.operands.len().saturating_sub(1);
        if is_extern {
            for _ in arg_count..6 {
                self.emit_line("i64.const 0");
            }
        }

        self.emit_line(&format!("call ${}", callee));
        if let Some(d) = inst.dest {
            self.emit_line(&format!("local.set ${}", self.reg_name(d)));
        }
    }

    fn emit_ret(&mut self, inst: &LirInst) {
        if let Some(op) = inst.operands.first() {
            match op {
                LirOperand::Reg(r) => self.push_reg(*r),
                _ => {
                    let s = self.operand_str(op);
                    // parse or fallback
                    self.emit_line(&format!("i64.const 0 ;; return {}", s));
                }
            }
        } else {
            self.emit_line("i64.const 0");
        }
        self.emit_line("br $__done");
    }

    fn emit_jmp(&mut self, inst: &LirInst) {
        if let Some(LirOperand::Label(name)) = inst.operands.first() {
            let id = name.strip_prefix("block_").and_then(|s| s.parse::<u32>().ok()).unwrap_or(0);
            self.emit_line(&format!("i32.const {}", id));
            self.emit_line("local.set $__block");
            self.emit_line("br $__dispatch");
        }
    }

    fn emit_br(&mut self, inst: &LirInst) {
        if inst.operands.len() < 3 { return; }
        let cond_reg = match &inst.operands[0] { LirOperand::Reg(r) => *r, _ => return };
        let label_t = match &inst.operands[1] { LirOperand::Label(n) => n.strip_prefix("block_").and_then(|s| s.parse::<u32>().ok()).unwrap_or(0), _ => return };
        let label_f = match &inst.operands[2] { LirOperand::Label(n) => n.strip_prefix("block_").and_then(|s| s.parse::<u32>().ok()).unwrap_or(0), _ => return };

        // if cond: goto label_t, else goto label_f
        self.push_reg(cond_reg);
        self.emit_line("i64.eqz");
        self.emit_line("if");
        self.indent += 1;
        self.emit_line(&format!("i32.const {}", label_f));
        self.emit_line("local.set $__block");
        self.emit_line("br $__dispatch");
        self.indent -= 1;
        self.emit_line("else");
        self.indent += 1;
        self.emit_line(&format!("i32.const {}", label_t));
        self.emit_line("local.set $__block");
        self.emit_line("br $__dispatch");
        self.indent -= 1;
        self.emit_line("end");
    }

    fn push_reg(&mut self, reg: VirtReg) {
        self.emit_line(&format!("local.get ${}", self.reg_name(reg)));
    }

    fn push_operand(&mut self, idx: usize, inst: &LirInst) {
        if let Some(op) = inst.operands.get(idx) {
            match op {
                LirOperand::Reg(r) => self.push_reg(*r),
                LirOperand::ImmI64(i) => self.emit_line(&format!("i64.const {}", i)),
                LirOperand::ImmF64(f) => {
                    if f.is_nan() { self.emit_line("f64.const nan:0x1"); }
                    else if f.is_infinite() && *f > 0.0 { self.emit_line("f64.const inf"); }
                    else if f.is_infinite() { self.emit_line("f64.const -inf"); }
                    else { self.emit_line(&format!("f64.const {:.20}", f)); }
                }
                _ => self.emit_line("i64.const 0"),
            }
        }
    }

    fn operand_str(&self, op: &LirOperand) -> String {
        match op {
            LirOperand::Reg(r) => format!("r{}", r),
            LirOperand::ImmI64(i) => i.to_string(),
            LirOperand::ImmF64(f) => f.to_string(),
            LirOperand::Label(n) => n.clone(),
            LirOperand::StackSlot(s) => format!("slot{}", s),
            LirOperand::StringRef(i) => format!("str{}", i),
        }
    }

    fn operand_reg(&self, idx: usize, inst: &LirInst) -> Option<VirtReg> {
        inst.operands.get(idx).and_then(|op| {
            if let LirOperand::Reg(r) = op { Some(*r) } else { None }
        })
    }

    fn reg_name(&self, reg: VirtReg) -> String {
        format!("r{}", reg)
    }

    fn string_offset(&self, idx: usize) -> u32 {
        let mut offset: u32 = 12; // header
        for i in 0..idx.min(self.program.string_table.len()) {
            offset += self.program.string_table[i].len() as u32 + 1;
        }
        offset
    }

    fn emit_line(&mut self, line: &str) {
        for _ in 0..self.indent { self.output.push_str("  "); }
        self.output.push_str(line);
        self.output.push('\n');
    }
}
