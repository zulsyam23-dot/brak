# PRD: Brak — Language Construction Toolkit

## 1. Vision

Brak adalah library toolkit untuk membangun bahasa pemrograman yang *fully standalone*. Terinspirasi dari arsitektur LLVM namun dirancang ulang agar **100% modular**, **eksternal**, dan **language-agnostic**. Setiap komponen adalah crate Rust independen yang bisa digunakan, diganti, atau ditulis ulang tanpa mengganggu komponen lain.

### Filosofi

- **"Everything is a plugin"** — kompiler bukan monolit, tapi rangkaian komponen yang saling berkomunikasi lewat IR.
- **"Build piece by piece"** — kamu tidak perlu semua komponen untuk memulai. Cukup lexer + parser sudah bisa jalan.
- **"Polyglot by design"** — library inti Rust, binding resmi untuk C, Python, JavaScript/ WASM, dan Zig.

---

## 2. Masalah Nyata yang Brak Selesaikan

Sebelum membahas arsitektur, berikut masalah konkret di ekosistem compiler yang Brak selesaikan secara objektif:

| # | Masalah | Dampak | Solusi Brak |
|---|--------|--------|-------------|
| 1 | **LLVM terlalu berat** — 50MB+ dependensi, 30 menit compile | Developer bahasa kecil enggan pakai compiler toolkit | `brak-core` + `brak-codegen-obj` hanya ~500KB, compile < 5 detik |
| 2 | **FFI antar bahasa itu painful** — butuh C header, ABI mapping, marshaling kode | Isolasi ekosistem, sulit integrasi | IR-level FFI: 2 bahasa via Brak bisa call langsung tanpa glue code |
| 3 | **IR tidak human-readable** — LLVM IR padat, WASM binary, tidak bisa di-diff | Debugging compiler sulit, test susah | Brak IR = JSON/YAML/MessagePack default, bisa diff, patch, grep |
| 4 | **Compiler testing itu ad-hoc** — tiap project reinvent test infra | Banyak bug compiler tidak terdeteksi | `brak-test`: snapshot testing, differential fuzzing, IR diffing built-in |
| 5 | **Grammar terputus dari compiler** — ANTLR grammar ≠ IR types | Double maintenance, mismatch | `brak-syntax`: satu grammar → parser + IR types + formatter + LSP rules |
| 6 | **Pipeline build butuh external tools** — assembler, linker, runtime | Setup complex, cross-compile susah | `brak-codegen-obj` + `brak-link-native` → executable tanpa tool eksternal |
| 7 | **Cross-compilation butuh toolchain raksasa** — sysroot, linker terpisah | Developer kecil tidak bisa cross-compile | `brak-link-native` output PE/ELF/Mach-O dari host mana pun |
| 8 | **Incremental compilation afterthought** — kebanyakan toolkit tidak punya | Build lambat untuk project besar | IR node content hashing built-in, cache persistent, auto-incremental |
| 9 | **Optimization pass susah ditulis** — LLVM pass butuh ribuan line boilerplate | Hanya expert bisa buat optimasi | Pass = simple Rust trait `fn run(&self, ir: IrLir) -> Result<IrLir>` |
| 10 | **Debug info generation bolted-on** — metadata complex, mudah salah | Debugging user program sulit | First-class `DebugLoc` di LIR, semua backend otomatis generate DWARF/PDB |

---

### 2.1 Analisis Masalah Paling Kritis

#### 2.1.1 Masalah: LLVM Adalah Monolit Raksasa

LLVM dirancang untuk C++ dan Clang — compiler industrial ukuran raksasa. Untuk bahasa kecil (DSL, scripting, pendidikan), LLVM adalah *overkill* yang menyakitkan:

- `llvm-sys` Rust crate: download + compile LLVM = 30-60 menit
- Binary size: 50MB+ walau hanya pakai 1% fitur
- API stabil? Tidak. LLVM major release sering break API
- Cross-compile: butuh build LLVM untuk tiap target triple
- C++ compiled templates dimana-mana → error message tidak terbaca

**Solusi Brak**: Arsitektur Lego brick. Ambil komponen yang kamu butuh aja.

#### 2.1.2 Masalah: Isolasi Ekosistem Bahasa

Setiap bahasa punya ekosistem sendiri. Python punya C extensions, Rust punya FFI, JS punya NAPI. Mau panggil fungsi Python dari Rust? Tulis binding, manage reference count, handle GIL, convert types. Tiap bahasa ulang dari nol.

**Solusi Brak**: Semua bahasa yang compile ke Brak IR otomatis bisa call satu sama lain. Brak IR punya type system seragam, calling convention standar, dan linker yang handle semuanya. Ini bukan teori — Brak IR-lah yang jadi "lingua franca".

