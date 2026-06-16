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

/// A high-level pipeline that simplifies the compilation process.
/// It automates the transition between AST, HIR, MIR, and LIR,
/// and handles optimization and code generation.
pub struct EasyPipeline {
    pass_manager: PassManager,
    opt_iterations: usize,
    entry_point: String,
}

impl Default for EasyPipeline {
    fn default() -> Self {
        let mut pm = PassManager::default();
        pm.add_pass(Box::new(brak_opt_inline::Inlining));
        pm.add_pass(Box::new(brak_opt_cp::ConstantPropagation));
        pm.add_pass(Box::new(brak_opt_fold::ConstantFolding));
        pm.add_pass(Box::new(brak_opt_gvn::GlobalValueNumbering));
        pm.add_pass(Box::new(brak_opt_dce::DeadCodeElimination));
        
        Self {
            pass_manager: pm,
            opt_iterations: 1,
            entry_point: "main".to_string(),
        }
    }
}

impl EasyPipeline {
    /// Creates a new EasyPipeline with default settings.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the number of optimization iterations to run.
    pub fn with_iterations(mut self, iterations: usize) -> Self {
        self.opt_iterations = iterations;
        self
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
        for _ in 0..self.opt_iterations {
            lir = self.pass_manager.run(lir)?;
        }
        
        Ok(lir)
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
