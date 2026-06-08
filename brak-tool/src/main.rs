use clap::{Parser as ClapParser, Subcommand};
use brak_core::SourceMap;
use brak_codegen_traits::CodegenBackend;
use brak_codegen_c::CBackend;
use brak_codegen_wasm::WasmBackend;
use brak_codegen_llvm::LlvmBackend;
use brak_frontend::lexer::{AsciiLexer, BrakLexer};
use brak_frontend::parser::Parser as BrakParser;
use brak_ir_ast::ast::Program;
use brak_ir_hir::hir::HirProgram;
use brak_ir_hir::lower::HirLower;
use brak_ir_hir::typeck::TypeChecker;
use brak_ir_mir::mir::MirProgram;
use brak_ir_mir::lower::MirLower;
use brak_ir_lir::lir::LirProgram;
use brak_ir_lir::lower::LirLower;
use brak_link_traits::{LinkerBackend, ObjectFile};
use brak_link_native::NativeLinker;
use brak_opt_traits::PassManager;
use brak_opt_dce::DeadCodeElimination;
use brak_opt_cp::ConstantPropagation;
use brak_opt_gvn::GlobalValueNumbering;
use brak_opt_inline::Inlining;
use brak_opt_licm::Licm;
use brak_opt_jt::JumpThreading;
use brak_opt_tco::TailCallOptimization;
use brak_opt_fold::ConstantFolding;
use brak_polyglot::{PolyglotBridge, CHeaderGenerator, PyO3Generator};

#[derive(ClapParser)]
#[command(name = "brak", about = "Brak Language Construction Toolkit")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Emit intermediate representation
    EmitIr {
        /// Source file to compile
        file: String,
        /// IR level: tokens, ast, hir, mir, lir, asm, obj
        #[arg(short, long, default_value = "ast")]
        level: String,
        /// Output format: text, json, yaml (ignored for asm/obj)
        #[arg(short, long, default_value = "text")]
        format: String,
        /// Output file (default: stdout for text, <file>.o for obj)
        #[arg(short, long)]
        output: Option<String>,
    },
    /// Build a standalone executable or shared library
    Build {
        /// Source files or object files to compile and link
        #[arg(required = true)]
        files: Vec<String>,
        /// Entry point function (default: main)
        #[arg(short, long, default_value = "main")]
        entry: String,
        /// Output file (default: first_input_file)
        #[arg(short, long)]
        output: Option<String>,
        /// Paths to external optimization pass plugins (.so, .dll, .dylib)
        #[arg(long)]
        opt_pass: Vec<String>,
        /// Number of times to run the optimization pipeline (default: 1)
        #[arg(long, default_value = "1")]
        opt_iterations: usize,
        /// Show detailed optimization logs
        #[arg(long)]
        verbose_opt: bool,
        /// Generate a C header for the compiled functions
        #[arg(long)]
        gen_h: Option<String>,
        /// Build as shared library (.dll/.so) instead of executable
        #[arg(long)]
        shared: bool,
        /// Generate a PyO3 Python extension module (specify module name)
        #[arg(long)]
        py_module: Option<String>,
    },
}

fn compile_to_ast(file: &str) -> Result<(SourceMap, Program), Box<dyn std::error::Error>> {
    let source = std::fs::read_to_string(file)?;
    let source_map = SourceMap::new(file, &source);
    let mut lexer = AsciiLexer::new();
    let tokens = lexer.lex(&source_map);
    let parser = BrakParser::new();
    let ast = parser.parse(&tokens)?;
    Ok((source_map, ast))
}

fn compile_to_hir(file: &str) -> Result<(SourceMap, HirProgram), Box<dyn std::error::Error>> {
    let (_sm, ast) = compile_to_ast(file)?;
    let lower = HirLower::new();
    let hir = lower.lower(ast).map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;
    let mut typeck = TypeChecker::new();
    typeck.check(&hir).map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;
    Ok((_sm, hir))
}

fn compile_to_mir(file: &str) -> Result<(SourceMap, MirProgram), Box<dyn std::error::Error>> {
    let (_sm, hir) = compile_to_hir(file)?;
    let mut lower = MirLower::new();
    let mir = lower.lower(hir)?;
    Ok((_sm, mir))
}