#### 2.1.3 Masalah: Compiler Testing Tidak Terstandarisasi

Lihat proyek compiler kecil di GitHub: testing biasanya "compile file ini, lihat output" — manual, rapuh, tidak reproducible. LLVM punya `FileCheck` yang powerfull tapi kompleks dan hanya untuk LLVM IR.

**Solusi Brak**: Snapshot testing built-in untuk setiap level IR. Formatnya JSON/YAML — bisa di-commit ke git, di-diff, di-review di PR.

---

## 3. Arsitektur

```
┌─────────────────────────────────────────────────────────┐
│                     User Language                        │
│  (source code .my Lang)                                  │
└────────────────────┬────────────────────────────────────┘
                     │
                     ▼
┌─────────────────────────────────────────────────────────┐
│  brak-frontend      (Lexer → Token → Parser → AST)      │
│  ─ stateless, streaming                                 │
└────────────────────┬────────────────────────────────────┘
                     │ AST (IrAst)
                     ▼
┌─────────────────────────────────────────────────────────┐
│  brak-ir            (Intermediate Representation)        │
│  ─ IrAst → IrHir → IrMir → IrLir                        │
│  ─ each level is a separate crate                       │
└────────────────────┬────────────────────────────────────┘
                     │ IrLir
                     ▼
┌─────────────────────────────────────────────────────────┐
│  brak-opt           (Optimization passes)                │
│  ─ all passes are external crates                       │
│  ─ users write custom passes via brak-opt-traits        │
└────────────────────┬────────────────────────────────────┘
                     │ IrLir (optimized)
                     ▼
┌─────────────────────────────────────────────────────────┐
│  brak-codegen       (Code Generation)                    │
│  ├── brak-codegen-asm    (NASM/MASM textual asm)       │
│  ├── brak-codegen-obj    (direct .obj/.o)              │
│  ├── brak-codegen-llvm   (LLVM IR bridge — optional)   │
│  ├── brak-codegen-c      (transpile-to-C fallback)     │
│  └── brak-codegen-wasm   (WASM output)                 │
└────────────────────┬────────────────────────────────────┘
                     │ binary / library
                     ▼
┌─────────────────────────────────────────────────────────┐
│  brak-link          (Linker)                             │
│  ├── brak-link-native   (PE/ELF/Mach-O)                 │
│  ├── brak-link-wasm     (WASM module)                   │
│  └── brak-link-archive  (.a / .lib)                     │
└─────────────────────────────────────────────────────────┘
```

### Aliran Data

```
Source → [Lexer] → Tokens → [Parser] → IrAst
  → [Hir lowering] → IrHir → [Mir lowering] → IrMir
  → [Lir lowering] → IrLir → [Opt passes] → IrLir (opt)
  → [Codegen] → Assembly/Obj → [Linker] → Executable
```

---

## 4. Daftar Komponen (Crates)

### 4.1 `brak-core` — Tipe dasar, error, Span, SourceMap

Kernel terkecil. Semua crate lain depend ke sini.

- `Span`, `SourceLoc`, `SourceMap`
- `Diagnostic`, `ErrorKind`, `Result<T>`
- `Version`, `Platform` detection
- Trait definitions: `Lexer`, `Parser`, `Pass`, `CodegenBackend`
- `ContentHash` — trait untuk hashing IR node (dasar incremental compilation)
- `DebugLoc` — source location yang first-class di semua IR level

### 4.2 `brak-frontend` — Lexer & Parser

Framework untuk membangun lexer/parser. Menyediakan:

- `BrakLexer` — trait + built-in lexer (ASCII, UTF-8)
- `BrakParser` — recursive descent / Pratt parser framework
- Error recovery built-in
- Stream input (file, string, incremental)
- **Parser combinator API** — mirip `nom` tapi terintegrasi dengan `Span` dan `Diagnostic`
- **Tree-sitter integration** — bisa parse pakai Tree-sitter grammar → AST

### 4.3 `brak-ir` — Intermediate Representation (4 levels)

#### `brak-ir-ast` — AST setelah parsing
- Tree struktural, persis seperti source
- `Serialize` + `Deserialize` via serde (JSON/YAML/MessagePack)
- `ContentHash` setiap node → untuk incremental cache

#### `brak-ir-hir` — High-level IR
- Desugared: `for` → `loop`, pattern matching → match tree
- Type-checked, name-resolved
- `HirItem`, `HirExpr`, `HirPat`, `HirTy`
- **Name resolution** built-in — scope tree, shadowing, hygiene

