# RealMe — Panduan Praktis Brak & brak-easy

Dokumen ini adalah panduan *real-world* untuk menggunakan Brak Language Construction Toolkit, dengan fokus pada **brak-easy**: API tingkat tinggi yang menyederhanakan pipeline kompilasi.

---

## 1. Apa Itu brak-easy?

**brak-easy** adalah crate high-level API yang membungkus seluruh pipeline kompilasi Brak (Lexing → Parsing → HIR → MIR → LIR → Optimasi → Codegen → Linking) menjadi **beberapa baris kode**.

Tanpa brak-easy, Anda harus memanggil setiap komponen secara manual:

```rust
// Tanpa brak-easy — manual, panjang
let tokens = AsciiLexer::new().lex(&source_map);
let ast = parser.parse(&tokens)?;
let hir = HirLower::new().lower(ast)?;
let mut typeck = TypeChecker::new();
typeck.check(&hir)?;
let mir = MirLower::new().lower(hir)?;
let mut lir = LirLower::new().lower(mir);
lir = pass_manager.run(lir)?;
let obj = ObjBackend::default().emit(&lir)?;
let exe = NativeLinker.link(&[obj], "main", 0x400000)?;
```

Dengan brak-easy:

```rust
// Dengan brak-easy — 1 baris
EasyPipeline::new().build_executable("prog", source_code, "output.exe")?;
```

---

## 2. Instalasi & Setup

Tambahkan brak-easy ke `Cargo.toml` project Anda:

```toml
[dependencies]
brak-easy = { path = "path/to/brak/brak-easy" }
```

Atau jika berada dalam workspace Brak:

```toml
brak-easy = { path = "../brak-easy" }
```

---

## 3. API Reference

### 3.1 `EasyPipeline`

Struct utama yang mengatur seluruh pipeline kompilasi.

#### Method

| Method | Signature | Deskripsi |
|--------|-----------|-----------|
| `new()` | `fn new() -> Self` | Buat pipeline dengan konfigurasi default |
| `with_iterations()` | `fn with_iterations(self, n: usize) -> Self` | Set jumlah iterasi optimasi (default: 1) |
| `with_entry_point()` | `fn with_entry_point(self, name: &str) -> Self` | Set nama entry point (default: `"main"`) |
| `build_executable()` | `fn build_executable(&self, name: &str, source: &str, output: &str) -> BrakResult<()>` | Source → Executable (all-in-one) |
| `compile_to_lir()` | `fn compile_to_lir(&self, name: &str, source: &str) -> BrakResult<LirProgram>` | Source → LIR (debug/inspeksi) |
| `ast_to_lir()` | `fn ast_to_lir(&self, name: &str, ast: Program) -> BrakResult<LirProgram>` | AST → LIR (jika Anda punya parser sendiri) |
| `lir_to_executable()` | `fn lir_to_executable(&self, name: &str, lir: LirProgram, output: &str) -> BrakResult<()>` | LIR → Executable (codegen + link) |

### 3.2 Optimization Passes (Default)

Pipeline optimasi bawaan dijalankan secara otomatis:

1. **Inlining** — Menempelkan body fungsi ke tempat pemanggilan
2. **Constant Propagation** — Menyebarkan nilai konstanta ke seluruh penggunaan
3. **Constant Folding** — Mengevaluasi ekspresi konstanta saat kompilasi
4. **Global Value Numbering** — Mendeteksi dan menggabungkan ekspresi redundan
5. **Dead Code Elimination** — Menghapus kode yang tidak pernah dieksekusi

---

## 4. Contoh Penggunaan

### 4.1 Hello World Minimal

Program Brak paling sederhana — fungsi `main` mengembalikan angka:

```rust
use brak_easy::EasyPipeline;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let source = "fn main() -> i32 { 42 }";
    
    EasyPipeline::new()
        .build_executable("hello", source, "hello.exe")?;
    
    println!("hello.exe berhasil dibuat!");
    Ok(())
}
```

Kompilasi dan jalankan:

```bash
cargo run
./hello.exe
echo $?   # Output: 42
```

### 4.2 Kalkulator Sederhana

```rust
use brak_easy::EasyPipeline;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        fn add(a: i32, b: i32) -> i32 { a + b }
        fn mul(a: i32, b: i32) -> i32 { a * b }

        fn main() -> i32 {
            let x: i32 = add(10, 20);
            let y: i32 = mul(x, 2);
            y
        }
    "#;

    EasyPipeline::new()
        .build_executable("calc", source, "calc.exe")?;

    Ok(())
}
```

### 4.3 Multi-Optimasi (Iterative)

Untuk performa maksimal, tingkatkan jumlah iterasi optimasi:

```rust
EasyPipeline::new()
    .with_iterations(3)          // 3x optimasi berulang
    .with_entry_point("run")     // entry point bukan "main"
    .build_executable("prog", source, "prog.exe")?;
```

### 4.4 Debug: Lihat LIR Sebelum Codegen

Untuk inspeksi IR level rendah:

