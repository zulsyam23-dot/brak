use brak_core::{SourceMap, Result as BrakResult};
use brak_frontend::lexer::{AsciiLexer, BrakLexer};
use brak_ir_ast::ast::Program;
use brak_ir_hir::lower::HirLower;
use brak_ir_hir::typeck::TypeChecker;
use brak_ir_mir::lower::MirLower;
use brak_ir_lir::lower::LirLower;
use brak_ir_lir::lir::LirProgram;
use brak_opt_traits::PassManager;
use brak_codegen_traits::CodegenBackend;
use brak_codegen_obj::ObjBackend;
use brak_link_traits::{LinkerBackend, ObjectFile};
use brak_link_native::NativeLinker;

use std::collections::HashSet;

/// Optimization level, similar to -O0/-O1/-O2/-O3.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptLevel {
    /// No optimization.
    None,
    /// Basic optimizations (fold + DCE).
    Less,
    /// Full pipeline, 1 iteration (default).
    Default,
    /// Full pipeline, 4 iterations.
    Aggressive,
}

const ALL_PASSES: &[&str] = &["inline", "cp", "fold", "gvn", "dce"];

fn build_pass_manager(level: OptLevel, iterations: usize, disabled: &HashSet<&'static str>) -> PassManager {
    let mut pm = PassManager::default();
    let enabled: &[&str] = match level {
        OptLevel::None => &[],
        OptLevel::Less => &["fold", "dce"],
        OptLevel::Default | OptLevel::Aggressive => ALL_PASSES,
    };
    for name in enabled {
        if !disabled.contains(name) {
            pm.add_pass(match *name {
                "inline" => Box::new(brak_opt_inline::Inlining),
                "cp" => Box::new(brak_opt_cp::ConstantPropagation),
                "fold" => Box::new(brak_opt_fold::ConstantFolding),
                "gvn" => Box::new(brak_opt_gvn::GlobalValueNumbering),
                "dce" => Box::new(brak_opt_dce::DeadCodeElimination),
                _ => unreachable!(),
            });
        }
    }
    pm.max_iterations = iterations;
    pm
}

/// A high-level pipeline that simplifies the compilation process.
/// It automates the transition between AST, HIR, MIR, and LIR,
/// and handles optimization and code generation.
pub struct EasyPipeline {
    opt_level: OptLevel,
    opt_iterations: Option<usize>,
    disabled_passes: HashSet<&'static str>,
    verbose: bool,
    entry_point: String,
}

impl Default for EasyPipeline {
    fn default() -> Self {
        Self {
            opt_level: OptLevel::Default,
            opt_iterations: None,
            disabled_passes: HashSet::new(),
            verbose: false,
            entry_point: "main".to_string(),
        }
    }
}

impl EasyPipeline {
    /// Creates a new EasyPipeline with default settings.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the optimization level.
    pub fn with_opt_level(mut self, level: OptLevel) -> Self {
        self.opt_level = level;
        self
    }

    /// Disables a specific optimization pass by name (e.g. "inline").
    pub fn without_pass(mut self, name: &'static str) -> Self {
        self.disabled_passes.insert(name);
        self
    }

    /// Prints optimizer activity to stdout.
    pub fn with_verbose(mut self, verbose: bool) -> Self {
        self.verbose = verbose;
        self
    }

    /// Sets the number of optimization iterations to run.
    /// Overrides the default for the chosen [`OptLevel`].
    pub fn with_iterations(mut self, iterations: usize) -> Self {
        self.opt_iterations = Some(iterations);
        self
    }

    fn pass_manager(&self) -> PassManager {
        let iterations = self.opt_iterations.unwrap_or(match self.opt_level {
            OptLevel::Aggressive => 4,
            _ => 1,
        });
        build_pass_manager(self.opt_level, iterations, &self.disabled_passes)
    }

    /// Sets the entry point function name (default is "main").
    pub fn with_entry_point(mut self, entry: &str) -> Self {
        self.entry_point = entry.to_string();
        self
    }

    /// Full flow from source string to binary executable.
    /// This is the easiest way to use Brak as a compiler library.
    pub fn build_executable(&self, name: &str, source: &str, output_path: &str) -> BrakResult<()> {
        let lir = self.compile_to_lir(name, source)?;
        self.lir_to_executable(name, lir, output_path)
    }