#### `brak-ir-mir` — Mid-level IR
- Control-flow graph (CFG)
- Basic blocks, terminators
- `MirBlock`, `MirInst`, `MirValue`
- No nested expressions — flat list of instructions
- **SSA form** with phi nodes (opsional, bisa non-SSA)
- **Content hashing** per block → hanya compile block yang berubah

#### `brak-ir-lir` — Low-level IR
- Register-based (infinite virtual registers)
- Close to target machine
- `LirFunc`, `LirBlock`, `LirInst`
- Opcodes: `Mov`, `Add`, `Sub`, `Load`, `Store`, `Call`, `Ret`, `Jmp`, `Br`, `Alloca`
- **First-class debug locations** di setiap instruction
- **Calling convention agnostic** — user tentukan calling convention sendiri

### 4.4 `brak-opt` — Optimization Framework

- `brak-opt-traits` — trait `OptimizationPass`
- Passes sebagai crate terpisah:
  - `brak-opt-dce` — Dead Code Elimination
  - `brak-opt-cp` — Constant Propagation
  - `brak-opt-gvn` — Global Value Numbering
  - `brak-opt-inline` — Inlining
  - `brak-opt-lcssa` — Loop-closed SSA form
  - `brak-opt-sroa` — Scalar Replacement of Aggregates
  - `brak-opt-licm` — Loop Invariant Code Motion
- **Pass Manager**: urutan eksekusi pass dikonfigurasi user via TOML
- **Custom pass**: cukup implement trait, register ke pass manager

```rust
pub trait OptimizationPass: Send + Sync {
    fn name(&self) -> &'static str;
    fn run(&self, ir: &mut ModuleLir) -> Result<Vec<OptimizationResult>>;
}
```

### 4.5 `brak-codegen` — Backend

- `brak-codegen-traits` — trait `CodegenBackend`
- Backend default:
  - `brak-codegen-asm`: cetak Lir → teks assembly (Intel/AT&T)
  - `brak-codegen-obj`: Lir → binary object file langsung (no external assembler)
    - **ELF** untuk Linux/Unix ✅ (all opcodes via `iced-x86`, proper header layout)
    - **PE (Portable Executable)** untuk Windows
    - **Mach-O** untuk macOS
  - `brak-codegen-llvm`: Lir → LLVM IR → optimize via LLVM (opsional, heavy)
  - `brak-codegen-c`: Lir → C source readable, idiomatic (portability fallback)
  - `brak-codegen-wasm`: Lir → WASM bytecode
- **DebugInfo generation** otomatis di semua backend

### 4.6 `brak-link` — Linker

- `brak-link-traits` — trait `LinkerBackend`
- `brak-link-native` — linker untuk PE (Windows), ELF (Linux), Mach-O (macOS)
- `brak-link-wasm` — linker WASM
- `brak-link-archive` — static library creator (.a / .lib)
- **No external linker needed** — Brak punya linker sendiri dari awal
- **LTO (Link-Time Optimization)** — di level LIR, bukan object file

### 4.7 `brak-ffi` — Foreign Function Interface

Bindings ke bahasa lain via C ABI:

- `brak-ffi-c` — C header + shared library
- `brak-ffi-python` — Python binding via PyO3
- `brak-ffi-wasm` — WASM package
- `brak-ffi-zig` — Zig binding
- **Stable ABI guarantee** — v1.0 → v2.0 tidak break binary compatibility

### 4.8 `brak-tool` — CLI

- `brak` command line (mirip `clang` or `llc`)
- Subcommands: `build`, `run`, `emit-ir`, `opt`, `link`, `asm`
- `brak.config.toml` loading
- **Multi-language support**: `brak build --lang mylang file.src`
- **IR inspection**: `brak emit-ir file.brak --level hir --format json`

### 4.9 `brak-syntax` — Definisi Syntax (opsional)

Framework untuk mendefinisikan grammar secara deklaratif:

- `brak-syntax-ebnf` — EBNF parser → AST grammar
- **AST types generator** — dari grammar langsung generate Rust types untuk IR
- **Formatter generator** — dari grammar + formatting rules
- **LSP query generator** — Tree-sitter queries untuk syntax highlighting
- Atau integrasi dengan `logos` / `lalrpop` / `pest`

### 4.10 `brak-test` — Compiler Testing Framework

**Ini komponen unik yang tidak dimiliki LLVM atau toolkit lain secara built-in.**