```rust
use brak_easy::EasyPipeline;

let pipeline = EasyPipeline::new();
let lir = pipeline.compile_to_lir("debug", source)?;

for func in &lir.functions {
    println!("Function: {}", func.name);
    for block in &func.blocks {
        for inst in &block.instructions {
            println!("  {:?}", inst);
        }
    }
}
```

### 4.5 Pipeline Manual (Partial)

Jika Anda ingin parsing sendiri tapi pakai brak-easy untuk sisanya:

```rust
use brak_easy::EasyPipeline;
use brak_frontend::lexer::{AsciiLexer, BrakLexer};
use brak_frontend::parser::Parser;
use brak_core::SourceMap;

let sm = SourceMap::new("prog", source);
let tokens = AsciiLexer::new().lex(&sm);
let ast = Parser::new().parse(&tokens)?;

let pipeline = EasyPipeline::new();
let lir = pipeline.ast_to_lir("prog", ast)?;
pipeline.lir_to_executable("prog", lir, "output.exe")?;
```

---

## 5. Pipeline Kompilasi (Visual)

```
Source Code (.brk)
    │
    ▼
┌─────────────────────────────┐
│  Lexer (AsciiLexer)         │  Tokenization
└──────────┬──────────────────┘
           │ tokens
           ▼
┌─────────────────────────────┐
│  Parser                     │  AST Construction
└──────────┬──────────────────┘
           │ AST (Program)
           ▼
┌─────────────────────────────┐
│  HirLower                   │  High-level IR + Type Checking
└──────────┬──────────────────┘
           │ HIR
           ▼
┌─────────────────────────────┐
│  MirLower                   │  Mid-level IR (CFG)
└──────────┬──────────────────┘
           │ MIR
           ▼
┌─────────────────────────────┐
│  LirLower                   │  Low-level IR (Register-based)
└──────────┬──────────────────┘
           │ LIR
           ▼
┌─────────────────────────────┐
│  PassManager (Optimasi)     │  Inline, CP, Fold, GVN, DCE
│  ─ diulang n iterasi ─      │
└──────────┬──────────────────┘
           │ LIR (optimized)
           ▼
┌─────────────────────────────┐
│  ObjBackend (Codegen)       │  → .obj / .o
└──────────┬──────────────────┘
           │ Object file
           ▼
┌─────────────────────────────┐
│  NativeLinker               │  → .exe / ELF / Mach-O
└─────────────────────────────┘
```

---

## 6. Tips & Best Practices

### 6.1 Pipeline Sesuai Kebutuhan

| Kebutuhan | Method |
|-----------|--------|
| Build sekali jadi | `build_executable()` |
| Debug IR level rendah | `compile_to_lir()` |
| Pakai parser kustom | `ast_to_lir()` + `lir_to_executable()` |
| Codegen aja (tanpa link) | Ambil output `ObjBackend` manual |

### 6.2 Optimasi

- **1 iterasi** — cukup untuk sebagian besar kode
- **2-3 iterasi** — untuk kode dengan banyak fungsi kecil (inlining benefit)
- **>3 iterasi** — diminishing returns, jarang diperlukan

### 6.3 Entry Point

Secara default Brak mencari fungsi `main`. Jika fungsi utama Anda berbeda:

```rust
EasyPipeline::new().with_entry_point("start")
```

### 6.4 Error Handling

Semua method brak-easy mengembalikan `BrakResult<()>` (= `Result<(), Box<dyn Error>>`). Selalu handle error:

```rust
match EasyPipeline::new().build_executable("x", source, "x.exe") {
    Ok(_) => println!("Sukses!"),
    Err(e) => eprintln!("Kompilasi gagal: {}", e),
}
```

---

## 7. Daftar Crate Terkait

| Crate | Fungsi |
|-------|--------|
| `brak-core` | Tipe dasar, Span, SourceMap, Error |
| `brak-frontend` | Lexer + Parser |
| `brak-ir-ast` | AST definition |
| `brak-ir-hir` | High-level IR + Type Checker |
| `brak-ir-mir` | Mid-level IR (CFG) |
| `brak-ir-lir` | Low-level IR (Register-based) |
| `brak-opt-traits` | Trait untuk optimization pass |
| `brak-opt-*` | Optimization passes (DCE, CP, GVN, Inline, dll) |
| `brak-codegen-traits` | Trait untuk codegen backend |
| `brak-codegen-obj` | Codegen → native object file |
| `brak-link-traits` | Trait untuk linker |
| `brak-link-native` | Linker → executable (PE/ELF/Mach-O) |

---

## 8. Troubleshooting

| Masalah | Penyebab | Solusi |
|---------|----------|--------|
| `Parser error` | Syntax Brak salah | Periksa tanda kurung, titik koma, tipe |
| `Typecheck error` | Mismatch tipe data | Pastikan tipe variabel dan fungsi cocok |
| `Failed to write executable` | Path output invalid | Pastikan direktori output ada |
| Optimasi terlalu lambat | Iterasi terlalu banyak | Kurangi `with_iterations()` ke 1-2 |

---

*Dibuat untuk memudahkan penggunaan Brak di dunia nyata. Lihat [PRD](prd.md) untuk gambaran arsitektur lengkap.*
