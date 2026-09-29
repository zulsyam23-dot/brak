# Panduan Bahasa Lit (.lit)

Lit adalah bahasa kecil yang didukung oleh parser dan pipeline Brak. Implementasinya masih terbatas pada fungsi yang mengembalikan literal integer atau string.

## Sintaks Sederhana
Lit saat ini hanya mendukung fungsi konstanta — satu ekspresi literal per fungsi:

```lit
fn versi() -> I32 = 42;
fn salam() -> String = "Halo dari Lit";
```

Body blok, variabel, operator, dan pemanggilan fungsi belum didukung parser Lit:

```lit
// BELUM DIDUKUNG — dokumentasi tujuan jangka panjang:
// fn tambah(a: i32, b: i32) -> i32 { a + b }
```

Tipe bawaan dalam anotasi Lit ditulis dengan nama kapital seperti `I32`, `I64`, `F32`, `F64`, `Bool`, `String`, dan `Void`. Ekspresi fungsi tetap dibatasi pada literal integer atau string.

## Cara Kompilasi
CLI `brak-tool` menerima file `.lit` sebagai input build dan menghubungkannya ke executable native:

```bash
cargo run -p brak-tool -- build program_saya.lit --output program.exe
```

Binary khusus `brak-lit` menghasilkan object file `.o` dari satu file Lit:

```bash
cargo run -p brak-lang-lit -- samples/hello_lit.lit
```
