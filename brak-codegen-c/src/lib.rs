use std::collections::HashSet;
use brak_codegen_traits::CodegenBackend;
use brak_core::Result;
use brak_ir_lir::lir::{
    LirOpcode, LirOperand, LirProgram, LirFunction, LirInst, VirtReg, LirType,
};

pub struct CBackend;

impl CodegenBackend for CBackend {
    fn name(&self) -> &'static str {
        "c"
    }

    fn emit(&self, program: &LirProgram) -> Result<Vec<u8>> {
        Ok(emit_c(program).into_bytes())
    }
}

pub fn emit_c(program: &LirProgram) -> String {
    let mut out = CWriter::new(&program.string_table);
    out.files = program.files.clone();
    out.emit_program(program);
    out.finish()
}

struct CWriter {
    output: String,
    indent: usize,
    float_regs: HashSet<VirtReg>,
    string_table: Vec<String>,
    string_refs: HashSet<usize>,
    last_cmp_lhs: Option<(VirtReg, bool)>,
    last_cmp_rhs: Option<(VirtReg, bool)>,
    last_line: usize,
    files: Vec<String>,
}

impl CWriter {
    fn new(string_table: &[String]) -> Self {
        Self {
            output: String::new(),
            indent: 0,
            float_regs: HashSet::new(),
            string_table: string_table.to_vec(),
            string_refs: HashSet::new(),
            last_cmp_lhs: None,
            last_cmp_rhs: None,
            last_line: 0,
            files: Vec::new(),
        }
    }

    fn finish(self) -> String {
        self.output
    }

    fn emit_program(&mut self, program: &LirProgram) {
        self.emit_line("#include <stdint.h>");
        self.emit_line("#include <stdbool.h>");
        self.emit_line("#include <stdlib.h>");
        self.emit_line("#include <string.h>");
        self.emit_line("struct _GenericStruct { int64_t _data[1024]; }; // Hack for generic field access");
        self.emit_blank();

        self.emit_struct_decls(program);
        self.emit_enum_decls(program);
        self.emit_blank();

        self.emit_extern_decls(program);
        self.emit_string_table();
        self.emit_blank();

        for func in &program.functions {
            self.analyze_function(func);
            self.emit_function(func);
            self.emit_blank();
        }
    }

    fn emit_struct_decls(&mut self, program: &LirProgram) {
        for s in &program.structs {
            self.emit_line(&format!("typedef struct {} {{", s.name));
            self.indent += 1;
            for (fname, fty) in &s.fields {
                self.emit_line(&format!("{} {};", self.c_type(fty), fname));
            }
            self.indent -= 1;
            self.emit_line(&format!("}} {};", s.name));
            self.emit_blank();
        }
    }

    fn emit_enum_decls(&mut self, program: &LirProgram) {
        for e in &program.enums {
            // Simplified enum: just a typedef to int for now
            self.emit_line(&format!("typedef int {};", e.name));
        }
    }

    fn c_type(&self, ty: &LirType) -> String {
        match ty {
            LirType::I32 => "int32_t".to_string(),
            LirType::I64 => "int64_t".to_string(),
            LirType::F32 => "float".to_string(),
            LirType::F64 => "double".to_string(),
            LirType::Bool => "bool".to_string(),
            LirType::String => "const char*".to_string(),
            LirType::Void => "void".to_string(),
            LirType::Named(s) => s.clone(),
            LirType::Ptr(inner) => format!("{}*", self.c_type(inner)),
        }
    }

    fn emit_extern_decls(&mut self, program: &LirProgram) {
        for ext in &program.extern_functions {
            self.emit_line(&format!(
                "extern int64_t {}(int64_t r0, int64_t r1, int64_t r2, int64_t r3, int64_t r4, int64_t r5);",
                ext.name
            ));
        }
        if !program.extern_functions.is_empty() {
            self.emit_blank();
        }
    }

