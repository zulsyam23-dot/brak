use std::collections::HashSet;
use brak_codegen_traits::CodegenBackend;
use brak_core::Result;
use brak_ir_lir::lir::{
    LirOpcode, LirOperand, LirProgram, LirFunction, LirInst, VirtReg,
};

pub struct LlvmBackend;

impl CodegenBackend for LlvmBackend {
    fn name(&self) -> &'static str {
        "llvm"
    }
    fn emit(&self, program: &LirProgram) -> Result<Vec<u8>> {
        Ok(emit_llvm(program).into_bytes())
    }
}

pub fn emit_llvm(program: &LirProgram) -> String {
    let mut w = LlvmWriter::new(program);
    w.emit_module();
    w.finish()
}

struct LlvmWriter<'a> {
    output: String,
    program: &'a LirProgram,
    float_regs: HashSet<VirtReg>,
    last_cmp_lhs: Option<VirtReg>,
    last_cmp_rhs: Option<VirtReg>,
    next_local: usize,
}

impl<'a> LlvmWriter<'a> {
    fn new(program: &'a LirProgram) -> Self {
        Self {
            output: String::new(),
            program,
            float_regs: HashSet::new(),
            last_cmp_lhs: None,
            last_cmp_rhs: None,
            next_local: 0,
        }
    }

    fn finish(self) -> String { self.output }

    fn fresh(&mut self) -> usize {
        let id = self.next_local;
        self.next_local += 1;
        id
    }

    fn emit(&mut self, s: &str) { self.output.push_str(s); }

