# PRD: Brak — Language Construction Toolkit

> **Status dokumen**: disinkronkan dengan `prioritas.md` (bukan hanya aspirasi).
> - `prioritas.md` = sumber kebenaran status bug/fitur & roadmap per fase.
> - `daftar_api.md` = daftar API nyata yang ada sekarang + cara pakai.
> - Bagian yang ditandai **📦 SUDAH ADA** benar-benar diimplementasi.
> - Bagian yang ditandai **🚧 BACKLOG/RENCANA** belum ada di kode — jangan dianggap nyata.

## 1. Vision

Brak adalah library toolkit untuk membangun bahasa pemrograman yang *fully standalone*. Terinspirasi dari arsitektur LLVM namun dirancang ulang agar **100% modular**, **eksternal**, dan **language-agnostic**. Setiap komponen adalah crate Rust independen yang bisa digunakan, diganti, atau ditulis ulang tanpa mengganggu komponen lain.

### Filosofi

- **"Everything is a plugin"** — kompiler bukan monolit, tapi rangkaian komponen yang saling berkomunikasi lewat IR.
- **"Build piece by piece"** — kamu tidak perlu semua komponen untuk memulai. Cukup lexer + parser sudah bisa jalan.
- **"Polyglot by design"** — library inti Rust. Binding nyata: C headers (`--gen-h`) & Python PyO3 (`--py-module`) via `brak-polyglot`. Binding JS/WASM & Zig 🚧 rencana (belum ada).

---

## 2. Masalah Nyata yang Brak Selesaikan

Sebelum membahas arsitektur, berikut masalah konkret di ekosistem compiler yang Brak selesaikan secara objektif:

| # | Masalah | Dampak | Solusi Brak |
|---|--------|--------|-------------|
| 1 | **LLVM terlalu berat** — 50MB+ dependensi, 30 menit compile | Developer bahasa kecil enggan pakai compiler toolkit | `brak-core` + `brak-codegen-obj` hanya ~500KB, compile < 5 detik |
| 2 | **FFI antar bahasa itu painful** — butuh C header, ABI mapping, marshaling kode | Isolasi ekosistem, sulit integrasi | IR-level FFI: 2 bahasa via Brak bisa call langsung tanpa glue code |
| 3 | **IR tidak human-readable** — LLVM IR padat, WASM binary, tidak bisa di-diff | Debugging compiler sulit, test susah | Brak IR = JSON/YAML (via `emit-ir --format`), bisa diff, patch, grep |
| 4 | **Compiler testing itu ad-hoc** — tiap project reinvent test infra | Banyak bug compiler tidak terdeteksi | `brak-test` 📦: snapshot testing, diagnostic testing, execution & differential testing (initial). Fuzzing = 🚧 |
| 5 | **Grammar terputus dari compiler** — ANTLR grammar ≠ IR types | Double maintenance, mismatch | `brak-syntax` 🚧 **rencana** — satu grammar → parser + IR types + formatter + LSP rules (belum ada di kode) |
| 6 | **Pipeline build butuh external tools** — assembler, linker, runtime | Setup complex, cross-compile susah | `brak-codegen-obj` + `brak-link-native` → executable tanpa tool eksternal |
| 7 | **Cross-compilation butuh toolchain raksasa** — sysroot, linker terpisah | Developer kecil tidak bisa cross-compile | `brak-link-native` output PE/ELF/Mach-O dari host mana pun |
| 8 | **Incremental compilation afterthought** — kebanyakan toolkit tidak punya | Build lambat untuk project besar | IR node content hashing built-in (trait `ContentHash`). Cache persisten = `brak-bitcode` 🚧 **eksperimental**, belum terintegrasi |
| 9 | **Optimization pass susah ditulis** — LLVM pass butuh ribuan line boilerplate | Hanya expert bisa buat optimasi | Pass = simple Rust trait `fn run(&self, ir: IrLir) -> Result<IrLir>` |
| 10 | **Debug info generation bolted-on** — metadata complex, mudah salah | Debugging user program sulit | `DebugLoc` first-class di LIR 📦. Generasi DWARF/CodeView di codegen-obj + linker 🚧 **sebagian & belum terverifikasi penuh** (lihat prioritas.md Fase 6 backlog) |

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

Kernel terkecil. Semua crate lain depend ke sini. 📦