    fn emit_string_table(&mut self) {
        let entries: Vec<(usize, String)> = self
            .string_table
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let escaped = s
                    .replace('\\', "\\\\")
                    .replace('"', "\\\"")
                    .replace('\n', "\\n")
                    .replace('\r', "\\r")
                    .replace('\t', "\\t");
                (i, escaped)
            })
            .collect();
        for (i, escaped) in entries {
            self.emit_line(&format!("static const char* const _s{} = \"{}\";", i, escaped));
        }
    }

    fn analyze_function(&mut self, func: &LirFunction) {
        self.float_regs.clear();
        self.string_refs.clear();

        for block in &func.blocks {
            for inst in &block.insts {
                for op in &inst.operands {
                    if let LirOperand::ImmF64(_) = op {
                        if let Some(d) = inst.dest {
                            self.float_regs.insert(d);
                        }
                    }
                }
                if inst.opcode == LirOpcode::Load {
                    for op in &inst.operands {
                        if let LirOperand::StringRef(idx) = op {
                            self.string_refs.insert(*idx);
                        }
                    }
                }
                if inst.opcode == LirOpcode::Mov {
                    for op in &inst.operands {
                        if let LirOperand::StringRef(idx) = op {
                            self.string_refs.insert(*idx);
                        }
                    }
                }
            }
        }

        for param in &func.params {
            if !self.float_regs.contains(param) {
                self.float_regs.remove(param);
            }
        }
    }

    fn emit_function(&mut self, func: &LirFunction) {
        let ret_type = "int64_t";
        let params: Vec<String> = func
            .params
            .iter()
            .map(|p| {
                if self.float_regs.contains(p) {
                    format!("double r{}", p)
                } else {
                    format!("int64_t r{}", p)
                }
            })
            .collect();
        let param_str = if params.is_empty() {
            "void".to_string()
        } else {
            params.join(", ")
        };

        self.emit_line(&format!("{} {}({})", ret_type, func.name, param_str));
        self.emit_line("{");
        self.indent += 1;

        self.emit_reg_decls(func);
        self.emit_string_ref_decls();

        for block in &func.blocks {
            self.emit_blank();
            self.emit_line(&format!("block_{}:", block.id));
            self.indent += 1;
            self.last_cmp_lhs = None;
            self.last_cmp_rhs = None;

            for inst in &block.insts {
                self.emit_inst(inst);
            }

            self.indent -= 1;
        }

        self.indent -= 1;
        self.emit_line("}");
    }

    fn emit_reg_decls(&mut self, func: &LirFunction) {
        let mut int_regs: Vec<VirtReg> = Vec::new();
        let mut float_regs: Vec<VirtReg> = Vec::new();
        let mut seen = HashSet::new();

        for block in &func.blocks {
            for inst in &block.insts {
                if let Some(d) = inst.dest {
                    if !seen.contains(&d) && !func.params.contains(&d) {
                        seen.insert(d);
                        if self.float_regs.contains(&d) {
                            float_regs.push(d);
                        } else {
                            int_regs.push(d);
                        }
                    }
                }
                for op in &inst.operands {
                    if let LirOperand::Reg(r) = op {
                        if !seen.contains(r) && !func.params.contains(r) {
                            seen.insert(*r);
                            if self.float_regs.contains(r) {
                                float_regs.push(*r);
                            } else {
                                int_regs.push(*r);
                            }
                        }
                    }
                }
            }
        }

        if !int_regs.is_empty() {
            let regs: Vec<String> = int_regs.iter().map(|r| format!("r{}", r)).collect();
            self.emit_line(&format!("int64_t {};", regs.join(", ")));
        }
        if !float_regs.is_empty() {
            let regs: Vec<String> = float_regs.iter().map(|r| format!("r{}", r)).collect();
            self.emit_line(&format!("double {};", regs.join(", ")));
        }
    }

    fn emit_string_ref_decls(&mut self) {
        let refs: Vec<usize> = self.string_refs.iter().copied().collect();
        for idx in refs {
            self.emit_line(&format!("const char* _s{}p = _s{};", idx, idx));
        }
    }

    fn emit_inst(&mut self, inst: &LirInst) {
        // Emit #line directive if source line changed
        if inst.debug.start.line > 0 && inst.debug.start.line != self.last_line {
            self.last_line = inst.debug.start.line;
            let file = self.files.get(inst.file_id).map(|s| s.as_str()).unwrap_or("");
            if !file.is_empty() {
                self.output.push_str(&format!("#line {} \"{}\"\n", inst.debug.start.line, file));
            } else {
                self.output.push_str(&format!("#line {}\n", inst.debug.start.line));
            }
        } else if inst.debug.start.line == 0 && self.last_line != 0 {
            // Reset to unknown line
            self.output.push_str("#line 0\n");
            self.last_line = 0;
        }
        match inst.opcode {
            LirOpcode::Cmp => {
                self.last_cmp_lhs = self.op_as_reg_ref(0, inst);
                self.last_cmp_rhs = self.op_as_reg_ref(1, inst);
            }
            LirOpcode::SetEq => {
                let dest = self.dest_name(inst);
                let lhs = self.cmp_op_str(&self.last_cmp_lhs);
                let rhs = self.cmp_op_str(&self.last_cmp_rhs);
                self.emit_line(&format!("{} = ({} == {}) ? 1 : 0;", dest, lhs, rhs));
            }
            LirOpcode::SetNe => {
                let dest = self.dest_name(inst);
                let lhs = self.cmp_op_str(&self.last_cmp_lhs);
                let rhs = self.cmp_op_str(&self.last_cmp_rhs);
                self.emit_line(&format!("{} = ({} != {}) ? 1 : 0;", dest, lhs, rhs));
            }
            LirOpcode::SetLt => {
                let dest = self.dest_name(inst);
                let lhs = self.cmp_op_str(&self.last_cmp_lhs);
                let rhs = self.cmp_op_str(&self.last_cmp_rhs);
                self.emit_line(&format!("{} = ({} < {}) ? 1 : 0;", dest, lhs, rhs));
            }
            LirOpcode::SetLe => {
                let dest = self.dest_name(inst);
                let lhs = self.cmp_op_str(&self.last_cmp_lhs);
                let rhs = self.cmp_op_str(&self.last_cmp_rhs);
                self.emit_line(&format!("{} = ({} <= {}) ? 1 : 0;", dest, lhs, rhs));
            }
            LirOpcode::SetGt => {
                let dest = self.dest_name(inst);
                let lhs = self.cmp_op_str(&self.last_cmp_lhs);
                let rhs = self.cmp_op_str(&self.last_cmp_rhs);
                self.emit_line(&format!("{} = ({} > {}) ? 1 : 0;", dest, lhs, rhs));
            }
            LirOpcode::SetGe => {
                let dest = self.dest_name(inst);
                let lhs = self.cmp_op_str(&self.last_cmp_lhs);
                let rhs = self.cmp_op_str(&self.last_cmp_rhs);
                self.emit_line(&format!("{} = ({} >= {}) ? 1 : 0;", dest, lhs, rhs));
            }
            LirOpcode::Mov => self.emit_mov(inst),
            LirOpcode::Add => self.emit_binop("+", inst),
            LirOpcode::Sub => self.emit_binop("-", inst),
            LirOpcode::Mul => self.emit_binop("*", inst),
            LirOpcode::Div => self.emit_binop("/", inst),
            LirOpcode::Mod => self.emit_binop("%", inst),
            LirOpcode::Neg => self.emit_unop("-", inst),
            LirOpcode::Not => self.emit_unop("!", inst),
            LirOpcode::And => self.emit_binop("&", inst),
            LirOpcode::Or => self.emit_binop("|", inst),
            LirOpcode::Xor => self.emit_binop("^", inst),
            LirOpcode::Shl => self.emit_binop("<<", inst),
            LirOpcode::Shr => self.emit_binop(">>", inst),
            LirOpcode::Load => self.emit_load(inst),
            LirOpcode::Store => self.emit_store(inst),
            LirOpcode::Alloca => self.emit_alloca(inst),
            LirOpcode::GetField => self.emit_get_field(inst),
            LirOpcode::StructInit => self.emit_struct_init(inst),
            LirOpcode::SetField => self.emit_set_field(inst),
            LirOpcode::Call => self.emit_call(inst),
            LirOpcode::Ret => self.emit_ret(inst),
            LirOpcode::Jmp => self.emit_jmp(inst),
            LirOpcode::Br => self.emit_br(inst),
            LirOpcode::Push | LirOpcode::Pop => {}
            LirOpcode::Comment => {}
        }
    }

    fn emit_get_field(&mut self, inst: &LirInst) {
        let dest = self.dest_name(inst);
        let obj = self.op_str(0, inst);
        let field = match inst.operands.get(1) {
            Some(LirOperand::Field(s)) => s,
            _ => "unknown",
        };
        // Use a cast to a generic pointer and then to the struct if possible,
        // or just use a placeholder struct name.
        self.emit_line(&format!("{} = ((struct _GenericStruct*)(uintptr_t){})->{};", dest, obj, field));
    }

    fn emit_struct_init(&mut self, inst: &LirInst) {
        let dest = self.dest_name(inst);
        let struct_name = match inst.operands.get(0) {
            Some(LirOperand::Label(s)) => s,
            _ => "unknown",
        };
        self.emit_line(&format!("{} = (int64_t)(uintptr_t)calloc(1, sizeof(struct {}));", dest, struct_name));
        let mut i = 1;
        while i < inst.operands.len() {
            let field = match inst.operands.get(i) {
                Some(LirOperand::Field(s)) => s,
                _ => break,
            };
            let val = self.operand_str(&inst.operands[i+1]);
            self.emit_line(&format!("((struct {}*)(uintptr_t){})->{} = {};", struct_name, dest, field, val));
            i += 2;
        }
    }

    fn emit_set_field(&mut self, inst: &LirInst) {
        let obj = self.op_str(0, inst);
        let field = match inst.operands.get(1) {
            Some(LirOperand::Field(s)) => s,
            _ => "unknown",
        };
        let val = self.op_str(2, inst);
        // Generic cast as we don't know the struct name here easily
        self.emit_line(&format!("((struct _GenericStruct*)(uintptr_t){})->{} = {};", obj, field, val));
    }

    fn emit_mov(&mut self, inst: &LirInst) {
        let dest = self.dest_name(inst);
        let src = self.op_str(0, inst);
        self.emit_line(&format!("{} = {};", dest, src));
    }

    fn emit_binop(&mut self, op: &str, inst: &LirInst) {
        let dest = self.dest_name(inst);
        let lhs = self.op_str(0, inst);
        let rhs = self.op_str(1, inst);
        self.emit_line(&format!("{} = {} {} {};", dest, lhs, op, rhs));
    }

    fn emit_unop(&mut self, op: &str, inst: &LirInst) {
        let dest = self.dest_name(inst);
        let src = self.op_str(0, inst);
        self.emit_line(&format!("{} = {}{};", dest, op, src));
    }

    fn emit_load(&mut self, inst: &LirInst) {
        let dest = self.dest_name(inst);
        let addr = self.op_str(0, inst);
        if self.is_float_dest(inst) {
            self.emit_line(&format!("{} = *(double*)(uintptr_t){};", dest, addr));
        } else {
            self.emit_line(&format!("{} = *(int64_t*)(uintptr_t){};", dest, addr));
        }
    }

    fn emit_store(&mut self, inst: &LirInst) {
        let addr = self.op_str(0, inst);
        let src = self.op_str(1, inst);
        if self.is_float_reg(inst.operands.get(1)) {
            self.emit_line(&format!("*(double*)(uintptr_t){} = {};", addr, src));
        } else {
            self.emit_line(&format!("*(int64_t*)(uintptr_t){} = {};", addr, src));
        }
    }

    fn emit_alloca(&mut self, inst: &LirInst) {
        let dest = self.dest_name(inst);
        let size = self.op_str(0, inst);
        self.emit_line(&format!("{} = (int64_t)(uintptr_t)calloc(1, {});", dest, size));
    }

    fn emit_call(&mut self, inst: &LirInst) {
        let callee = match inst.operands.first() {
            Some(LirOperand::Label(name)) => name.clone(),
            _ => return,
        };
        let args: Vec<String> = inst.operands[1..]
            .iter()
            .map(|op| self.operand_str(op))
            .collect();
        let arg_str = if args.is_empty() {
            String::new()
        } else {
            args.join(", ")
        };

        if let Some(dest) = inst.dest {
            let dname = self.reg_name(dest);
            self.emit_line(&format!("{} = {}({});", dname, callee, arg_str));
        } else {
            self.emit_line(&format!("{}({});", callee, arg_str));
        }
    }

    fn emit_ret(&mut self, inst: &LirInst) {
        if let Some(op) = inst.operands.first() {
            let val = self.operand_str(op);
            self.emit_line(&format!("return {};", val));
        } else {
            self.emit_line("return 0;");
        }
    }

    fn emit_jmp(&mut self, inst: &LirInst) {
        if let Some(LirOperand::Label(name)) = inst.operands.first() {
            self.emit_line(&format!("goto {};", name));
        }
    }

    fn emit_br(&mut self, inst: &LirInst) {
        if inst.operands.len() < 3 {
            return;
        }
        let cond = self.operand_str(&inst.operands[0]);
        let label_t = match &inst.operands[1] {
            LirOperand::Label(name) => name.clone(),
            _ => return,
        };
        let label_f = match &inst.operands[2] {
            LirOperand::Label(name) => name.clone(),
            _ => return,
        };
        self.emit_line(&format!("if ({}) goto {}; else goto {};", cond, label_t, label_f));
    }

    fn dest_name(&self, inst: &LirInst) -> String {
        match inst.dest {
            Some(d) => self.reg_name(d),
            None => "_".to_string(),
        }
    }

    fn reg_name(&self, reg: VirtReg) -> String {
        format!("r{}", reg)
    }

    fn op_str(&self, idx: usize, inst: &LirInst) -> String {
        inst.operands
            .get(idx)
            .map(|op| self.operand_str(op))
            .unwrap_or_default()
    }

    fn op_as_reg_ref(&self, idx: usize, inst: &LirInst) -> Option<(VirtReg, bool)> {
        inst.operands.get(idx).and_then(|op| {
            if let LirOperand::Reg(r) = op {
                Some((*r, self.float_regs.contains(r)))
            } else {
                None
            }
        })
    }

    fn cmp_op_str(&self, reg: &Option<(VirtReg, bool)>) -> String {
        match reg {
            Some((r, is_float)) => {
                if *is_float {
                    format!("r{}", r)
                } else {
                    format!("r{}", r)
                }
            }
            None => "0".to_string(),
        }
    }

    fn operand_str(&self, op: &LirOperand) -> String {
        match op {
            LirOperand::Reg(r) => format!("r{}", r),
            LirOperand::ImmI64(i) => i.to_string(),
            LirOperand::ImmF64(f) => {
                if f.is_nan() {
                    "0.0/0.0".to_string()
                } else if f.is_infinite() {
                    if *f > 0.0 {
                        "1.0/0.0".to_string()
                    } else {
                        "-1.0/0.0".to_string()
                    }
                } else {
                    format!("{:.20}", f)
                }
            }
            LirOperand::Label(name) => name.clone(),
            LirOperand::StackSlot(slot) => format!("*(int64_t*)(_stack + {})", slot * 8),
            LirOperand::StringRef(idx) => format!("(int64_t)(uintptr_t)_s{}p", idx),
            LirOperand::Field(s) => s.clone(),
        }
    }

    fn is_float_dest(&self, inst: &LirInst) -> bool {
        inst.dest
            .map(|d| self.float_regs.contains(&d))
            .unwrap_or(false)
    }

    fn is_float_reg(&self, op: Option<&LirOperand>) -> bool {
        match op {
            Some(LirOperand::Reg(r)) => self.float_regs.contains(r),
            _ => false,
        }
    }

    fn emit_line(&mut self, line: &str) {
        for _ in 0..self.indent {
            self.output.push_str("    ");
        }
        self.output.push_str(line);
        self.output.push('\n');
    }

    fn emit_blank(&mut self) {
        self.output.push('\n');
    }
}