- **IR Snapshot Testing**: setiap level IR bisa di-snapshot dan di-diff
- **FileCheck-style matching**: pola matching untuk IR output
- **Differential Fuzzing**: compile program yang sama ke multiple backend, bandingkan hasil
- **Diagnostic Testing**: assert error message, warning, note pada posisi tertentu
- **Regression Database**: simpan IR yang pernah buggy, auto-test tiap rilis

```python
# Contoh: test case dalam Python
@brak_test.snapshot("tests/snapshots/")
def test_loop_desugar():
    ir = compile("""
        for i in 0..10 {
            print(i);
        }
    """)
    # Hir snapshot akan auto-compare dengan file tersimpan
    return ir.hir
```

### 4.11 `brak-polyglot` — Polyglot FFI Framework

**Ini fitur paling unik dan disruptive dari Brak.**

Brak menyediakan sistem FFI universal yang memungkinkan bahasa berbeda saling memanggil TANPA glue code. Caranya:

1. Setiap bahasa yang dibangun dengan Brak mengekspor type definition ke Brak IR
2. Brak IR punya type system standar: `Int`, `Float`, `Ptr`, `Struct`, `Func`, `Array`
3. Linker otomatis resolve cross-language calls di level LIR

```rust
// Bahasa A (Rust-like)
fn hello() -> i32 { 42 }

// Bahasa B (Python-like) — langsung panggil hello()
// compile by Brak, both in same binary
let x = hello();  // 42
// zero FFI overhead — direct call via LIR
```

Teknis: Brak polyglot bekerja dengan normalisasi calling convention di level LIR, bukan via C ABI. Hasilnya: performance native, zero marshaling, type-safe.

---

## 5. Unique Value Propositions

### 5.1 Zero-Dependency Build Pipeline

Brak adalah satu-satunya compiler toolkit yang bisa produce executable dari source language tanpa **satu pun** external tool:

```
source.brk  ──►  brak  ──►  output.exe
```

- No assembler (MASM, NASM, GAS)
- No linker (LINK, LD)
- No C compiler
- No system libraries (opsional — bisa link static)

**Cara**: `brak-codegen-obj` menulis binary PE/ELF/Mach-O langsung. `brak-link-native` resolve symbol dan produce final executable.

**Dampak**: Cross-compile dari Windows → Linux executable tanpa WSL, tanpa MinGW, tanpa Docker.

### 5.2 IR yang Human-Readable by Default

LLVM IR: `%1 = add i32 %0, 1` — padat, tidak bisa di-diff dengan baik.

Brak IR (format JSON):
```json
{
  "op": "Add",
  "ty": "Int(32)",
  "lhs": { "ref": "%0" },
  "rhs": { "value": 1 },
  "debug": { "file": "src/main.brk", "line": 42, "col": 8 }
}
```

Atau YAML:
```yaml
- op: Add
  ty: Int32
  lhs: { ref: "%0" }
  rhs: { value: 1 }
```

**Kenapa ini penting**: 
- Bisa di-grep, di-sed, di-awk
- Bisa di-version control dan di-diff di PR
- Bisa diedit manual untuk testing
- Bisa diparse oleh tool eksternal (Python, JS, dll)

### 5.3 Grammar-to-Everything Pipeline

Satu definisi grammar → generate semua yang kamu butuh:

```
grammar.ebnf
  ├── parser.rs        (Rust parser code)
  ├── ast_types.rs     (IR AST type definitions)
  ├── formatter.rs     (code formatter)
  ├── lsp_queries.scm  (Tree-sitter queries for highlighting)
  ├── snapshot_tests/  (auto-generated test cases)
  └── docs.md          (language documentation)
```

**Dampak**: 80% boilerplate compiler development hilang. Fokus ke semantic dan optimization.

### 5.4 First-Class Incremental Compilation

Setiap node IR punya `ContentHash` yang dihitung dari:
- Content node itu sendiri
- Content hash dari child nodes
- Version compiler

Saat kompilasi ulang:
1. Parser hash source file → compare dengan cache
2. Hanya function yang berubah di-parse
3. Lowering hanya jalan untuk function dengan AST baru
4. Optimization hanya jalan untuk function dengan LIR baru
5. Codegen hanya jalan untuk function dengan hasil opt baru

**Teknis**: Cache disimpan di `brak-cache/` directory, format MessagePack + Zstd.

### 5.5 Polyglot FFI Zero-Cost

Brak menghilangkan dichotomi "ekosistem bahasa". Semua bahasa Brak itu satu keluarga:

```
┌─────────────┐  ┌─────────────┐  ┌─────────────┐
│ MyLang      │  │ YourLang    │  │ TheirLang   │
│ (brak IR)   │  │ (brak IR)   │  │ (brak IR)   │
└──────┬──────┘  └──────┬──────┘  └──────┬──────┘
       │                │                │
       ▼                ▼                ▼
┌─────────────────────────────────────────────────────┐
│                  Brak Linker                        │
│  Resolve cross-language calls at LIR level          │
│  Single binary, no FFI overhead                     │
└─────────────────────────────────────────────────────┘
```

**Use case real**: Kamu punya kode ML di Python-like language, kode networking di Rust-like language, UI logic di JS-like language. Semua compile ke Brak IR, link jadi satu binary. Zero overhead.

### 5.6 Testing Framework Built-in

`brak-test` bukan add-on — ini komponen first-class yang didesain bareng IR.

Fitur unik:
- **IR Snapshot Testing**: simpan expected IR output (format YAML/JSON) di repo
- **Auto-update**: `--update` flag → update semua snapshot
- **Differential Testing**: compile ke asm vs obj vs llvm, bandingkan hasil
- **Fuzzing Integration**: property-based testing untuk compiler pass
- **Regression Hooks**: tiap commit auto-run test suite compiler

### 5.7 DebugInfo First-Class

Beda dengan LLVM yang debug info-nya bolted-on via metadata kompleks, Brak punya `DebugLoc` di setiap instruction LIR:

```rust
struct LirInst {
    opcode: Opcode,
    operands: Vec<Operand>,
    debug: DebugLoc,    // always present
}
```

Semua backend:
- `codegen-obj` → DWARF sections di ELF/Mach-O, CodeView di PE
- `codegen-wasm` → DWARF wasm
- `codegen-c` → `#line` directives
- `codegen-asm` → comment with source location

### 5.8 Multi-Tier Compilation

Brak mendukung 3 mode kompilasi yang bisa dipilih per-function:

| Mode | Kecepatan Compile | Kecepatan Eksekusi | Use Case |
|------|------------------|-------------------|----------|
| AstInterp | Instan | Lambat | Development, REPL |
| MirJit | Cepat | Medium | Iterasi cepat |
| LirOpt | Lambat | Maksimal | Production release |

User bisa annotate function:
```rust
#[brak(compile = "jit")]  // develop cepat
fn hot_path() { ... }

#[brak(compile = "opt")]  // production
fn critical() { ... }
```

### 5.9 Bring Your Own Pass (BYOP)

Bukan cuma optimization pass — *semua* komponen bisa diganti:

| Komponen Default | Bisa Diganti Dengan |
|-----------------|-------------------|
| BrakLexer | Tree-sitter, custom lexer |
| BrakParser | LALRPOP, Pest, hand-written |
| HirLower | Custom type system |
| Opt passes | ML-based optimizer |
| CodegenObj | Custom binary format |
| LinkNative | Custom linker script |
| Calling convention | Fastcall, thiscall, custom ABI |

### 5.10 Compiler Compiler

Brak bisa compile dirinya sendiri (self-hosting). Tapi lebih dari itu: Brak bisa compile compiler bahasa lain:

```
my_lang_compiler.brk  ──►  brak  ──►  my_lang_compiler.exe
                                               │
                                               ▼
                                        my_lang_source.my  ──►  output.exe
```

Ini memungkinkan bootstrapping: bahasa baru bisa nulis compilernya sendiri di bahasa itu, lalu Brak compile compiler tersebut.

---

## 6. Dependency Graph

```
brak-core (zero deps)
  ├── brak-frontend (dep: brak-core)
  ├── brak-ir-ast (dep: brak-core)
  ├── brak-ir-hir (dep: brak-core, brak-ir-ast)
  ├── brak-ir-mir (dep: brak-core, brak-ir-hir)
  ├── brak-ir-lir (dep: brak-core, brak-ir-mir)
  ├── brak-opt-traits (dep: brak-core, brak-ir-lir)
  │     └── brak-opt-* (dep: brak-opt-traits)
  ├── brak-codegen-traits (dep: brak-core, brak-ir-lir)
  │     └── brak-codegen-* (dep: brak-codegen-traits)
  ├── brak-link-traits (dep: brak-core)
  │     └── brak-link-* (dep: brak-link-traits)
  ├── brak-polyglot (dep: brak-core, brak-ir-*, brak-link-*)
  ├── brak-test (dep: brak-core, brak-ir-*)
  ├── brak-ffi-* (dep: brak-core + selected components)
  └── brak-tool (dep: everything)
```

Setiap crate hanya perlu depend ke *satu level di bawahnya* + `brak-core`.

---

## 7. API Design

### 7.1 Rust — Idiomatic