- `Span`, `SourceLoc`, `SourceMap`, `DUMMY_SPAN`
- `Diagnostic`, `Diagnostics`, `Severity`, `Result<T>`
- `Version` (+ `BRAK_VERSION`)
- `ContentHash` — trait hashing IR node (dasar incremental compilation)
- Catatan: `Platform` detection & trait `Lexer/Parser/Pass/CodegenBackend` di core **tidak ada** — trait lexer ada di `brak-frontend`, codegen di `brak-codegen-traits`, opt di `brak-opt-traits`.

### 4.2 `brak-frontend` — Lexer & Parser

Framework untuk membangun lexer/parser. Menyediakan:

- `BrakLexer` — trait + built-in lexer (ASCII): `AsciiLexer`
- `BrakParser` — recursive descent: `Parser` (Pratt-style precedence)
- Error recovery built-in (parser collect ke `Diagnostics`)
- Stream input (file, string) via `SourceMap`
- 🚧 **Parser combinator API** (mirip `nom`), **tree-sitter integration**, dan **`DebugLoc` di Span** — belum ada di kode

### 4.3 `brak-ir` — Intermediate Representation (4 levels)

#### `brak-ir-ast` — AST setelah parsing
- Tree struktural, persis seperti source
- `Serialize` + `Deserialize` via serde (**`serde_json`**; YAML/MessagePack 🚧 belum ada)
- `ContentHash` setiap node → untuk incremental cache

#### `brak-ir-hir` — High-level IR
- Desugared: `for` → `loop`, pattern matching → match tree
- Type-checked, name-resolved
- `HirItem`, `HirExpr`, `HirPat`, `HirTy`
- **Name resolution** via `ScopeStack` (shadowing & out-of-scope terdeteksi). Hygiene 🚧

#### `brak-ir-mir` — Mid-level IR
- Control-flow graph (CFG)
- Basic blocks, terminators
- `MirBlock`, `MirInst`, `MirValue`
- No nested expressions — flat list of instructions
- 🚧 **SSA form with phi nodes** — belum ada
- **Content hashing** per block → hanya compile block yang berubah

#### `brak-ir-lir` — Low-level IR
- Register-based (infinite virtual registers)
- Close to target machine
- `LirFunc`, `LirBlock`, `LirInst`
- Opcodes: `Mov`, `Add`, `Sub`, `Load`, `Store`, `Call`, `Ret`, `Jmp`, `Br`, `Alloca`
- **First-class debug locations** di setiap instruction
- **Calling convention agnostic** — user tentukan calling convention sendiri

### 4.4 `brak-opt` — Optimization Framework

- `brak-opt-traits` — trait `LirOptimizationPass` + `PassManager`
- Passes sebagai crate terpisah (semuanya 📦):
  - `brak-opt-fold` — Constant Folding
  - `brak-opt-cp` — Constant Propagation (path-sensitive, lattice)
  - `brak-opt-gvn` — Global Value Numbering
  - `brak-opt-inline` — Inlining
  - `brak-opt-licm` — Loop Invariant Code Motion
  - `brak-opt-jt` — Jump Threading
  - `brak-opt-tco` — Tail Call Optimization (self tail-call → loop)
  - `brak-opt-dce` — Dead Code Elimination
  - `brak-opt-utils` — CFG builder, Dominance, Natural Loops (dipakai pass)
  - 🚧 `brak-opt-lcssa`, `brak-opt-sroa` — belum ada sebagai crate
- **Pass Manager** 🚧: urutan pass dikonfigurasi **di kode** (bukan TOML) via `add_pass`
- **Custom pass**: cukup implement trait, daftarkan lewat `add_pass` atau plugin dinamis `load_external_pass`

```rust
pub trait LirOptimizationPass: Send + Sync {
    fn name(&self) -> &'static str;
    fn run(&self, program: LirProgram) -> Result<LirProgram>;
}
```

### 4.5 `brak-codegen` — Backend

- `brak-codegen-traits` — trait `CodegenBackend` + `CodegenExecutable`
- Backend default (backend utama = `brak-codegen-obj`):
  - `brak-codegen-asm`: cetak Lir → teks assembly (Intel). ⚠️ **tidak dipakai pipeline** (non-consumer)
  - `brak-codegen-obj`: Lir → binary object file langsung (no external assembler)
    - **ELF** untuk Linux/Unix (via `iced-x86`)
    - **PE (Portable Executable)** untuk Windows (COFF + CodeView)
    - **Mach-O** untuk macOS
    - Opcode yang belum didukung → error eksplisit (bukan silent)
  - `brak-codegen-llvm`: Lir → LLVM IR (opsional, heavy)
  - `brak-codegen-c`: Lir → C source readable (portability fallback)
  - `brak-codegen-wasm`: Lir → **WAT text**, bukan binary `.wasm`