    /// Compiles a source string to LIR (Low-level IR) with optimizations.
    pub fn compile_to_lir(&self, name: &str, source: &str) -> BrakResult<LirProgram> {
        let sm = SourceMap::new(name, source);
        let mut lexer = AsciiLexer::new();
        let tokens = lexer.lex(&sm);
        
        let parser = brak_frontend::parser::Parser::new();
        let ast = parser.parse(&tokens).map_err(|e| {
            let msg = format!("Parser error: {}", e);
            Box::<dyn std::error::Error>::from(msg)
        })?;
        
        self.ast_to_lir(name, ast)
    }

    /// Processes an AST to LIR, applying all necessary transformations and optimizations.
    pub fn ast_to_lir(&self, name: &str, ast: Program) -> BrakResult<LirProgram> {
        let hir_lower = HirLower::new();
        let hir = hir_lower.lower(ast).map_err(|e| {
            let msg = format!("HIR Lowering error: {:?}", e);
            Box::<dyn std::error::Error>::from(msg)
        })?;
        
        let mut typeck = TypeChecker::new();
        typeck.check(&hir).map_err(|e| {
            let msg = format!("Typecheck error: {:?}", e);
            Box::<dyn std::error::Error>::from(msg)
        })?;
        
        let mut mir_lower = MirLower::new();
        let mir = mir_lower.lower(hir).map_err(|e| {
            let msg = format!("MIR Lowering error: {:?}", e);
            Box::<dyn std::error::Error>::from(msg)
        })?;
        
        let mut lir_lower = LirLower::new();
        lir_lower.set_file_id(0);
        let mut lir = lir_lower.lower(mir);
        lir.files = vec![name.to_string()];
        
        // Run optimizations
        // BUG-H08: PassManager::run already iterates max_iterations times;
        // wrapping it in another loop ran every pass up to N² times.
        let pm = self.pass_manager();
        lir = pm.run(lir)?;

        Ok(lir)
    }

    /// Compiles a source string to an object file (no linking).
    pub fn compile_to_object(&self, name: &str, source: &str) -> BrakResult<Vec<u8>> {
        let lir = self.compile_to_lir(name, source)?;
        ObjBackend::default().emit(&lir)
    }

    /// Converts LIR to a standalone executable file.
    pub fn lir_to_executable(&self, name: &str, lir: LirProgram, output_path: &str) -> BrakResult<()> {
        let obj_backend = ObjBackend::default();
        let obj_bytes = obj_backend.emit(&lir)?;
        
        let linker = NativeLinker;
        let obj_file = ObjectFile {
            name: format!("{}.o", name),
            data: obj_bytes,
        };
        
        let output = linker.link(&[obj_file], &self.entry_point, 0x400000)?;
        std::fs::write(output_path, output.data).map_err(|e| {
            let msg = format!("Failed to write executable: {}", e);
            Box::<dyn std::error::Error>::from(msg)
        })?;
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opt_levels_change_pipeline() {
        let src = "fn main() -> i32 { 42 }";
        let none = EasyPipeline::new().with_opt_level(OptLevel::None).compile_to_lir("t", src).unwrap();
        let aggr = EasyPipeline::new().with_opt_level(OptLevel::Aggressive).compile_to_lir("t", src).unwrap();
        assert!(!none.functions.is_empty() && !aggr.functions.is_empty());

        let obj = EasyPipeline::new().compile_to_object("t", src).unwrap();
        assert!(!obj.is_empty());
    }
}

#[cfg(test)]
mod nested_tests {
    use super::*;

    fn run(src: &str, level: OptLevel) -> i32 {
        let lir = EasyPipeline::new().with_opt_level(level).compile_to_lir("t", src).unwrap();
        // interpret via brak-test? no - just check via building exe
        let dir = std::env::temp_dir().join("brak_nested_test");
        std::fs::create_dir_all(&dir).unwrap();
        let out = dir.join(format!("t_{:?}.exe", level));
        EasyPipeline::new().with_opt_level(level).lir_to_executable("t", lir, out.to_str().unwrap()).unwrap();
        let status = std::process::Command::new(&out).status().unwrap();
        status.code().unwrap()
    }

    #[test]
    fn nested_correct_at_every_opt_level() {
        let src = "fn main() { let x = 0; let i = 0; while i < 5 { if i % 2 == 0 { x = x + i; } else { x = x + 1; } i = i + 1; } return x; }";
        for level in [OptLevel::None, OptLevel::Less, OptLevel::Default, OptLevel::Aggressive] {
            assert_eq!(run(src, level), 8, "wrong result at {:?}", level);
        }
    }
}