fn compile_to_lir(file: &str) -> Result<(SourceMap, LirProgram), Box<dyn std::error::Error>> {
    let (_sm, mir) = compile_to_mir(file)?;
    let mut lower = LirLower::new();
    lower.set_file_id(0);
    let mut lir = lower.lower(mir);
    lir.files = vec![file.to_string()];
    Ok((_sm, lir))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Commands::EmitIr { file, level, format, output } => {
            match level.as_str() {
                "tokens" => {
                    let source = std::fs::read_to_string(&file)?;
                    let source_map = SourceMap::new(&file, &source);
                    let mut lexer = AsciiLexer::new();
                    let tokens = lexer.lex(&source_map);
                    match format.as_str() {
                        "json" => println!("{}", serde_json::to_string_pretty(&tokens)?),
                        "yaml" => println!("{}", serde_yaml::to_string(&tokens)?),
                        _ => {
                            for tok in &tokens {
                                println!("{:<15} {:?}", format!("{:?}", tok.kind), tok.lexeme);
                            }
                        }
                    }
                }
                "ast" => {
                    let (_sm, ast) = compile_to_ast(&file)?;
                    match format.as_str() {
                        "json" => println!("{}", serde_json::to_string_pretty(&ast)?),
                        "yaml" => println!("{}", serde_yaml::to_string(&ast)?),
                        _ => println!("{}", ast),
                    }
                }
                "hir" => {
                    let (_sm, hir) = compile_to_hir(&file)?;
                    match format.as_str() {
                        "json" => println!("{}", serde_json::to_string_pretty(&hir)?),
                        "yaml" => println!("{}", serde_yaml::to_string(&hir)?),
                        _ => println!("{}", hir),
                    }
                }
                "mir" => {
                    let (_sm, mir) = compile_to_mir(&file)?;
                    match format.as_str() {
                        "json" => println!("{}", serde_json::to_string_pretty(&mir)?),
                        "yaml" => println!("{}", serde_yaml::to_string(&mir)?),
                        _ => println!("{}", mir),
                    }
                }
                "lir" => {
                    let (_sm, lir) = compile_to_lir(&file)?;
                    match format.as_str() {
                        "json" => println!("{}", serde_json::to_string_pretty(&lir)?),
                        "yaml" => println!("{}", serde_yaml::to_string(&lir)?),
                        _ => println!("{}", lir),
                    }
                }
                "asm" => {
                    let (_sm, lir) = compile_to_lir(&file)?;
                    let backend = brak_codegen_asm::AsmBackend;
                    let bytes = backend.emit(&lir)?;
                    if let Some(path) = output {
                        std::fs::write(&path, &bytes)?;
                    } else {
                        println!("{}", String::from_utf8_lossy(&bytes));
                    }
                }
                "obj" => {
                    let (_sm, lir) = compile_to_lir(&file)?;
                    let backend = brak_codegen_obj::ObjBackend::default();
                    let bytes = backend.emit(&lir)?;
                let out_path = output.clone().unwrap_or_else(|| {
                        let stem = std::path::Path::new(&file).file_stem()
                            .map(|s| s.to_string_lossy().to_string())
                            .unwrap_or_else(|| "output".to_string());
                        format!("{stem}.o")
                    });
                    std::fs::write(&out_path, &bytes)?;
                }
                "c" => {
                    let (_sm, lir) = compile_to_lir(&file)?;
                    let backend = CBackend;
                    let bytes = backend.emit(&lir)?;
                    let out_path = output.clone().unwrap_or_else(|| {
                        let stem = std::path::Path::new(&file).file_stem()
                            .map(|s| s.to_string_lossy().to_string())
                            .unwrap_or_else(|| "output".to_string());
                        format!("{stem}.c")
                    });
                    std::fs::write(&out_path, &bytes)?;
                    println!("Emitted C source: {out_path}");
                }
                "wasm" => {
                    let (_sm, lir) = compile_to_lir(&file)?;
                    let backend = WasmBackend;
                    let bytes = backend.emit(&lir)?;
                    let out_path = output.clone().unwrap_or_else(|| {
                        let stem = std::path::Path::new(&file).file_stem()
                            .map(|s| s.to_string_lossy().to_string())
                            .unwrap_or_else(|| "output".to_string());
                        format!("{stem}.wat")
                    });
                    std::fs::write(&out_path, &bytes)?;
                    println!("Emitted WAT: {out_path}");
                }
                "llvm" => {
                    let (_sm, lir) = compile_to_lir(&file)?;
                    let backend = LlvmBackend;
                    let bytes = backend.emit(&lir)?;
                    let out_path = output.clone().unwrap_or_else(|| {
                        let stem = std::path::Path::new(&file).file_stem()
                            .map(|s| s.to_string_lossy().to_string())
                            .unwrap_or_else(|| "output".to_string());
                        format!("{stem}.ll")
                    });
                    std::fs::write(&out_path, &bytes)?;
                    println!("Emitted LLVM IR: {out_path}");
                }
                _ => {
                    return Err(format!("Unknown IR level: {level}. Use: tokens, ast, hir, mir, lir, asm, obj, c, wasm, llvm").into());
                }
            }
        }
        Commands::Build { files, entry, output, opt_pass, opt_iterations, verbose_opt, gen_h, shared, py_module } => {
            let mut objects = Vec::new();
            let mut all_bindings = Vec::new();
            let mut lib_name = String::new();
            
            for file in &files {
                let path = std::path::Path::new(file);
                let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
                
                let (name, data) = match ext {
                    "o" | "obj" => {
                        (path.file_name().unwrap().to_string_lossy().to_string(), std::fs::read(file)?)
                    }
                    "a" | "lib" => {
                        (path.file_name().unwrap().to_string_lossy().to_string(), std::fs::read(file)?)
                    }
                    "lit" => {
                        let source = std::fs::read_to_string(file)?;
                        let hir = brak_lang_lit::compile_lit_to_hir(&source, file)?;
                        if gen_h.is_some() || py_module.is_some() {
                            all_bindings.extend(PolyglotBridge::extract_bindings(&hir));
                        }
                        let mut lower_mir = MirLower::new();
                        let mir = lower_mir.lower(hir)?;
                        let mut lower_lir = LirLower::new();
                        lower_lir.set_file_id(0);
                        let mut lir = lower_lir.lower(mir);
                        lir.files = vec![file.to_string()];
                        
                        let mut pm = PassManager::default()
                            .with_iterations(opt_iterations)
                            .with_verbose(verbose_opt);
                        
                        pm.add_pass(Box::new(ConstantFolding));
                        pm.add_pass(Box::new(ConstantPropagation));
                        pm.add_pass(Box::new(Inlining));
                        pm.add_pass(Box::new(GlobalValueNumbering));
                        pm.add_pass(Box::new(DeadCodeElimination));
                        pm.add_pass(Box::new(JumpThreading));
                        pm.add_pass(Box::new(TailCallOptimization));
                        
                        for pass_path in &opt_pass { pm.load_external_pass(pass_path)?; }
                        lir = pm.run(lir)?;
                        let backend = brak_codegen_obj::ObjBackend::default();
                        let obj_bytes = backend.emit(&lir)?;
                        let stem = path.file_stem().unwrap().to_string_lossy().to_string();
                        lib_name = stem.clone();
                        (stem + ".o", obj_bytes)
                    }
                    _ => {
                        let (_sm, hir) = compile_to_hir(file)?;
                        
                        if gen_h.is_some() || py_module.is_some() {
                            all_bindings.extend(PolyglotBridge::extract_bindings(&hir));
                        }

                        let mut lower_mir = MirLower::new();
                        let mir = lower_mir.lower(hir)?;
                        let mut lower_lir = LirLower::new();
                        lower_lir.set_file_id(0);
                        let mut lir = lower_lir.lower(mir);
                        lir.files = vec![file.to_string()];
                        
                        let mut pm = PassManager::default()
                            .with_iterations(opt_iterations)
                            .with_verbose(verbose_opt);

                        let skip_passes = std::env::var("BRK_SKIP_PASSES").unwrap_or_default();
                        
                        if !skip_passes.contains("fold") {
                            pm.add_pass(Box::new(ConstantFolding));
                        }
                        if !skip_passes.contains("cp") {
                            pm.add_pass(Box::new(ConstantPropagation));
                        }
                        if !skip_passes.contains("inline") {
                            pm.add_pass(Box::new(Inlining));
                        }
                        if !skip_passes.contains("gvn") {
                            pm.add_pass(Box::new(GlobalValueNumbering));
                        }
                        if !skip_passes.contains("licm") {
                            pm.add_pass(Box::new(Licm));
                        }
                        if !skip_passes.contains("jt") {
                            pm.add_pass(Box::new(JumpThreading));
                        }
                        if !skip_passes.contains("tco") {
                            pm.add_pass(Box::new(TailCallOptimization));
                        }
                        if !skip_passes.contains("dce") {
                            pm.add_pass(Box::new(DeadCodeElimination));
                        }

                        for pass_path in &opt_pass {
                            pm.load_external_pass(pass_path)?;
                        }

                        lir = pm.run(lir)?;
                        let backend = brak_codegen_obj::ObjBackend::default();
                        let obj_bytes = backend.emit(&lir)?;
                        let stem = path.file_stem().unwrap().to_string_lossy().to_string();
                        lib_name = stem.clone();
                        (stem + ".o", obj_bytes)
                    }
                };
                
                objects.push(ObjectFile { name, data });
            }

            // Generate C header if requested
            if let Some(h_path) = gen_h {
                CHeaderGenerator::generate_file(std::path::Path::new(&h_path), &all_bindings)?;
                println!("Generated C header: {h_path}");
            }

            if shared {
                let out_path = output.clone().unwrap_or_else(|| format!("lib{}.so", lib_name));
                let linker = NativeLinker;
                let base_addr = if cfg!(target_os = "windows") { 0x140000000 } else { 0x400000 };
                let output_exec = linker.link(&objects, &entry, base_addr)?;
                std::fs::write(&out_path, &output_exec.data)?;
                println!("Built shared library: {out_path}");
            }

            if let Some(ref mod_name) = py_module {
                let out_dir = output.clone().unwrap_or_else(|| format!("py_{mod_name}"));
                let proj_dir = std::path::Path::new(&out_dir);
                std::fs::create_dir_all(proj_dir.join("src"))?;

                let lib_so = format!("lib{lib_name}.so");
                println!("Generating PyO3 module '{mod_name}' in {out_dir}/");
                println!("  Brak object: {} files compiled", objects.len());

                // Write Cargo.toml
                let cargo_toml = format!(
                    r#"[package]
name = "{mod_name}"
version = "0.1.0"
edition = "2021"

[lib]
name = "{mod_name}"
crate-type = ["cdylib"]

[dependencies]
pyo3 = {{ version = "0.23", features = ["extension-module"] }}

[build-dependencies]
cc = "1"
"#,
                    mod_name = mod_name
                );
                std::fs::write(proj_dir.join("Cargo.toml"), &cargo_toml)?;

                // Write pyproject.toml
                std::fs::write(
                    proj_dir.join("pyproject.toml"),
                    &PyO3Generator::generate_pyproject(&mod_name),
                )?;

                // Write build.rs — links the compiled Brak object
                let build_rs = format!(
                    r#"fn main() {{
    let obj_path = std::path::Path::new("{lib_so}");
    if obj_path.exists() {{
        println!("cargo:rustc-link-search=native=.");
        println!("cargo:rustc-link-lib={lib_name}");
    }}
    // If no prebuilt library, compile from object
    let obj_file = format!("{lib_name}.o");
    if std::path::Path::new(&obj_file).exists() {{
        cc::Build::new()
            .object(&obj_file)
            .compile("{lib_name}");
    }}
}}
"#,
                    lib_so = lib_so,
                    lib_name = lib_name
                );
                std::fs::write(proj_dir.join("build.rs"), &build_rs)?;

                // Write src/lib.rs — generated bindings
                let lib_rs = PyO3Generator::generate_string(&mod_name, &all_bindings);
                std::fs::write(proj_dir.join("src").join("lib.rs"), &lib_rs)?;

                println!("Generated PyO3 module in {out_dir}/");
                println!("  Build with: cd {out_dir} && maturin develop");
                println!("  Or: cd {out_dir} && cargo build --release");
            }

            if !shared && py_module.is_none() {
                let linker = NativeLinker;
                let base_addr = if cfg!(target_os = "windows") { 0x140000000 } else { 0x400000 };
                let output_exec = linker.link(&objects, &entry, base_addr)?;

                let out_path = output.unwrap_or_else(|| {
                    let stem = std::path::Path::new(&files[0]).file_stem()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_else(|| "output".to_string());
                    match output_exec.format {
                        "pe" => format!("{stem}.exe"),
                        "macho" => stem,
                        _ => stem,
                    }
                });
                std::fs::write(&out_path, &output_exec.data)?;
                println!("Built: {out_path} (format: {}) using {} objects", output_exec.format, objects.len());
            }
        }
    }

    Ok(())
}