- **DebugInfo** 🚧 sebagian: DWARF/CodeView ada di codegen-obj & linker tapi belum matang/terverifikasi penuh

### 4.6 `brak-link` — Linker

- `brak-link-traits` — trait `LinkerBackend`, struct `ObjectFile` & `LinkerOutput`
- `brak-link-native` — linker untuk PE (Windows, exe **dan** DLL), ELF (Linux exe; ELF DYN 🚧), Mach-O (macOS)
- `brak-link-wasm` — linker WASM
- `brak-link-archive` — static library creator + parser (.a / .lib; input archive dipakai CLI)
- **No external linker needed** — Brak punya linker sendiri dari awal
- **LTO (Link-Time Optimization)** 🚧 — belum ada; optimasi berjalan di level LIR sebelum codegen

### 4.7 `brak-polyglot` — FFI (bukan `brak-ffi-*`)

Realita: FFI diimplementasi lewat **`brak-polyglot`**, bukan crate `brak-ffi-*` terpisah.
Crate `brak-ffi-c/python/wasm/zig` **belum ada**.

- `PolyglotBridge` — normalisasi tipe (Brak ↔ C) + `extract_bindings` dari HIR
- `CHeaderGenerator` — generate C header
- `PyO3Generator` — generate proyek Python extension (PyO3) yang siap `maturin develop`
- Binding WASM/Zig 🚧
- **Stable ABI guarantee** 🚧 — klaim belum bisa divalidasi (belum ada komitmen ABI lintas versi)

### 4.8 `brak-tool` — CLI

- `brak` command line
- Subcommands nyata: **`build`** dan **`emit-ir`**. `run`, `opt`, `link`, `asm` 🚧 belum ada
- `brak.config.toml` loading 🚧 belum ada
- **Multi-language support**: via ekstensi file — `.brk` (Brak) dan `.lit` (Lit). Flag `--lang` 🚧 belum ada
- **IR inspection**: `brak emit-ir file.brak --level hir --format json` 📦
- Build: `--entry`, `--output`, `--shared` (DLL), `--gen-h`, `--py-module`, `--opt-pass`, `--opt-iterations`, `--verbose-opt`

### 4.9 `brak-syntax` — Definisi Syntax (opsional) 🚧 BELUM ADA

Framework untuk mendefinisikan grammar secara deklaratif (rencana):

- `brak-syntax-ebnf` — EBNF parser → AST grammar
- **AST types generator** — dari grammar langsung generate Rust types untuk IR
- **Formatter generator** — dari grammar + formatting rules
- **LSP query generator** — Tree-sitter queries untuk syntax highlighting

### 4.10 `brak-test` — Compiler Testing Framework

Fitur yang sudah ada (semua Rust API, bukan Python):

- **IR Snapshot Testing**: `SnapshotTester::assert_snapshot(name, &ir)` — compare dengan file tersimpan, update via `update: true`
- **Diagnostic Testing**: `DiagnosticTester::assert_has_error` / `assert_has_warning`
- **Execution & Differential Testing (initial)**: `ExecutionTester::assert_output(exe, expected)`
- 🚧 **FileCheck-style matching**, **Differential Fuzzing**, **Regression Database** — belum ada

### 4.11 `brak-polyglot` — Polyglot FFI Framework

FFI nyata untuk memanggil fungsi Brak dari bahasa lain:

1. Fungsi publik diekstrak dari HIR (`PolyglotBridge::extract_bindings`)
2. Tipe dinormalisasi (`brak_to_c`, `hir_to_c`, `c_to_brak`)
3. Generator menghasilkan **C header** (`CHeaderGenerator`) atau **proyek PyO3** (`PyO3Generator`)

```rust
// lib.brk — fungsi Brak yang diekstrak menjadi binding
fn add(a: i32, b: i32) -> i32 { a + b }
```

Pemakaian via CLI:
```bash
brak build lib.brk --gen-h lib.h --shared   # header C + DLL
brak build lib.brk --py-module liblib -o py_liblib/   # proyek Python
# lalu: cd py_liblib && maturin develop → import liblib
```

