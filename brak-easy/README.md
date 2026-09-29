# brak-easy

API tingkat tinggi untuk menjalankan pipeline kompilasi Brak dari source string sampai object file atau executable native.

```rust
use brak_easy::{EasyPipeline, OptLevel};

let source = "fn main() -> i32 { 42 }";
EasyPipeline::new()
    .with_opt_level(OptLevel::Default)
    .build_executable("hello", source, "hello.exe")?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

## API utama

- `compile_to_lir` dan `ast_to_lir` untuk memperoleh LIR yang sudah dioptimasi.
- `compile_to_object` untuk menghasilkan object file tanpa linking.
- `build_executable` dan `lir_to_executable` untuk membuat executable.
- `with_entry_point`, `with_iterations`, `with_verbose`, dan `without_pass` untuk mengatur pipeline.

`OptLevel::None` mematikan pass; `Less` menjalankan fold dan DCE; `Default` menjalankan inline, CP, fold, GVN, dan DCE satu iterasi; `Aggressive` memakai pass default dengan empat iterasi. Backend dan linker menggunakan format host.
