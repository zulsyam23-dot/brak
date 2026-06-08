use brak_codegen_traits::CodegenBackend;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: brak-lit <input.lit>");
        std::process::exit(1);
    }

    let input = &args[1];
    let source = std::fs::read_to_string(input).unwrap_or_else(|e| {
        eprintln!("Error reading {input}: {e}");
        std::process::exit(1);
    });

    let hir = match brak_lang_lit::compile_lit_to_hir(&source, input) {
        Ok(h) => h,
        Err(e) => {
            eprintln!("Error: {e}");
            std::process::exit(1);
        }
    };

    let mut lower_mir = brak_ir_mir::lower::MirLower::new();
    let mir = lower_mir.lower(hir).unwrap_or_else(|d| {
        eprintln!("MIR errors: {d:?}");
        std::process::exit(1);
    });

    let mut lower_lir = brak_ir_lir::lower::LirLower::new();
    let lir = lower_lir.lower(mir);

    let backend = brak_codegen_obj::ObjBackend::default();
    let obj_bytes = backend.emit(&lir).unwrap_or_else(|e| {
        eprintln!("Codegen error: {e}");
        std::process::exit(1);
    });

    let out_path = std::path::Path::new(input).with_extension("o");
    std::fs::write(&out_path, &obj_bytes).unwrap_or_else(|e| {
        eprintln!("Error writing {out_path:?}: {e}");
        std::process::exit(1);
    });
    println!("Written: {out_path:?}");
}