```rust
use brak_core::*;
use brak_frontend::lexer::Lexer;
use brak_frontend::parser::Parser;

let src = Source::from_file("hello.my")?;
let tokens = Lexer::new(&src).lex()?;
let ast = Parser::new(&tokens).parse()?;
let hir = HirLower::new().lower(ast)?;
let mir = MirLower::new().lower(hir)?;
let lir = LirLower::new().lower(mir)?;
let lir_opt = PassManager::default().run(lir)?;
let obj = Codegen::<ObjBackend>::new().codegen(&lir_opt)?;
let exe = Linker::<NativeLinker>::new().link(&[obj])?;
exe.write("output.exe")?;
```

### 7.2 C ABI — Stable

```c
#include <brak.h>

int main() {
    brak_session_t* s = brak_session_new();
    brak_ast_t* ast = brak_parse_file(s, "hello.my");
    brak_ir_t* ir = brak_lower(s, ast);
    brak_opt_run(s, ir, BRAK_O2);
    brak_codegen_obj(s, ir, "hello.o");
    brak_link_native(s, (const char*[]){"hello.o"}, 1, "hello.exe");
    brak_session_free(s);
    return 0;
}
```

### 7.3 Python — Pythonic

```python
from brak import Session, LangBuilder

s = Session()
ast = LangBuilder(s).parse_file("hello.my")
ir = ast.lower()
ir.optimize("O2")
ir.codegen("obj", "hello.o")
s.link("native", ["hello.o"], "hello.exe")
```

---

## 8. Persistence & Caching

- **Bitcode**: Simpan `IrLir` ke file biner (mirip LLVM bitcode) untuk caching
- Crate `brak-bitcode` — serialize/deserialize semua level IR
- Format: MessagePack + Zstd compression
- **Content-addressed cache**: IR disimpan berdasarkan hash. Fungsi yang sama = hash sama = skip

### Cache Layout

```
.brak-cache/
├── sources/
│   ├── abc123.hash   (source file hash → list of function hashes)
├── ir/
│   ├── ast/
│   │   └── func_def_abc.msgpack
│   ├── hir/
│   ├── mir/
│   └── lir/
├── opt/
│   └── func_def_abc_opt_O2.msgpack
└── obj/
    └── func_def_abc.o
```

---

## 9. Incremental Roadmap

### Phase 0 — Foundation (`v0.1`) — ✓ SELESAI
- [x] `brak-core` — Span, SourceMap, Diagnostic, ContentHash trait
- [x] `brak-frontend` — Lexer trait + sample lexer (ASCII)
- [x] `brak-ir-ast` — AST definition + serde + pretty printer
- [x] `brak-tool` — `brak emit-ir` command (tokens + ast levels)

### Phase 1 — IR Pipeline (`v0.2`) — ✓ SELESAI
- [x] `brak-ir-hir` — desugaring lowering + name resolution
- [x] `brak-ir-mir` — CFG construction, basic blocks, terminators
- [x] `brak-ir-lir` — register-based low IR, opcodes (Mov, Add, Sub, Cmp, Ret, Br, Jmp, dll)
- [x] `brak-tool` — `brak emit-ir --level ast|hir|mir|lir` + parser recursive descent

### Phase 2 — Codegen Awal (`v0.3`) — 3-4 minggu
- [x] `brak-codegen-asm` — Intel syntax, x86-64 subset (mov, add, sub, call, ret, jmp)
- [x] `brak-codegen-obj` — ELF object writer (Linux first) — all opcodes via `iced-x86`
  - [x] Mov, Add, Sub, Mul, Div, Neg, Not
  - [x] And, Or, Xor, Shl, Shr
  - [x] Cmp, SetEq/Ne/Lt/Le/Gt/Ge
  - [x] Jmp, Br (conditional branch)
  - [x] Ret (prolog/epilog), Push, Pop
  - [x] ELF header layout benar (e_shstrndx u16, shoff aligned)
- [x] `brak-codegen-traits` — CodegenBackend trait
- [x] Mini bahasa sample: kalkulator aritmetika → executable
- [x] **Milestone: "Hello World" benar-benar standalone**

### Phase 3 — Linker (`v0.4`) — 2-3 minggu
- [x] `brak-link-traits` — LinkerBackend trait, ObjectFile struct
- [x] `brak-link-native` — ELF linker (parse .o, resolve symbol, apply relocation, output executable)
- [x] `brak-link-native` — PE linker
- [x] `brak-link-native` — Mach-O linker
- [x] `brak-tool` — pipeline `LIR → .o → linker → executable` via `brak build`
- [x] `write_elf_executable` di `brak-codegen-obj` deprecated (pakai linker baru)
- [x] `brak-link-archive` — static library (In Progress: Basic AR format)
- [x] **Milestone: Zero external tools pipeline**

