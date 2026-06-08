# Panduan Bahasa Lit (.lit)

**LitLang** adalah bahasa pemrograman alternatif yang jauh lebih sederhana dalam ekosistem Brak. Ia dirancang untuk mendemonstrasikan betapa mudahnya menambahkan dukungan bahasa baru ke dalam pipeline compiler Brak.

## Sintaks Sederhana
Lit menggunakan gaya penulisan minimalis.

```lit
// Definisi fungsi sederhana
fn tambah(a: I32, b: I32) -> I32 {
    let c: I32 = a + b;
    c
}

// Fungsi utama
fn main() -> I32 {
    let x: I32 = 100;
    tambah(x, 50)
}
```

## Perbedaan Utama dengan Brak
- **Parsing Cepat**: Parser Lit jauh lebih ringan dan cepat karena fiturnya yang terbatas.
- **Tujuan Khusus**: Lit sering digunakan untuk menulis skrip kecil atau modul pembantu yang akan dipanggil oleh kode Brak melalui Polyglot.

## Cara Kompilasi
Anda bisa mengompilasi file `.lit` menggunakan tool yang sama:

```bash
cargo run -p brak-tool -- build program_saya.lit --output program.exe
```

Atau melihat HIR (High-level IR) yang dihasilkan untuk memastikan kode Anda valid:
```bash
cargo run -p brak-tool -- emit-ir program_saya.lit --level hir
```