Realita: cross-language call **di dalam satu binary** (`.brk` ↔ `.lit`) bekerja lewat IR
perantara yang sama (contoh `samples/cross_lit.brk`). Normalisasi calling convention
**di level LIR untuk pemanggilan antar bahasa brak**, sedangkan keluar ke C/Python
via C ABI (Win64/SystemV) + header.

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

Brak IR (format JSON, contoh disederhanakan — struktur nyata lihat `emit-ir --format json` di `daftar_api.md`):
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

### 5.3 Grammar-to-Everything Pipeline 🚧 BELUM ADA

Rencana: satu definisi grammar → generate semua yang kamu butuh:

```
grammar.ebnf
  ├── parser.rs        (Rust parser code)
  ├── ast_types.rs     (IR AST type definitions)
  ├── formatter.rs     (code formatter)
  ├── lsp_queries.scm  (Tree-sitter queries for highlighting)
  ├── snapshot_tests/  (auto-generated test cases)
  └── docs.md          (language documentation)
```

Butuh `brak-syntax` (lihat §4.9) yang belum ada. Saat ini grammar ditulis tangan:
lexer `AsciiLexer` + parser `Parser` di `brak-frontend`.

### 5.4 Incremental Compilation — Sebagian

- `ContentHash` 📦: trait hashing tersedia (`brak-core`), dipakai PassManager untuk
  deteksi perubahan.
- Cache persisten 🚧 **eksperimental**: `brak-bitcode` menyimpan AST/HIR/MIR/LIR
  (`get_or_compute_*`), format **serde_json** (bukan MessagePack), dan **belum
  terintegrasi** ke CLI maupun `brak-easy`.
- Parser/backed caching per-function ("hanya fungsi yang berubah") 🚧 belum ada.

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

**Use case real saat ini**: `.brk` dan `.lit` di-compile ke IR yang sama lalu
di-link jadi satu binary — fungsi Lit bisa dipanggil dari Brak tanpa glue code
(contoh `samples/cross_lit.brk`). Keluar ke bahasa lain (C/Python) via C ABI + header.

### 5.6 Testing Framework Built-in

`brak-test` bukan add-on — ini komponen first-class yang didesain bareng IR.

Fitur yang terimplementasi:
- **IR Snapshot Testing**: simpan expected IR output ke snapshot dir, `update: true` untuk auto-refresh
- **Diagnostic Testing**: `assert_has_error` / `assert_has_warning`
- **Execution & Differential Testing (initial)**: jalankan exe, bandingkan output/exit

🚧 **Fuzzing Integration** (property-based) dan **Regression Hooks** — belum ada.

### 5.7 DebugInfo First-Class

Beda dengan LLVM yang debug info-nya bolted-on via metadata kompleks, Brak punya `debug: Span` di setiap instruction LIR:

```rust
struct LirInst {
    opcode: Opcode,
    operands: Vec<LirOperand>,
    debug: Span,          // selalu ada (source location)
}
```

Status nyata backend:
- `codegen-obj` → DWARF sections di ELF/Mach-O & CodeView di PE 🚧 **sebagian, belum terverifikasi penuh** (prioritas.md Fase 6 backlog)
- `codegen-c` → `#line` directives 📦
- `codegen-wasm` → DWARF wasm 🚧 tidak ada
- `codegen-asm` → comment with source location 🚧 tidak ada

### 5.8 Multi-Tier Compilation 🚧 BELUM ADA

Rencana 3 mode kompilasi per-function (AstInterp / MirJit / LirOpt) dan anotasi
`#[brak(compile = "jit")]` — **belum diimplementasi**. Saat ini satu jalur penuh:
frontend → HIR → MIR → LIR → opt → codegen → link.

### 5.9 Bring Your Own Pass (BYOP)

Bukan cuma optimization pass — *beberapa* komponen bisa diganti lewat trait:

| Komponen | Mekanisme saat ini |
|----------|--------------------|
| Lexer | trait `BrakLexer` (sekarang `AsciiLexer`); buat struct sendiri lalu implement |
| Parser | `Parser` dari `brak-frontend` (struct konkret) — bisa ditulis tangan, input token bebas |
| Opt passes | trait `LirOptimizationPass` + `PassManager::add_pass` / `load_external_pass` 📦 |
| Codegen | trait `CodegenBackend` + `CodegenExecutable` 📦 |
| Linker | trait `LinkerBackend` 📦 |
| HirLower / MirLower / LirLower | struct konkret (tanpa trait) — bisa memanggil API langsung |