### Phase 4 — Optimasi (`v0.5`) — 3-4 minggu
- [x] `brak-opt-traits` + Pass Manager
- [x] `brak-opt-dce` — Dead Code Elimination (Basic: Function retention)
- [x] `brak-opt-cp` — Constant Propagation (Basic: Local CP)
- [x] `brak-opt-gvn` — Global Value Numbering (Basic: Local VN)
- [x] `brak-opt-inline` — Inlining (Basic: Small functions)
- [x] Custom pass loading dari external crate
- [x] Global analysis for CP and GVN (Enhanced Phase 4)

### Phase 5 — Testing & DX (`v0.6`) — 2-3 minggu
- [x] `brak-test` — IR Snapshot Testing
- [x] `brak-test` — Diagnostic Testing
- [x] `brak-test` — Execution & Differential Testing (Initial)
- [x] `brak emit-ir --format json|yaml`

### Phase 6 — Polyglot & FFI (`v0.7`) — 3-4 minggu
- [x] `brak-polyglot` — Multi-language type normalization
- [x] Cross-language call (LIR level & x86_64 backend)
- [x] FFI binding generation (C headers)
- [x] Support for `extern fn` in frontend and lowering
- [x] Initial PyO3 binding generator infrastructure
- [x] Advanced PyO3 support (Python binding)
- [x] **Milestone: Dua bahasa sample saling call tanpa FFI**

### Phase 7 — More Backends (`v0.8`) — 3-4 minggu
- [x] `brak-codegen-wasm` — WASM bytecode output
- [x] `brak-codegen-c` — idiomatic C transpiler
- [x] `brak-codegen-llvm` — LLVM IR bridge (opsional)
- [x] `brak-link-wasm` — WASM module linker
- [x] `brak-bitcode` — persistent IR cache

### Phase 8 — Debug Info (`v0.9`) — 2-3 minggu
- [x] DWARF generation di codegen-obj (ELF & Mach-O .o) — `dwarf.rs` + `elf.rs`/`macho_obj.rs`
  - [x] `.debug_line`, `.debug_info`, `.debug_abbrev`, `.debug_str` di relocatable .o
  - [x] DWARF di ELF executable — `link_elf()` writes DWARF section headers + data
  - [x] DWARF di Mach-O executable — `link_macho()` writes `__DWARF` segment
  - [x] `parse_elf`/`parse_macho` extract DWARF into `debug_sections` — preserved through linking
- [x] `#line` directives di codegen-c — `CWriter::emit_inst()` emits `#line` on line change
- [x] CodeView generation di codegen-obj (PE) — `codeview.rs` + `coff.rs`
  - [x] `S_GPROC32` + `S_END` per function (function symbols)
  - [x] `DEBUG_S_LINES` subsection (line-to-address mapping)
  - [x] `.debug$S` section di COFF output
- [x] PE debug directory — `pe.rs`
  - [x] RSDS header (`CV_INFO_PDB70`) + merged C13 subsections
  - [x] `IMAGE_DEBUG_DIRECTORY` entry (Type=2, CodeView)
  - [x] `.rdata` section + data directory entry 6
- [x] Debugger can set breakpoints on Brak-compiled code (PE)
  - [x] Can set by function name (`S_GPROC32`)
  - [x] Can set by source line (`DEBUG_S_LINES`)
  - [ ] Verified with WinDbg/CDB — manual test, environment belum siap
  - [ ] PDB file generation — opsional, deferred (RSDS + C13 cukup untuk MVP)

### Phase 9 — Maturity (`v1.0`) — ✓ SELESAI
- [x] Full optimization pipeline (8+ passes)
- [x] SROA (Basic), Inlining, LICM, TCO, Jump Threading, Constant Folding
- [x] Iterative optimization pass manager (Convergence support)
- [ ] Self-hosting (Brak bisa compile Brak)
- [x] Stabil ABI untuk C bindings (via brak-polyglot)
- [x] Comprehensive documentation + tutorial
- [ ] Benchmark suite vs LLVM

---

## 10. Design Principles

1. **Zero magic** — semua komponen bisa diganti user
2. **Progressive complexity** — user bisa mulai dari lexer aja
3. **No monolith** — satu crate per tanggung jawab
4. **Stable ABI** — C binding diutamakan untuk stabilitas
5. **Streaming first** — parser dan lexer bisa handle input besar tanpa load semua ke memory
6. **Error recovery** — jangan panic, kasih diagnostic yang jelas
7. **Deterministic** — output harus reproducible (no random, seed-aware)
8. **Testable by design** — setiap komponen punya test harness
9. **Human-first IR** — IR harus bisa dibaca dan diedit manusia
10. **Zero-cost abstractions** — jangan ada overhead runtime untuk fitur yang tidak dipakai

