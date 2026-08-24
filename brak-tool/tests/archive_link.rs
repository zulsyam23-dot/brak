use brak_frontend::lexer::{AsciiLexer, BrakLexer};
use brak_frontend::parser::Parser as BrakParser;
use brak_ir_hir::lower::HirLower;
use brak_ir_hir::typeck::TypeChecker;
use brak_ir_mir::lower::MirLower;
use brak_ir_lir::lower::LirLower;
use brak_codegen_obj::ObjBackend;
use brak_codegen_traits::CodegenBackend;
use brak_link_archive::{ArchiveWriter, ArchiveFormat, parse_archive};
use brak_link_traits::{LinkerBackend, ObjectFile};
use brak_link_native::NativeLinker;

fn compile_to_object(src: &str) -> Vec<u8> {
    let sm = brak_core::SourceMap::new("t.brk", src);
    let mut lexer = AsciiLexer::new();
    let tokens = lexer.lex(&sm);
    let ast = BrakParser::new().parse(&tokens).unwrap();
    let hir = HirLower::new().lower(ast).unwrap();
    let mut tc = TypeChecker::new();
    tc.check(&hir).unwrap();
    let mir = MirLower::new().lower(hir).unwrap();
    let lir = LirLower::new().lower(mir);
    ObjBackend::default().emit(&lir).unwrap()
}

/// BUG-H03 regression: an `.a` input must be unpacked into member objects and
/// linked, not fed raw to the ELF/COFF parser.
#[test]
fn link_executable_from_archive() {
    let lib = compile_to_object(
        "fn add(a: i32, b: i32) -> i32 { return a + b; }",
    );
    let main = compile_to_object(
        "extern fn add(a: i32, b: i32) -> i32;\nfn main() -> i32 { return add(20, 10); }",
    );

    // Wrap the library object into an archive (with symbol index member).
    let mut writer = ArchiveWriter::new(ArchiveFormat::Unix);
    writer.add_entry("add.o".to_string(), lib);
    let archive_bytes = writer.write().unwrap();

    // Unpack exactly as brak-tool does now.
    let mut objects: Vec<ObjectFile> = parse_archive(&archive_bytes).unwrap()
        .into_iter()
        .map(|m| ObjectFile { name: m.name, data: m.data })
        .collect();
    objects.push(ObjectFile { name: "main.o".to_string(), data: main });

    let out = NativeLinker.link(&objects, "main", 0x140000000).expect("link from archive");
    assert!(!out.data.is_empty());
}