### 5.10 Compiler Compiler 🚧 RENCANA

Rencana: Brak bisa compile compiler bahasa lain (bootstrapping). Infrastruktur
dasar mendukung (run-time, struct/enum), tapi **self-hosting penuh belum
diverifikasi** — tidak ada bukti Brak berhasil meng-compile dirinya sendiri saat ini.

---

## 6. Dependency Graph

```
brak-core (zero deps — tipe dasar, Span, Diagnostic, ContentHash)
  ├── brak-frontend (dep: brak-core, brak-ir-ast)
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
  ├── brak-polyglot (dep: brak-core, brak-ir-hir)
  ├── brak-lang-lit (dep: brak-ir-hir)
  ├── brak-test (dep: brak-core, brak-ir-*)
  ├── brak-bitcode (dep: brak-ir-*)          — eksperimental
  ├── brak-easy (dep: frontend, semua IR, opt, codegen-obj, link-native)
  └── brak-tool (dep: everything)
```

Setiap crate hanya perlu depend ke *satu level di bawahnya* + `brak-core`.

---

## 7. API Design

> Referensi lengkap: `daftar_api.md`. Contoh di bawah adalah API nyata yang ada sekarang.

### 7.1 Rust — pipeline level tinggi (`brak-easy`) 📦

```rust
use brak_easy::{EasyPipeline, OptLevel};

let src = "fn main() -> i32 { 42 }";
EasyPipeline::new()
    .with_opt_level(OptLevel::Default)
    .build_executable("app", src, "app.exe")?;
```

Atau level demi level (frontend → HIR → MIR → LIR → opt → obj → link):
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
use brak_link_traits::{LinkerBackend, ObjectFile};
use brak_link_native::NativeLinker;

let sm = SourceMap::new("main.brk", src);
let tokens = AsciiLexer::new().lex(&sm);
let ast = Parser::new().parse(&tokens)?;
let hir = HirLower::new().lower(ast).map_err(|e| e.to_string())?;
TypeChecker::new().check(&hir)?;
let mir = MirLower::new().lower(hir)?;
let mut ll = LirLower::new();
ll.set_file_id(0);
let lir = ll.lower(mir);
let obj = ObjBackend::default().emit(&lir)?;
let exe = NativeLinker.link(
    &[ObjectFile { name: "main.o".into(), data: obj }],
    "main", 0x140000000,
)?;
std::fs::write("output.exe", exe.data)?;
```

### 7.2 C — lewat header yang di-generate 📦

Tidak ada C API *session* (pola `brak_session_*` belum ada). Jalur C = compile Brak
ke shared library + generate header, lalu pakai dari C:

```bash
brak build lib.brk --shared --gen-h lib.h -o lib.dll
```

```c
#include "lib.h"   /* hasil generate: deklarasi fungsi publik Brak */

int main(void) {
    return add(10, 20);
}
```

### 7.3 Python — via PyO3 yang di-generate 📦

Modul `brak` Python (pola `from brak import Session`) **belum ada**. Jalur nyata:

```bash
brak build lib.brk --py-module liblib -o py_liblib/
cd py_liblib && maturin develop
```

```python
import liblib
liblib.add(10, 20)   # memanggil fungsi Brak yang diekspor
```

---

## 8. Persistence & Caching

- **Bitcode**: Crate `brak-bitcode` — serialize/deserialize AST/HIR/MIR/LIR ke file JSON
- Format nyata: **serde_json** (bukan MessagePack + Zstd)
- **Content-addressed cache**: cache dikunci oleh hash yang anda berikan — lewat
  `get_or_compute_*(hash, |f| ...)`; bila hash ada, compute tidak dijalankan
- Status: 🚧 **eksperimental** — belum terintegrasi ke CLI maupun `brak-easy`

### API BitcodeCache

```rust
let cache = BitcodeCache::new(".brak_cache");
let ast = cache.get_or_compute_ast(hash_of_src, || {
    AstBuilder::default().build(src)   // hasil dipakai bila belum ter-cache
});
cache.invalidate("ast", hash);
cache.clear_all();
```

### Struktur penyimpanan (di dalam direktori cache)

```
<cache_dir>/
├── ast/<hash>.json
├── hir/<hash>.json
├── mir/<hash>.json
└── lir/<hash>.json
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
- [x] `brak-link-archive` — static library (AR format + symbol table + parser, input `.a`/`.lib` didukung CLI)
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
- [x] `brak-codegen-wasm` — output **WAT text** (bukan binary `.wasm`; butuh wat2wasm)
- [x] `brak-codegen-c` — idiomatic C transpiler
- [x] `brak-codegen-llvm` — LLVM IR bridge (opsional, `.ll` struktural valid)
- [x] `brak-link-wasm` — WASM module linker (remap type index nyata)
- [x] `brak-bitcode` — persistent IR cache 🚧 **eksperimental** (JSON, belum terintegrasi)

