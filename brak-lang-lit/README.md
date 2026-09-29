# brak-lang-lit

Crate ini menyediakan parser sederhana untuk bahasa Lit dan CLI `brak-lit`. Parser mengubah sumber Lit menjadi HIR Brak; CLI melanjutkan lowering ke MIR/LIR lalu menghasilkan object file native.

## Batasan bahasa

Satu fungsi ditulis sebagai ekspresi literal bertipe. Bentuk yang didukung saat ini mencakup literal integer dan string; operator, pemanggilan fungsi, serta ekspresi identifier belum didukung.

```lit
fn main() -> I32 = 42;
```

Jalankan CLI dari root workspace:

```powershell
cargo run -p brak-lang-lit -- samples/hello_lit.lit
```

Perintah tersebut menghasilkan `samples/hello_lit.o`.

## API

```rust
let program = brak_lang_lit::compile_lit_to_hir(source, "example.lit")?;
```

`compile_lit_to_hir` mengembalikan `HirProgram` atau `LitError` yang memuat pesan, span, dan path sumber.
