# brak-easy

High-level API untuk pipeline kompilasi Brak — source code langsung jadi executable dalam 1 panggilan fungsi.

## Kegunaan

- **Simplified Pipeline**: Lexing → Parsing → HIR → MIR → LIR → Optimasi → Codegen → Linking, otomatis
- **Optimasi Bawaan**: Inlining, Constant Propagation, Constant Folding, GVN, DCE — aktif secara default
- **Konfigurabel**: Atur jumlah iterasi optimasi dan entry point function

## Cara Pemakaian

```rust
use brak_easy::EasyPipeline;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let source = "fn main() -> i32 { 42 }";
    EasyPipeline::new().build_executable("hello", source, "hello.exe")?;
    Ok(())
}
```

## Method Tersedia

| Method | Fungsi |
|--------|--------|
| `build_executable()` | Source → Executable (all-in-one) |
| `compile_to_lir()` | Source → LIR (inspeksi debug) |
| `compile_to_object()` | Source → Object file bytes (tanpa linking) |
| `ast_to_lir()` | AST → LIR (pakai parser kustom) |
| `lir_to_executable()` | LIR → Executable (codegen + link) |
| `with_opt_level(level)` | Set level optimasi (`None`/`Less`/`Default`/`Aggressive`) |
| `without_pass(name)` | Matikan satu pass (mis. `"inline"`, `"cp"`, `"fold"`, `"gvn"`, `"dce"`) |
| `with_verbose(true)` | Log aktivitas optimizer |
| `with_iterations(n)` | Set jumlah iterasi optimasi |
| `with_entry_point(name)` | Ubah entry point (default: "main") |

```rust
// Contoh: build cepat tanpa inline
EasyPipeline::new()
    .with_opt_level(OptLevel::Less)
    .without_pass("inline")
    .build_executable("hello", source, "hello.exe")?;
```

## Dependencies

Semua crate Brak: `brak-core`, `brak-frontend`, `brak-ir-ast`, `brak-ir-hir`, `brak-ir-mir`, `brak-ir-lir`, `brak-opt-traits`, `brak-opt-*` (dce, cp, fold, gvn, inline), `brak-codegen-traits`, `brak-codegen-obj`, `brak-link-traits`, `brak-link-native`.