### Phase 8 — Debug Info (`v0.9`) — 🚧 SEBAGIAN (per prioritas.md)

> **Peringatan**: prioritas.md menempatkan DWARF/PDB sebagai **backlog (Fase 6, "⏳ BACKLOG")**.
> Kode sebagian sudah ada (`dwarf.rs`, `codeview.rs`, `pe.rs`), tapi **belum terverifikasi penuh**
> dengan debugger dan tidak boleh dianggap selesai.

- [x] `#line` directives di codegen-c — `CWriter::emit_inst()` emits `#line` on line change
- [~] DWARF generation di codegen-obj (ELF & Mach-O .o) — `dwarf.rs` + `elf.rs`/`macho_obj.rs`
  - [~] `.debug_line`, `.debug_info`, `.debug_abbrev`, `.debug_str` di relocatable .o
  - [~] DWARF di ELF executable — `link_elf()` writes DWARF section headers + data
  - [~] DWARF di Mach-O executable — `link_macho()` writes `__DWARF` segment
  - [~] `parse_elf`/`parse_macho` extract DWARF into `debug_sections` — preserved through linking
- [~] CodeView generation di codegen-obj (PE) — `codeview.rs` + `coff.rs`
  - [~] `S_GPROC32` + `S_END` per function (function symbols)
  - [~] `DEBUG_S_LINES` subsection (line-to-address mapping)
  - [~] `.debug$S` section di COFF output
- [~] PE debug directory — `pe.rs` (RSDS header, `IMAGE_DEBUG_DIRECTORY`, `.rdata`)
- [ ] Verified dengan debugger nyata (WinDbg/CDB) — manual test belum dilakukan
- [ ] PDB file generation — deferred
- **Aksi**: sebelum dianggap selesai, verifikasi dengan debugger + pindahkan status di prioritas.md

### Phase 9 — Maturity (`v1.0`) — ✓ SELESAI* (dengan catatan)

- [x] Pipeline optimasi: **CLI** 8 pass (Fold, CP, Inline, GVN, LICM, JT, TCO, DCE); **brak-easy** 5 pass default
- [x] Inlining, LICM, TCO (self tail-call nyata), Jump Threading, Constant Folding
- [x] Iterative optimization pass manager (convergence via ContentHash)
- [~] Self-hosting (Brak compile Brak) — infrastruktur siap (struct/enum), **belum diverifikasi** (lihat §5.10)
- [~] Stabil ABI untuk C bindings (via brak-polyglot) — binding tersedia, klaim stabilitas lintas versi belum divalidasi
- [x] Documentation: README, LANG_BRAK, LANG_LIT, POLYGLOT_GUIDE, daftar_api.md
- [x] `brak-easy` — pipeline level tinggi untuk konsumen library
- [ ] Benchmark suite vs LLVM
- [ ] SROA — **tidak ada** (crate `brak-opt-sroa` belum pernah dibuat; baris lama salah)
- Realita tambahan: daftar bug lengkap & status fase → `prioritas.md`

## 10. Design Principles & Workflow

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
| `BRK_` | `BRK_ERROR_TYPE` | C ABI constant — 🚧 rencana (C session API belum ada) |
| `brk_` | `brk_session_new` | C ABI function — 🚧 rencana (belum ada) |
| `brak.` | `brak.config.toml` | Config file — 🚧 rencana (belum ada parser config) |
| `.brk` | `main.brk` | Source file extension |
| `.bkr` | `output.bkr` | Brak bitcode IR — 🚧 rencana (brak-bitcode sekarang pakai file **`.json`** via serde_json) |

---

## 13. Struktur Repository

