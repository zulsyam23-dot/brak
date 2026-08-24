# Panduan Bahasa Lit (.lit)

**LitLang** adalah bahasa pemrograman alternatif yang jauh lebih sederhana dalam ekosistem Brak. Ia dirancang untuk mendemonstrasikan betapa mudahnya menambahkan dukungan bahasa baru ke dalam pipeline compiler Brak.

## Sintaks Sederhana
Lit saat ini hanya mendukung fungsi konstanta — satu ekspresi literal per fungsi:

```lit
fn versi() -> i32 = 42;
fn salam() -> string = "Halo dari Lit";
```

Sintaks dengan body `{ ... }`, `let`, dan pemanggilan fungsi (contoh di bawah)
belum didukung grammar Lit saat ini:

```lit
// BELUM DIDUKUNG — dokumentasi tujuan jangka panjang:
// fn tambah(a: i32, b: i32) -> i32 { a + b }
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
