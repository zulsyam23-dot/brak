# Daftar API Brak — Catatan Penting

> **Sumber data**: dokumen ini diambil langsung dari kode sumber yang ada saat ini
> (30 crate di workspace) dan diverifikasi dengan menjalankan binary `target/release/brak.exe`
> (output `--help`, build + run `samples/hello.brk` → exit code 42).
> Tidak ada klaim dokumentasi yang tidak didukung kode.

## Isi
1. [Pipeline kompilasi](#1-pipeline-kompilasi)
2. [CLI `brak`](#2-cli-brak)
3. [API library per crate](#3-api-library-per-crate)
4. [Contoh program lengkap](#4-contoh-program-lengkap)
5. [Status kejujuran](#5-status-kejujuran)

---

## 1. Pipeline kompilasi

Alur data nyata (dari `brak-tool/src/main.rs` dan `brak-easy/src/lib.rs`):

```
.brk/.lit
   → brak-frontend (AsciiLexer → Token → Parser → Program/AST)
   → brak-ir-hir (HirLower → HirProgram, lalu TypeChecker)
   → brak-ir-mir (MirLower → MirProgram, CFG ber-block)
   → brak-ir-lir (LirLower → LirProgram, register virtual)
   → brak-opt-* (PassManager)
   → brak-codegen-obj (ObjBackend → .o)
   → brak-link-native (NativeLinker → .exe / .dll)
```

Satu pintu masuk paling simpel: **`brak-easy::EasyPipeline`** (`build_executable`).

---

## 2. CLI `brak`

Dibangun dengan `clap` di crate `brak-tool` (binary bernama `brak` / `brak.exe`).
Build: `cargo build --release` lalu pakai `target/release/brak`.

```
Usage: brak.exe <COMMAND>

Commands:
  emit-ir  Emit intermediate representation
  build    Build a standalone executable or shared library
  help     Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
```

### 2.1 `brak emit-ir <FILE>`

Mencetak IR level tertentu dari sebuah file sumber.

| Flag | Nilai | Default |
|------|-------|---------|
| `-l, --level <LEVEL>` | `tokens`, `ast`, `hir`, `mir`, `lir`, `asm`, `obj`, `c`, `wasm`, `llvm` * | `ast` |
| `-f, --format <FORMAT>` | `text`, `json`, `yaml` (diabaikan untuk asm/obj) | `text` |
| `-o, --output <OUTPUT>` | file tujuan (default: stdout untuk text, `<file>.o` untuk obj) | — |

*Catatan: `--help` menampilkan `tokens, ast, hir, mir, lir, asm, obj`, tapi kode
(`brak-tool/src/main.rs:126-238`) juga menerima `c`, `wasm`, `llvm`.

Contoh terverifikasi:
```bash
brak emit-ir samples/hello.brk --level hir
# fn main() -> i32 {
#   42;
# }

brak emit-ir samples/hello.brk --level ast --format json
brak emit-ir samples/hello.brk --level lir
brak emit-ir samples/hello.brk --level c    --output hello.c
brak emit-ir samples/hello.brk --level llvm --output hello.ll
brak emit-ir samples/hello.brk --level wasm --output hello.wat
```

### 2.2 `brak build <FILES>...`

Mengompilasi file sumber **dan/atau** file objek/archive lalu me-link jadi executable
atau shared library.

| Flag | Nilai | Default |
|------|-------|---------|
| `FILES...` | `.brk`, `.lit`, `.o`/`.obj`, `.a`/`.lib` (wajib ≥1) | — |
| `-e, --entry <ENTRY>` | nama fungsi entry | `main` |
| `-o, --output <OUTPUT>` | file keluaran (default: nama file input pertama) | — |
| `--opt-pass <PATH>` | path plugin pass optimasi dinamis (`.so`, `.dll`, `.dylib`) | — |
| `--opt-iterations <N>` | berapa kali pipeline optimasi dijalankan | `1` |
| `--verbose-opt` | cetak log optimasi detail | off |
| `--gen-h <PATH>` | hasilkan C header untuk fungsi publik | — |
| `--shared` | build DLL/shared library (bukan executable) | off |
| `--py-module <NAME>` | hasilkan proyek PyO3 extension module dengan nama ini | — |

Catatan perilaku nyata (`brak-tool/src/main.rs`):
- Input `.brk`: pipeline penuh + **8 pass** Fold, CP, Inline, GVN, LICM, JT, TCO, DCE
  (DCE dilewati untuk `--shared`). Pass bisa dimatikan via env `BRK_SKIP_PASSES`
  berisi nama pass dipisah koma (mis. `BRK_SKIP_PASSES=licm,jt`).
- Input `.lit`: pipeline **6 pass** (tanpa LICM).
- Input archive `.a`/`.lib` di-unpack ke member object-nya.
- Exit code executable = nilai return fungsi entry (terverifikasi: `hello.brk`
  return 42 → shell exit 42).

Contoh:
```bash
# Executable
brak build samples/hello.brk -o hello.exe
brak build main.brk lib.math.a -e main -o app.exe

# DLL
brak build math.brk --shared -o math.dll

# C header
brak build math.brk --gen-h math.h --shared

# Python extension (menghasilkan folder proyek PyO3)
brak build math.brk --py-module mathlib -o py_mathlib/
```

> Keterbatasan nyata: dengan `--shared` di Linux, linker hanya menghasilkan error
> jelas (ELF ET_DYN belum diimplementasi). DLL Windows didukung penuh.

---

## 3. API library per crate

Semua crate cocok untuk dipakai sebagai dependensi Rust (`path = "crate/..."` di
workspace). Berikut API publik aktualnya.

### 3.1 `brak-core` — tipe dasar & trac

```rust
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
pub struct Version { pub major: u16, pub minor: u16, pub patch: u16 }
pub const BRAK_VERSION: Version = Version::new(0, 1, 0);

// mod span
pub struct SourceLoc { pub line, pub column, pub offset }   // SourceLoc::new(l,c,o) const
pub struct Span { pub start: SourceLoc, pub end: SourceLoc } // Span::new(a,b) const
pub const DUMMY_SPAN: Span;
pub struct SourceMap { pub filename, pub source, pub lines }
impl SourceMap {
    pub fn new(filename, source) -> Self;
    pub fn loc_at(&self, offset) -> Option<SourceLoc>;
    pub fn span_at(&self, start, end) -> Option<Span>;
}

// mod diagnostic
pub enum Severity { Error, Warning, Note, Help }
pub struct Diagnostic { pub severity, pub message, pub span, pub notes, pub help }
impl Diagnostic { pub fn error(m), pub fn warning(m), pub fn with_span(s), pub fn with_note(n), pub fn with_help(h) }
pub struct Diagnostics { pub entries: Vec<Diagnostic> }
impl Diagnostics { pub fn new(), pub fn push(d), pub fn has_errors() -> bool, pub fn extend(o) }
// Diagnostics implements std::error::Error + Display

// mod content_hash
pub trait ContentHash { fn content_hash(&self) -> u64; }
pub fn combine_hash(a: u64, b: u64) -> u64;
```

### 3.2 `brak-frontend` — lexer & parser

```rust
// mod lexer
pub enum TokenKind { Ident, Number, String, /* semua operator+keyword, */ Eof }
pub struct Token { pub kind: TokenKind, pub lexeme: String, pub span: Span }
impl Token { pub fn new(kind, lexeme, span) -> Self }
pub trait BrakLexer: Send + Sync {
    fn lex(&mut self, source: &SourceMap) -> Vec<Token>;
    fn reset(&mut self, source: &SourceMap);
}
pub struct AsciiLexer;
impl AsciiLexer { pub fn new() -> Self }
impl BrakLexer for AsciiLexer {}

// mod parser
pub struct Parser;
impl Parser {
    pub fn new() -> Self;
    pub fn parse(self, tokens: &[Token]) -> Result<Program, Diagnostics>;
}
```

Pemakaian minimal:
```rust
use brak_core::SourceMap;
use brak_frontend::lexer::{AsciiLexer, BrakLexer};
use brak_frontend::parser::Parser;

let sm = SourceMap::new("main.brk", "fn main() -> i32 { 42 }");
let tokens = AsciiLexer::new().lex(&sm);
let ast = Parser::new().parse(&tokens)?;   // brak_ir_ast::ast::Program
```

### 3.3 `brak-ir-ast` — AST

```rust
pub mod ast;   // Program, Item, StructDef, Field, EnumDef, Variant, TraitDef,
               // ImplDef, UseStmt, ModDef, ConstDef, StaticDef, Visibility,
               // ExternFn, FnDef, Param, Let, Block, Stmt, Expr, MatchArm,
               // Pattern, BinOp, UnOp, Ident, Type
struct Program { pub items: Vec<Item> }
enum Item { FnDef, ExternFn, Let, Struct, Enum, Use, Mod, Const, Static, Trait, Impl }
enum Expr { Int(i64, Span), Float(f64, Span), String(String, Span), Bool(bool, Span),
            Ident(Ident), Call{..}, BinOp{op,lhs,rhs,span}, UnOp{op,expr,span},
            Assign{..}, If{..}, Match{..}, Block(Block), StructInit{..}, EnumCons{..}, Field{..} }
enum Pattern { Wildcard(Span), Ident(Ident), Literal(Expr), Variant{enum_name,variant,bindings,span} }
enum Type { I32, I64, F32, F64, Bool, String, Void, Named(String), Ptr(Box), Ref(Box), Array(Box, usize), Slice(Box), Fn(Vec, Box) }
```
Semua node `Serialize` + `Deserialize` (serde). AST ini langsung bisa di-lower
lanjut levat HIR atau diproses manual.

### 3.4 `brak-ir-hir` — HIR, lowering & typeck

```rust
// mod heading: HirProgram, HirItem, HirStruct, HirField, HirEnum, HirVariant,
//              HirExternFunction, HirFunction, HirParam, HirGlobalLet, HirBlock,
//              HirStmt, HirExpr, HirPattern, HirLiteral, HirBinOp, HirUnOp, HirType

// mod lower
pub struct HirLower;
impl HirLower {
    pub fn new() -> Self;
    pub fn lower(&self, program: ast::Program) -> Result<HirProgram, Diagnostics>;
    pub fn lower_block(&self, block: ast::Block) -> HirBlock;
    pub fn lower_expr(&self, expr: ast::Expr) -> HirExpr;
}

// mod typeck
pub struct TypeChecker;
impl TypeChecker {
    pub fn new() -> Self;
    pub fn check(&mut self, program: &HirProgram) -> Result<(), Diagnostics>;
}
```
`HirPattern` adalah enum nyata: `Wildcard`, `Binding(Ident)`, `Literal(HirLiteral)`,
`Variant { .. }` — bukan string (dasar match yang berfungsi).

### 3.5 `brak-ir-mir` — CFG mid-level

```rust
pub type LocalId = usize;
pub type BlockId = usize;

pub struct MirProgram { pub functions, pub extern_functions, pub structs, pub enums }
pub struct MirFunction { pub name, pub params: Vec<LocalId>, pub ret_ty, pub blocks, pub locals, pub span }
pub struct MirBlock { pub id, pub name, pub insts, pub terminator, pub span }
pub enum MirInst { Assign { dest, value, span }, Call { dest, callee, args, span } }
pub enum MirTerminator { Return{..}, Jump{target}, Branch{cond,then,else_}, Unreachable }
pub enum MirValue { Local, Int, Float, Bool, String, BinOp, UnOp, GetField, StructInit, SetField, EnumInit }
pub enum MirBinOp { Add, Sub, Mul, Div, Mod, FAdd, FSub, FMul, FDiv, Eq, Ne, Lt, Le, Gt, Ge, And, Or, BitAnd, BitOr, BitXor, Shl, Shr }
pub enum MirUnOp { Neg, Not, BitNot }
pub enum MirType { I32, I64, F32, F64, Bool, String, Void, Named(String) }

// mod lower
pub struct MirLower;
impl MirLower { pub fn new() -> Self; pub fn lower(&mut self, program: HirProgram) -> Result<MirProgram, Diagnostics>; }
```

### 3.6 `brak-ir-lir` — low-level register IR

```rust
pub type VirtReg = usize;
pub type BlockId = usize;

pub struct LirProgram { pub functions, pub extern_functions, pub structs, pub enums, pub string_table, pub files }
pub struct LirFunction { pub name, pub params: Vec<VirtReg>, pub blocks, pub reg_count, pub span }
pub struct LirBlock { pub id, pub name, pub insts, pub span }
pub struct LirInst { pub opcode, pub dest: Option<VirtReg>, pub operands, pub call_conv, pub debug: Span, pub file_id }
pub enum LirOpcode { Mov, Add, Sub, Mul, Div, Mod, FAdd, FSub, FMul, FDiv, Neg, Not,
    And, Or, Xor, Shl, Shr, Cmp, SetEq, SetNe, SetLt, SetLe, SetGt, SetGe,
    Load, Store, Alloca, Call, Ret, Jmp, Br, Push, Pop, Comment, GetField, StructInit, SetField }
pub enum LirOperand { Reg(VirtReg), ImmI64(i64), ImmF64(f64), Label(String), StackSlot(u32), StringRef(usize), Field(String) }
pub enum CallingConvention { Brak, Cdecl, SystemV, Win64 }
pub enum LirType { I32, I64, F32, F64, Bool, String, Void, Named(String), Ptr(Box) }

impl LirInst { pub fn new(op) -> Self; pub fn with_call_conv(cc); pub fn with_dest(vreg);
              pub fn with_op(op); pub fn with_ops(ops); pub fn with_debug(span); pub fn with_file(id); }
```
Builder `with_*` memudahkan membuat LIR manual (untuk backend/optimasi sendiri).
Semua tipe LIR memiliki `Display` → output teks `brak emit-ir`.

// mod lower
```rust
pub struct LirLower;
impl LirLower { pub fn new() -> Self; pub fn set_file_id(&mut self, id); pub fn lower(&mut self, program: MirProgram) -> LirProgram; }
```

### 3.7 `brak-opt-traits` — framework optimasi

```rust
pub trait LirOptimizationPass: Send + Sync {
    fn name(&self) -> &'static str;
    fn run(&self, program: LirProgram) -> Result<LirProgram>;
}

pub struct PassManager { pub max_iterations: usize, pub verbose: bool }
impl PassManager {
    pub fn default() -> Self;                        // max_iterations=1
    pub fn with_iterations(n) -> Self;
    pub fn with_verbose(bool) -> Self;
    pub fn add_pass(&mut self, pass: Box<dyn LirOptimizationPass>);
    pub fn load_external_pass(&mut self, path: &str) -> Result<()>;
    pub fn run(&self, program: LirProgram) -> Result<LirProgram>;  // loop sampai konvergen
}
```
Plugin eksternal: library yang mengekspor `pub extern "Rust" fn create_pass() -> Box<dyn LirOptimizationPass>`
(bisa dimuat via `--opt-pass` di CLI).

### 3.8 Pass optimasi (`brak-opt-*`)

Pass yang sudah ada, semuanya mengimplementasikan `LirOptimizationPass`:

| Crate | Struct | Fungsi |
|-------|--------|--------|
| `brak-opt-fold` | `ConstantFolding` | lipat ekspresi konstanta (`x+0`, `1*x`, dll.) |
| `brak-opt-cp` | `ConstantPropagation` | dataflow path-sensitive, lattice Top/Known/Ambig |
| `brak-opt-inline` | `Inlining` | inline fungsi kecil (<20 inst; skip self-rekursif) |
| `brak-opt-gvn` | `GlobalValueNumbering` | eliminasi perhitungan redundan (komutatif dikanonikalisasi) |
| `brak-opt-licm` | `Licm` | hoist loop-invariant (whitelist ketat, pre-header eksplisit) |
| `brak-opt-jt` | `JumpThreading` | lompatan berantai |
| `brak-opt-tco` | `TailCallOptimization` | self tail-call → loop (rekursi 500k aman) |
| `brak-opt-dce` | `DeadCodeElimination` | hapus fungsi mati (Div/Mod dihitung side-effect) |

Contoh urutan standar CLI (lihat `brak-tool/src/main.rs`):
```rust
let mut pm = PassManager::default().with_iterations(1).with_verbose(false);
pm.add_pass(Box::new(ConstantFolding));
pm.add_pass(Box::new(ConstantPropagation));
pm.add_pass(Box::new(Inlining));
pm.add_pass(Box::new(GlobalValueNumbering));
pm.add_pass(Box::new(Licm));
pm.add_pass(Box::new(JumpThreading));
pm.add_pass(Box::new(TailCallOptimization));
pm.add_pass(Box::new(DeadCodeElimination));
let lir = pm.run(lir)?;
```

### 3.9 `brak-opt-utils` — analisis CFG bersama

```rust
pub struct CfgGraph;                                        // domain analisis
pub fn build_cfg(func: &LirFunction) -> CfgGraph;           // resolve label by id & nama
pub struct Dominance;
pub fn compute_dominance(func: &LirFunction, cfg: &CfgGraph) -> Dominance;
pub fn dominates(dom: &Dominance, a: BlockId, b: BlockId) -> bool;
pub struct NaturalLoop;
pub fn find_natural_loops(func: &LirFunction, cfg: &CfgGraph, dom: &Dominance) -> Vec<NaturalLoop>;
```
Dipakai oleh pass yang butuh informasi kontrol flow.

### 3.10 `brak-codegen-traits` — backend contract

```rust
pub trait CodegenBackend: Send + Sync {
    fn name(&self) -> &'static str;
    fn emit(&self, program: &LirProgram) -> Result<Vec<u8>>;
}
pub trait CodegenExecutable: CodegenBackend {
    fn emit_executable(&self, program: &LirProgram, entry: &str) -> Result<Vec<u8>>;
}
```

### 3.11 Backend codegen (`brak-codegen-*`)

| Crate | Struct | Fungsi bebas | Output |
|-------|--------|--------------|--------|
| `brak-codegen-obj` | `ObjBackend` (+ `ObjectFormat` enum) | `emit_obj(&LirProgram)` | `.o`/`.obj` (ELF/COFF/Mach-O sesuai host) — **backend utama CLI** |
| `brak-codegen-c` | `CBackend` | `emit_c(&LirProgram) -> String` | C source |
| `brak-codegen-wasm` | `WasmBackend` | `emit_wasm(&LirProgram) -> String` | **WAT text**, bukan binary |
| `brak-codegen-llvm` | `LlvmBackend` | `emit_llvm(&LirProgram) -> String` | LLVM IR `.ll` |
| `brak-codegen-asm` | `AsmBackend` (+ mod `x86_64`, `regalloc`) | `emit_asm(&LirProgram) -> String` | teks assembly |

`codegen-obj` juga mengekspos modul: `elf` (`write_elf`, `write_elf_executable`),
`coff` (`write_coff`), `macho_obj` (`write_macho`), `dwarf` (`build_dwarf`),
`codeview` (`build_codeview`), `x86_64` (peng-encode + `native_call_conv()`).

> Catatan: `brak-codegen-asm` saat ini tidak punya consumer di pipeline manapun
> (dead code menurut prioritas.md); `ObjBackend` adalah backend yang dipakai CLI dan
> `brak-easy`.

### 3.12 `brak-link-traits` — linker contract

```rust
pub struct ObjectFile { pub name: String, pub data: Vec<u8> }
pub struct LinkerOutput { pub data: Vec<u8>, pub format: &'static str }

pub trait LinkerBackend: Send + Sync {
    fn name(&self) -> &'static str;
    fn link(&self, objects: &[ObjectFile], entry: &str, base_addr: u64) -> Result<LinkerOutput>;
}
```

### 3.13 `brak-link-native` — linker executable

```rust
pub struct NativeLinker;
impl NativeLinker {
    pub fn link(&self, objects: &[ObjectFile], entry: &str, base_addr: u64) -> Result<LinkerOutput>;   // exe
    pub fn link_shared(&self, objects: &[ObjectFile], base_addr: u64) -> Result<LinkerOutput>;         // DLL
}
// mod elf :  link_elf(objects, entry, base) -> Result<LinkerOutput>
// mod pe  :  link_pe(...),  link_pe_shared(...)
// mod macho: link_macho(...)
// mod parse: parse_elf, parse_macho, parse_coff, build_global_sym_map, resolve_sym_addr,
//            find_entry_offset, apply_reloc, apply_reloc_with_addend, merge_text, apply_all_relocs, ...
```
Mendeteksi format object (`PE`/`ELF`/`Mach-O`/`COFF`) dan memproduksi executable
atau DLL tanpa tool eksternal (zero-dependency). Base address default:
Windows exe `0x140000000`, Linux exe/DLL `0x400000`, Windows DLL `0x180000000`.

### 3.14 `brak-link-wasm` & `brak-link-archive`

```rust
// wasm
pub struct WasmLinker;
pub fn link_wasm(objects: &[ObjectFile], entry: &str) -> Result<LinkerOutput>;

// archive
pub struct ArchiveEntry { pub name: String, pub data: Vec<u8> }
pub enum ArchiveFormat { Unix, Windows }
pub struct ArchiveWriter;
impl ArchiveWriter { pub fn new(format) -> Self; pub fn add_entry(&mut self, name: String, data: Vec<u8>); pub fn write(&self) -> Result<Vec<u8>>; }
pub fn parse_archive(data: &[u8]) -> Result<Vec<ArchiveEntry>>;
```
`parse_archive` dipakai CLI untuk menerima input `.a`/`.lib`.

### 3.15 `brak-easy` — pipeline level tinggi (paling disarankan untuk dipakai)

```rust
pub enum OptLevel { None, Less, Default, Aggressive }   // Aggressive = 4 iterasi

pub struct EasyPipeline;
impl EasyPipeline {
    pub fn new() -> Self;
    pub fn with_opt_level(self, level: OptLevel) -> Self;
    pub fn without_pass(self, name: &'static str) -> Self;      // matikan pass by name
    pub fn with_verbose(self, bool) -> Self;
    pub fn with_iterations(self, usize) -> Self;
    pub fn with_entry_point(self, entry: &str) -> Self;         // default "main"
    pub fn build_executable(&self, name: &str, source: &str, output_path: &str) -> BrakResult<()>;
    pub fn compile_to_lir(&self, name: &str, source: &str) -> BrakResult<LirProgram>;
    pub fn ast_to_lir(&self, name: &str, ast: Program) -> BrakResult<LirProgram>;
    pub fn compile_to_object(&self, name: &str, source: &str) -> BrakResult<Vec<u8>>;
    pub fn lir_to_executable(&self, name: &str, lir: LirProgram, output_path: &str) -> BrakResult<()>;
}
```
> Penting: default pipeline `brak-easy` hanya 5 pass
> `inline, cp, fold, gvn, dce` (satu iterasi) — **bukan** 8 pass seperti CLI.

### 3.16 `brak-polyglot` — FFI ke C & Python

```rust
pub enum ForeignType { ... }                       // mapping tipe ke luar (C/Python)
pub struct FfiBinding;                             // satu fungsi publik ter-bound
impl FfiBinding { pub fn to_c_declaration(&self) -> String; }

pub struct PolyglotBridge;
impl PolyglotBridge {
    pub fn brak_to_c(brak_ty: &BrakType) -> ForeignType;
    pub fn hir_to_c(hir_ty: &HirType) -> ForeignType;
    pub fn c_to_brak(foreign_ty: &ForeignType) -> Option<BrakType>;
    pub fn extract_bindings(program: &HirProgram) -> Vec<FfiBinding>;
}

pub struct CHeaderGenerator;
impl CHeaderGenerator {
    pub fn generate_string(bindings: &[FfiBinding]) -> String;
    pub fn generate_file(path: &Path, bindings: &[FfiBinding]) -> std::io::Result<()>;
}

pub struct PyO3Generator;
impl PyO3Generator {
    pub fn generate_string(module_name: &str, bindings: &[FfiBinding]) -> String;
    pub fn generate_project(module_name: &str, bindings: &[FfiBinding], output_dir: &Path) -> std::io::Result<()>;
    pub fn generate_pyproject(module_name: &str) -> String;
}
```
Di CLI, alur FFI: `brak build file.brk --gen-h file.h` (header C) atau
`--py-module nama` (buat proyek PyO3 + maturin).

### 3.17 `brak-bitcode` — cache IR (eksperimental)

```rust
pub struct BitcodeCache;
impl BitcodeCache {
    pub fn new(path: impl Into<PathBuf>) -> Self;
    pub fn get_or_compute_ast(&self, hash: u64, f: impl FnOnce() -> Program) -> Program;
    pub fn get_or_compute_hir(&self, hash: u64, f: impl FnOnce() -> HirProgram) -> HirProgram;
    pub fn get_or_compute_mir(&self, hash: u64, f: impl FnOnce() -> MirProgram) -> MirProgram;
    pub fn get_or_compute_lir(&self, hash: u64, f: impl FnOnce() -> LirProgram) -> LirProgram;
    pub fn contains(&self, level: &str, hash: u64) -> bool;
    pub fn invalidate(&self, level: &str, hash: u64) -> Result<()>;
    pub fn clear_all(&self) -> Result<()>;
}
```
> Eksperimental: belum terintegrasi ke CLI maupun `brak-easy`.

### 3.18 `brak-lang-lit` — bahasa alternatif Lit

```rust
pub struct LitError;
pub fn compile_lit_to_hir(source: &str, path: &str) -> Result<HirProgram, LitError>;
```
Syntax Lit saat ini hanya fungsi konstanta:
```lit
fn versi() -> i32 = 42;
fn salam() -> string = "Halo dari Lit";
```
(no `let`, no body `{}`, no pemanggilan fungsi — lihat `docs/LANG_LIT.md`).

### 3.19 `brak-test` — alat bantu testing compiler

```rust
pub struct DiagnosticTester;
impl DiagnosticTester { pub fn assert_has_error(diags, msg) -> Result<()>; pub fn assert_has_warning(diags, msg) -> Result<()>; }

pub struct ExecutionTester;
impl ExecutionTester { pub fn assert_output(exe_path: &Path, expected: &str) -> Result<()>; }

pub struct SnapshotTester;
impl SnapshotTester { pub fn new(snapshot_dir: impl AsRef<Path>, update: bool) -> Self; pub fn assert_snapshot<T: Serialize>(&self, name: &str, ir: &T) -> Result<()>; }
```

### 3.20 `brak-codegen-obj` — detail internal

- `pub enum ObjectFormat` — format object yang dipilih.
- `pub struct ObjBackend` — `Default`, implement `CodegenBackend`.
- Level obj di `emit-ir` memakai backend obj ini.
- Dukungan opcode obj backend sekarang lengkap untuk aritmetika, perbandingan,
  control flow, struct (`StructInit`/`GetField`/`SetField`), `Load`/`Store`;
  opcode yang belum didukung menghasilkan error `CodegenError::Unsupported`
  (bukan silent). String literal → `StringRef` = error eksplisit saat ini.

---

## 4. Contoh program lengkap

```rust
// Cargo.toml gunakan workspace: pustaka-pustaka brak via path relatif
use brak_easy::{EasyPipeline, OptLevel};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let src = r#"
fn main() -> i32 {
    let x: i32 = 10;
    let y: i32 = 20;
    x + y
}
"#;
    EasyPipeline::new()
        .with_opt_level(OptLevel::Default)
        .build_executable("app", src, "app.exe")?;
    Ok(())
}
```

Pipeline manual (level demi level):
```rust
use brak_core::SourceMap;
use brak_frontend::lexer::{AsciiLexer, BrakLexer};
use brak_frontend::parser::Parser;
use brak_ir_hir::lower::HirLower;
use brak_ir_hir::typeck::TypeChecker;
use brak_ir_mir::lower::MirLower;
use brak_ir_lir::lower::LirLower;
use brak_codegen_traits::CodegenBackend;
use brak_codegen_obj::ObjBackend;
use brak_link_traits::LinkerBackend;
use brak_link_native::NativeLinker;

let src = "fn main() -> i32 { 42 }";
let sm = SourceMap::new("main.brk", src);
let tokens = AsciiLexer::new().lex(&sm);
let ast = Parser::new().parse(&tokens)?;

let hir = HirLower::new().lower(ast).map_err(|e| e.to_string())?;
TypeChecker::new().check(&hir)?;

let mir = MirLower::new().lower(hir)?;
let mut lower = LirLower::new();
lower.set_file_id(0);
let mut lir = lower.lower(mir);
lir.files = vec!["main.brk".to_string()];

let obj = ObjBackend::default().emit(&lir)?;
let exe = NativeLinker.link(
    &[brak_link_traits::ObjectFile { name: "main.o".into(), data: obj }],
    "main",
    0x140000000,
)?;
std::fs::write("out.exe", exe.data)?;
```

Menulis pass optimasi custom:
```rust
use brak_core::Result;
use brak_ir_lir::lir::LirProgram;
use brak_opt_traits::{LirOptimizationPass, PassManager};

struct MyPass;
impl LirOptimizationPass for MyPass {
    fn name(&self) -> &'static str { "my_pass" }
    fn run(&self, program: LirProgram) -> Result<LirProgram> { Ok(program) } // logika anda
}

// pasang:
let mut pm = PassManager::default();
pm.add_pass(Box::new(MyPass));
lir = pm.run(lir)?;
```

API untuk **backend baru** (contoh penerapan trait):
```rust
use brak_core::Result;
use brak_ir_lir::lir::LirProgram;
use brak_codegen_traits::CodegenBackend;

struct MyBackend;
impl CodegenBackend for MyBackend {
    fn name(&self) -> &'static str { "my" }
    fn emit(&self, program: &LirProgram) -> Result<Vec<u8>> { Ok(vec![]) }
}
```

---

## 5. Status kejujuran

Catatan agar tidak terjadi kesalahpahaman saat memakai API:

| Hal | Status nyata |
|-----|--------------|
| `brak build` | Bekerja penuh untuk `.brk` → exe; `.lit` didukung; input `.o`/`.a` didukung. Exit code = nilai return entry. Terverifikasi. |
| `--shared` | DLL Windows didukung (export table nyata). Linux → error jelas (ELF DYN belum). |
| `--py-module` | Membuat proyek PyO3 yang siap di-build dengan maturin/cargo. |
| Pipeline default `brak-easy` | 5 pass (inline, cp, fold, gvn, dce) 1 iterasi. |
| Pipeline CLI `brak build` | 8 pass (tambah licm, jt, tco) 1 iterasi. |
| Backend output utama | `brak-codegen-obj` (ELF/COFF/Mach-O) + `brak-link-native`. |
| `brak-codegen-wasm` | Output **WAT text** (`.wat`), bukan binary `.wasm`; butuh wat2wasm. |
| `brak-codegen-asm` | Ada tapi **tidak dipakai** pipeline (dead code). |
| `brak-bitcode` | **Eksperimental**, belum terintegrasi CLI/brak-easy. |
| Bahasa Lit | Hanya fungsi konstanta (`fn x() -> ty = lit;`). |
| `brk gen-h` (C header) | Berfungsi via `--gen-h`. Header dari fungsi yang diekstrak `extract_bindings`. |
| Dukungan tipe | `i32 i64 f32 f64 bool string void` + Named struct/enum. Array/Slice di-parse tapi belum usable end-to-end (indexing postfix belum ada). |
| Float | Operasi `+ - * /` float didukung (FAdd/FSub/FMul/FDiv). Perbandingan float memakai perbandingan bit-pattern i64 (akurat untuk nilai normal). |
| Struct/Enum | Parsing, lowering, codegen struct dan enum (konstruksi + field) tersedia. |

> Dokumen ini mengikuti keadaan kode sekarang. Bila ingin menyelaraskan
> `prioritas.md` / `prd.md` / README dengan realita di atas, lihat bagian "Level D —
> DOCS vs REALITAS" di `prioritas.md`.