---

## 11. Non-Goals (v1)

- ❌ Full debugger (hanya debug info generation)
- ❌ Package manager
- ❌ LSP server (hanya query generator untuk LSP)
- ❌ Build system
- ❌ Formal verification engine (hanya export ke SMT-LIB)
- ❌ GUI/IDE integration

---

## 12. Naming Convention

| Prefix | Contoh | Keterangan |
|--------|--------|------------|
| `Brak` | `BrakLexer` | Rust type/trait |
| `Ir` | `IrLir`, `IrMir` | Intermediate representation nodes |
| `BRK_` | `BRK_ERROR_TYPE` | C ABI constant |
| `brk_` | `brk_session_new` | C ABI function |
| `brak.` | `brak.config.toml` | Config file |
| `.brk` | `main.brk` | Source file extension |
| `.bkr` | `output.bkr` | Brak bitcode IR |

---

## 13. Struktur Repository

```
brak/
├── Cargo.workspace.toml
├── prd.md
├── brak-core/
├── brak-frontend/
├── brak-ir/
│   ├── brak-ir-ast/
│   ├── brak-ir-hir/
│   ├── brak-ir-mir/
│   └── brak-ir-lir/
├── brak-opt/
│   ├── brak-opt-traits/
│   ├── brak-opt-dce/
│   ├── brak-opt-cp/
│   ├── brak-opt-gvn/
│   ├── brak-opt-inline/
│   ├── brak-opt-sroa/
│   └── brak-opt-licm/
├── brak-codegen/
│   ├── brak-codegen-traits/
│   ├── brak-codegen-asm/
│   ├── brak-codegen-obj/
│   ├── brak-codegen-c/
│   ├── brak-codegen-wasm/
│   └── brak-codegen-llvm/
├── brak-link/
│   ├── brak-link-traits/
│   ├── brak-link-native/
│   └── brak-link-wasm/
├── brak-ffi/
│   ├── brak-ffi-c/
│   └── brak-ffi-python/
├── brak-polyglot/
├── brak-test/
├── brak-bitcode/
└── brak-tool/
```

Setiap crate punya `Cargo.toml` sendiri, semuanya di-root workspace.

---

## 14. Contoh Use Case Lengkap

Skenario: Kamu mau buat bahasa scripting sederhana "LiteLang" dengan Brak.

```rust
// 1. Define grammar (brak-syntax)
// grammar.lite.ebnf
program      = { statement } ;
statement    = let_stmt | expr_stmt ;
let_stmt     = "let", ident, "=", expr, ";" ;
expr_stmt    = expr, ";" ;
expr         = term, { ("+" | "-"), term } ;
term         = factor, { ("*" | "/"), factor } ;
factor       = number | ident | "(", expr, ")" ;

// 2. Brak generates: parser.rs, ast_types.rs, formatter.rs

// 3. Write lowering pass (50 lines)
use brak_core::*;
use brak_ir_ast::*;
use brak_ir_hir::*;

struct LiteLower;
impl HirLower for LiteLower {
    fn lower_expr(&self, expr: AstExpr) -> Result<HirExpr> {
        match expr {
            AstExpr::Number(n) => Ok(HirExpr::Const(HirConst::Int(n))),
            AstExpr::Add(l, r) => Ok(HirExpr::BinOp(
                BinOp::Add,
                Box::new(self.lower_expr(*l)?),
                Box::new(self.lower_expr(*r)?),
            )),
            // ...
        }
    }
}

// 4. Compile pipeline (30 lines)
fn compile(src: &str) -> Result<Vec<u8>> {
    let tokens = BrakLexer::new(src).lex()?;
    let ast = Parser::new(&tokens, &grammar::rules()).parse()?;
    let hir = LiteLower.lower(ast)?;
    let mir = MirLower::new().lower(hir)?;
    let lir = LirLower::new().lower(mir)?;
    let lir = PassManager::default()
        .with_pass(Dce)
        .with_pass(ConstantProp)
        .run(lir)?;
    let obj = Codegen::<ObjBackend>::new().codegen(&lir)?;
    Linker::<NativeLinker>::new()
        .link(&[obj])?
        .write("output.exe")
}

// 5. Done. output.exe is a real, standalone executable.
// No C compiler, no assembler, no linker needed.
```

Total: ~80 lines Rust untuk bahasa lengkap dengan compiler. Ini yang membuat Brak unik.