    fn emit_module(&mut self) {
        self.emit("target datalayout = \"e-m:e-p:64:64:64-i64:64-f64:64:64\"\n");
        self.emit("target triple = \"x86_64-unknown-unknown\"\n\n");

        for (i, s) in self.program.string_table.iter().enumerate() {
            let escaped = s
                .replace('\\', "\\5C")
                .replace('"', "\\22")
                .replace('\n', "\\0A")
                .replace('\r', "\\0D")
                .replace('\t', "\\09");
            self.emit(&format!("@_s{} = private unnamed_addr constant [{} x i8] c\"{}\\00\"\n", i, s.len() + 1, escaped));
        }
        if !self.program.string_table.is_empty() {
            self.emit("\n");
        }

        for ext in &self.program.extern_functions {
            self.emit(&format!("declare i64 @{}(i64, i64, i64, i64, i64, i64)\n", ext.name));
        }
        if !self.program.extern_functions.is_empty() {
            self.emit("\n");
        }

        for func in &self.program.functions {
            self.analyze_function(func);
            self.emit_function(func);
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
        self.next_local = 0;

        let param_str: Vec<String> = func.params.iter()
            .map(|p| {
                let ty = if self.float_regs.contains(p) { "double" } else { "i64" };
                format!("{} %r{}", ty, p)
            })
            .collect();
        let params = param_str.join(", ");
        self.emit(&format!("define i64 @{}({}) {{\n", func.name, params));
        self.emit("entry:\n");

        let mut regs_used: HashSet<VirtReg> = HashSet::new();
        for block in &func.blocks {
            for inst in &block.insts {
                if let Some(d) = inst.dest { regs_used.insert(d); }
                for op in &inst.operands {
                    if let LirOperand::Reg(r) = op { regs_used.insert(*r); }
                }
            }
        }

        for r in &regs_used {
            if !func.params.contains(r) {
                let ty = if self.float_regs.contains(r) { "double" } else { "i64" };
                self.emit(&format!("  %r{} = alloca {}, align 8\n", r, ty));
            }
        }

        for p in &func.params {
            let ty = if self.float_regs.contains(p) { "double" } else { "i64" };
            self.emit(&format!("  store {} %r{}, {}* %r{}\n", ty, p, ty, p));
        }

        let mut block_ids: Vec<String> = Vec::new();
        for block in &func.blocks {
            block_ids.push(format!("block_{}", block.id));
        }

        for (bi, block) in func.blocks.iter().enumerate() {
            let is_last = bi == func.blocks.len() - 1;
            let id = &block_ids[bi];

            if bi > 0 {
                self.emit(&format!("{}:\n", id));
            }

            self.last_cmp_lhs = None;
            self.last_cmp_rhs = None;

            for inst in &block.insts {
                self.emit_inst(inst);
            }

            if !is_last && block.insts.last().map(|i| matches!(i.opcode, LirOpcode::Jmp | LirOpcode::Br | LirOpcode::Ret)).unwrap_or(false) == false {
                if bi + 1 < block_ids.len() {
                    self.emit(&format!("  br label %{}\n", block_ids[bi + 1]));
                }
            }
        }

        self.emit("  unreachable\n");
        self.emit("}\n\n");
    }

    fn emit_inst(&mut self, inst: &LirInst) {
        match inst.opcode {
            LirOpcode::Cmp => {
                self.last_cmp_lhs = self.op_reg(0, inst);
                self.last_cmp_rhs = self.op_reg(1, inst);
            }
            LirOpcode::SetEq => self.emit_icmp("eq", inst),
            LirOpcode::SetNe => self.emit_icmp("ne", inst),
            LirOpcode::SetLt => self.emit_icmp("slt", inst),
            LirOpcode::SetLe => self.emit_icmp("sle", inst),
            LirOpcode::SetGt => self.emit_icmp("sgt", inst),
            LirOpcode::SetGe => self.emit_icmp("sge", inst),
            LirOpcode::Mov => {
                let dest = inst.dest;
                let op = &inst.operands[0];
                match op {
                    LirOperand::Reg(r) => {
                        let t1 = self.fresh();
                        let ty = self.reg_type(*r);
                        self.emit(&format!("  %_t{} = load {}, i64* %r{}\n", t1, ty, r));
                        if let Some(d) = dest {
                            let dty = self.reg_type(d);
                            self.emit(&format!("  store {} %_t{}, {}* %r{}\n", dty, t1, dty, d));
                        }
                    }
                    LirOperand::ImmI64(i) => {
                        if let Some(d) = dest {
                            let dty = self.reg_type(d);
                            self.emit(&format!("  store {} {}, {}* %r{}\n", dty, i, dty, d));
                        }
                    }
                    LirOperand::ImmF64(f) => {
                        let bits = f.to_bits();
                        let hex = format!("0x{:016x}", bits);
                        if let Some(d) = dest {
                            self.emit(&format!("  store i64 {}, i64* %r{}\n", hex, d));
                        }
                    }
                    LirOperand::StringRef(idx) => {
                        if let Some(d) = dest {
                            self.emit(&format!("  store i64 ptrtoint ([{} x i8]* @_s{} to i64), i64* %r{}\n",
                                self.program.string_table[*idx].len() + 1, idx, d));
                        }
                    }
                    _ => {}
                }
            }
            LirOpcode::Add => self.emit_arith("add", inst),
            LirOpcode::Sub => self.emit_arith("sub", inst),
            LirOpcode::Mul => self.emit_arith("mul", inst),
            LirOpcode::Div => self.emit_arith("sdiv", inst),
            LirOpcode::Mod => self.emit_arith("srem", inst),
            LirOpcode::Neg => {
                if let Some(d) = inst.dest {
                    if let Some(s) = self.op_reg(0, inst) {
                        let dty = self.reg_type(d);
                        let t1 = self.fresh();
                        self.emit(&format!("  %_t{} = load {}, i64* %r{}\n", t1, dty, s));
                        let t2 = self.fresh();
                        self.emit(&format!("  %_t{} = sub {} 0, %_t{}\n", t2, dty, t1));
                        self.emit(&format!("  store {} %_t{}, {}* %r{}\n", dty, t2, dty, d));
                    }
                }
            }
            LirOpcode::Not => {
                if let Some(d) = inst.dest {
                    if let Some(s) = self.op_reg(0, inst) {
                        let t1 = self.fresh();
                        self.emit(&format!("  %_t{} = load i64, i64* %r{}\n", t1, s));
                        let t2 = self.fresh();
                        self.emit(&format!("  %_t{} = icmp eq i64 %_t{}, 0\n", t2, t1));
                        let t3 = self.fresh();
                        self.emit(&format!("  %_t{} = zext i1 %_t{} to i64\n", t3, t2));
                        self.emit(&format!("  store i64 %_t{}, i64* %r{}\n", t3, d));
                    }
                }
            }
            LirOpcode::And => self.emit_arith("and", inst),
            LirOpcode::Or => self.emit_arith("or", inst),
            LirOpcode::Xor => self.emit_arith("xor", inst),
            LirOpcode::Shl => self.emit_arith("shl", inst),
            LirOpcode::Shr => self.emit_arith("ashr", inst),
            LirOpcode::Load => {
                if let (Some(d), Some(a)) = (inst.dest, self.op_reg(0, inst)) {
                    let t1 = self.fresh();
                    self.emit(&format!("  %_t{} = load i64, i64* %r{}\n", t1, a));
                    let dty = self.reg_type(d);
                    self.emit(&format!("  store {} %_t{}, {}* %r{}\n", dty, t1, dty, d));
                }
            }
            LirOpcode::Store => {
                if let (Some(a), Some(v)) = (self.op_reg(0, inst), self.op_reg(1, inst)) {
                    let t1 = self.fresh();
                    self.emit(&format!("  %_t{} = load i64, i64* %r{}\n", t1, v));
                    self.emit(&format!("  store i64 %_t{}, i64* %r{}\n", t1, a));
                }
            }
            LirOpcode::Alloca => {
                if let Some(d) = inst.dest {
                    let size = inst.operands.get(0).map(|op| match op {
                        LirOperand::ImmI64(i) => *i,
                        _ => 0,
                    }).unwrap_or(0);
                    let t1 = self.fresh();
                    self.emit(&format!("  %_t{} = alloca i8, i64 {}\n", t1, size));
                    let t2 = self.fresh();
                    self.emit(&format!("  %_t{} = ptrtoint i8* %_t{} to i64\n", t2, t1));
                    self.emit(&format!("  store i64 %_t{}, i64* %r{}\n", t2, d));
                }
            }
            LirOpcode::Call => self.emit_call(inst),
            LirOpcode::Ret => {
                if let Some(op) = inst.operands.first() {
                    match op {
                        LirOperand::Reg(r) => {
                            let t1 = self.fresh();
                            let ty = self.reg_type(*r);
                            self.emit(&format!("  %_t{} = load {}, i64* %r{}\n", t1, ty, r));
                            self.emit(&format!("  ret {} %_t{}\n", ty, t1));
                        }
                        LirOperand::ImmI64(i) => { self.emit(&format!("  ret i64 {}\n", i)); }
                        _ => { self.emit("  ret i64 0\n"); }
                    }
                } else {
                    self.emit("  ret i64 0\n");
                }
            }
            LirOpcode::Jmp => {
                if let Some(LirOperand::Label(n)) = inst.operands.first() {
                    self.emit(&format!("  br label %{}\n", n));
                }
            }
            LirOpcode::Br => {
                if inst.operands.len() < 3 { return; }
                let label_t = match &inst.operands[1] { LirOperand::Label(n) => n.clone(), _ => return };
                let label_f = match &inst.operands[2] { LirOperand::Label(n) => n.clone(), _ => return };
                if let Some(c) = self.op_reg(0, inst) {
                    let t1 = self.fresh();
                    self.emit(&format!("  %_t{} = load i64, i64* %r{}\n", t1, c));
                    let t2 = self.fresh();
                    self.emit(&format!("  %_t{} = icmp ne i64 %_t{}, 0\n", t2, t1));
                    self.emit(&format!("  br i1 %_t{}, label %{}, label %{}\n", t2, label_t, label_f));
                }
            }
            LirOpcode::Push | LirOpcode::Pop | LirOpcode::Comment => {}
        }
    }

    fn emit_icmp(&mut self, cond: &str, inst: &LirInst) {
        if let Some(d) = inst.dest {
            if let (Some(l), Some(r)) = (self.last_cmp_lhs, self.last_cmp_rhs) {
                let lty = self.reg_type(l);
                let t1 = self.fresh();
                self.emit(&format!("  %_t{} = load {}, i64* %r{}\n", t1, lty, l));
                let t2 = self.fresh();
                self.emit(&format!("  %_t{} = load {}, i64* %r{}\n", t2, self.reg_type(r), r));
                let t3 = self.fresh();
                self.emit(&format!("  %_t{} = icmp {} i64 %_t{}, %_t{}\n", t3, cond, t1, t2));
                let t4 = self.fresh();
                self.emit(&format!("  %_t{} = zext i1 %_t{} to i64\n", t4, t3));
                self.emit(&format!("  store i64 %_t{}, i64* %r{}\n", t4, d));
            }
        }
    }

    fn emit_arith(&mut self, op: &str, inst: &LirInst) {
        if let Some(d) = inst.dest {
            if let (Some(l), Some(r)) = (self.op_reg(0, inst), self.op_reg(1, inst)) {
                let dty = self.reg_type(d);
                let t1 = self.fresh();
                self.emit(&format!("  %_t{} = load {}, i64* %r{}\n", t1, dty, l));
                let t2 = self.fresh();
                self.emit(&format!("  %_t{} = load {}, i64* %r{}\n", t2, dty, r));
                let t3 = self.fresh();
                self.emit(&format!("  %_t{} = {} i64 %_t{}, %_t{}\n", t3, op, t1, t2));
                self.emit(&format!("  store {} %_t{}, {}* %r{}\n", dty, t3, dty, d));
            }
        }
    }

    fn emit_call(&mut self, inst: &LirInst) {
        let callee = match inst.operands.first() {
            Some(LirOperand::Label(n)) => n.clone(),
            _ => return,
        };
        let is_extern = self.program.extern_functions.iter().any(|e| e.name == callee);

        let mut arg_temps: Vec<usize> = Vec::new();
        for op in &inst.operands[1..] {
            match op {
                LirOperand::Reg(r) => {
                    let t = self.fresh();
                    self.emit(&format!("  %_t{} = load i64, i64* %r{}\n", t, r));
                    arg_temps.push(t);
                }
                LirOperand::ImmI64(i) => {
                    let t = self.fresh();
                    self.emit(&format!("  %_t{} = add i64 0, {}\n", t, i));
                    arg_temps.push(t);
                }
                _ => {
                    let t = self.fresh();
                    self.emit(&format!("  %_t{} = add i64 0, 0\n", t));
                    arg_temps.push(t);
                }
            }
        }

        let call_args: Vec<String> = arg_temps.iter().map(|t| format!("i64 %_t{}", t)).collect();
        let mut all_args = call_args;
        if is_extern {
            while all_args.len() < 6 {
                let t = self.fresh();
                self.emit(&format!("  %_t{} = add i64 0, 0\n", t));
                all_args.push(format!("i64 %_t{}", t));
            }
        }
        let arg_str = all_args.join(", ");

        let t = self.fresh();
        self.emit(&format!("  %_t{} = call i64 @{}({})\n", t, callee, arg_str));

        if let Some(d) = inst.dest {
            self.emit(&format!("  store i64 %_t{}, i64* %r{}\n", t, d));
        }
    }

    fn reg_type(&self, reg: VirtReg) -> &'static str {
        if self.float_regs.contains(&reg) { "double" } else { "i64" }
    }

    fn op_reg(&self, idx: usize, inst: &LirInst) -> Option<VirtReg> {
        inst.operands.get(idx).and_then(|op| {
            if let LirOperand::Reg(r) = op { Some(*r) } else { None }
        })
    }
}