```
brak/
├── Cargo.toml             (workspace root)
├── prd.md
├── prioritas.md
├── daftar_api.md
├── brak-core/
├── brak-frontend/         (lexer::{AsciiLexer, BrakLexer}, parser::{Parser, Program})
├── brak-ir-ast/
├── brak-ir-hir/           (lower::HirLower, typeck::TypeChecker)
├── brak-ir-mir/           (lower::MirLower)
├── brak-ir-lir/           (lower::LirLower)
├── brak-opt-traits/       (PassManager, LirOptimizationPass)
├── brak-opt-fold/
├── brak-opt-cp/
├── brak-opt-gvn/
├── brak-opt-inline/
├── brak-opt-licm/
├── brak-opt-jt/
├── brak-opt-tco/
├── brak-opt-dce/
├── brak-opt-utils/
├── brak-codegen-traits/   (CodegenBackend, CodegenExecutable)
├── brak-codegen-obj/      (ObjBackend — ELF/PE/Mach-O)
├── brak-codegen-asm/      (dx86asm) — belum ada consumer
├── brak-codegen-c/        (CWriter)
├── brak-codegen-wasm/     (WAT text)
├── brak-codegen-llvm/     (LLVM IR text)
├── brak-link-traits/      (LinkerBackend, ObjectFile)
├── brak-link-native/      (PE/ELF/Mach-O linker)
├── brak-link-wasm/        (WASM module linker)
├── brak-link-archive/     (AR static lib)
├── brak-polyglot/         (normalisasi tipe lintas bahasa, FFI, gen C header & PyO3)
├── brak-lang-lit/         (bahasa Lit — konstanta saja, -> cross_lit.brk)
├── brak-test/             (IR/diff/exec snapshot testing)
├── brak-bitcode/          (eksperimental, serde_json cache)
├── brak-easy/             (EasyPipeline, OptLevel — pipeline 5 pass)
└── brak-tool/             (CLI: brak build | emit-ir | help)
```

Setiap crate punya `Cargo.toml` sendiri, semuanya di-root workspace.

---

## 14. Contoh Use Case Lengkap

> Skenario asli (grammar-driven: `grammar.lite.ebnf` → Brak generates parser/ast) 🚧 **BELUM ADA**
> — tidak ada generator parser berbasis grammar. Jalur nyata: tulis lexer/parser sendiri
> di atas token dasar + pipeline trasnformer.

Skenario nyata: kamu mau buat bahasa scripting sederhana "LiteLang" dengan Brak.

```rust
// 1. Lex & parse pakai parser umum (tokennya sama: Vec<brak_ir_ast::Token>)
use brak_core::SourceMap;
use brak_frontend::lexer::{AsciiLexer, BrakLexer};
use brak_frontend::parser::Parser;
use brak_ir_hir::hir::*;
use brak_ir_hir::lower::HirLower;

let sm = SourceMap::new("main.lite", src);
let tokens = AsciiLexer::new().lex(&sm);
let ast = Parser::new().parse(&tokens)?;

// 2. Lowering kustom: AST umum → HIR (peta fitur bahasa kamu)
let mut hir = HirLower::new().lower(ast).map_err(|e| e.to_string())?;
//   lalu transformasikan Hir sesuai semantik LiteLang...

// 3. Pipeline kebawah ke executable (API nyata)
use brak_ir_mir::lower::MirLower;
use brak_ir_lir::lower::LirLower;
use brak_opt_traits::PassManager;
use brak_opt_fold::ConstantFolding;
use brak_opt_cp::ConstantPropagation;
use brak_codegen_traits::CodegenBackend;
use brak_codegen_obj::ObjBackend;
use brak_link_traits::{LinkerBackend, ObjectFile};
use brak_link_native::NativeLinker;

let mir = MirLower::new().lower(hir);
let mut ll = LirLower::new();
ll.set_file_id(0);
let lir = ll.lower(mir);

let mut pm = PassManager::new();
pm.add_pass(Box::new(ConstantFolding));
pm.add_pass(Box::new(ConstantPropagation));
let lir = pm.run(lir)?;

let obj = ObjBackend::default().emit(&lir)?;
let exe = NativeLinker.link(
    &[ObjectFile { name: "main.o".into(), data: obj }],
    "main", 0x140000000,
)?;
std::fs::write("output.exe", exe.data)?;
```

Total: ~50 baris Rust untuk bahasa sederhana dengan compiler. Jalur librari
level tinggi yang lebih ringkas: `brak-easy` (lihat §7.1).
