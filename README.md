# Brak Language Construction Toolkit

Brak adalah toolkit modular untuk membangun bahasa pemrograman dari nol hingga menjadi file executable native, tanpa ketergantungan pada tool eksternal (seperti LLVM, GCC, atau MSVC).

## Panduan Penggunaan Bahasa

Untuk mempelajari cara menulis kode, silakan baca dokumen berikut:
- [Panduan Bahasa Brak (.brk)](docs/LANG_BRAK.md) - Bahasa utama sistem.
- [Panduan Bahasa Lit (.lit)](docs/LANG_LIT.md) - Bahasa alternatif sederhana.

## Arsitektur Proyek

Proyek ini dibagi menjadi puluhan modul (crates) yang masing-masing menangani satu tahap spesifik dalam pipeline kompilasi:

### 1. Inti & Frontend
- **brak-core**: Tipe dasar, penanganan error, dan pemetaan kode sumber.
- **brak-frontend**: Lexer dan Parser untuk mengubah kode sumber menjadi AST.
- **brak-lang-lit**: Implementasi bahasa alternatif "Lit" yang sangat sederhana.

### 2. Representasi Internal (IR)
- **brak-ir-ast**: Abstract Syntax Tree (representasi langsung dari kode).
- **brak-ir-hir**: High-level IR untuk validasi tipe (Type Checking).
- **brak-ir-mir**: Mid-level IR dengan Control Flow Graph (CFG) dan variabel lokal.
- **brak-ir-lir**: Low-level IR yang berbasis register, siap untuk optimasi dan codegen.

### 3. Optimasi (`brak-opt-*`)
Berbagai modul untuk membuat kode lebih cepat dan kecil:
- **DCE**: Menghapus kode mati.
- **CP**: Menyederhanakan konstanta.
- **GVN**: Menghapus perhitungan redundan.
- **Inline**: Memasukkan isi fungsi ke pemanggilnya.
- **LICM**: Mengeluarkan kode invariant dari loop.

### 4. Codegen & Linker
- **brak-codegen-obj**: Mengubah LIR menjadi object file native.
- **brak-codegen-asm**: Menghasilkan teks assembly.
- **brak-link-***: Menggabungkan file objek menjadi executable atau library (.exe, .dll, .a).

### 5. Fitur Unik
- **brak-bitcode**: Cache eksperimental untuk AST/HIR/MIR/LIR dalam JSON; belum terintegrasi ke pipeline kompilasi.

## Cara Menggunakan

### 1. Instalasi
Pastikan Anda memiliki [Rust](https://rustup.rs/) terinstal. Clone repositori ini dan bangun toolkit:

```bash
git clone https://github.com/user/brak.git
cd brak
cargo build --release
```

### 2. Kompilasi Program
Gunakan **brak-tool** untuk mengompilasi file `.brk` menjadi executable native:

```bash
# Build executable standar
./target/release/brak build samples/hello.brk --output hello.exe

# Menjalankan dengan cargo (untuk pengembangan)
cargo run -p brak-tool -- build samples/hello.brk --output hello.exe
```

### 3. Alur Kerja Pengembangan yang Aman
Untuk memastikan kode Anda berjalan lancar tanpa kesalahan:
- **Gunakan Tipe Eksplisit**: Brak sangat ketat terhadap tipe data. Selalu definisikan tipe pada `let` dan `fn`.
- **Cek Representasi Internal**: Jika terjadi error aneh, lihat IR di level tertentu untuk debug:
  ```bash
  brak emit-ir samples/hello.brk --level hir  # Cek tipe data
  brak emit-ir samples/hello.brk --level mir  # Cek alur kontrol (CFG)
  brak emit-ir samples/hello.brk --level obj  # Hasilkan object file native
  ```
- **Verifikasi dengan Testing**: Selalu jalankan test suite jika Anda mengubah kompilator:
  ```bash
  cargo test
  ```

## Status Proyek
- **Self-hosting**: Belum diverifikasi.
- **Zero-Dependency**: Tidak butuh LLVM/GCC terinstal di sistem target.
- **Cache IR**: API eksperimen tersedia melalui `brak-bitcode`; bukan bagian dari build CLI.
- **Optimasi**: CLI dan `brak-easy` memakai pipeline pass yang berbeda; lihat dokumentasi crate untuk batas tiap pass.

---
*Dibuat secara profesional untuk memastikan modularitas, kejujuran performa, dan kemudahan pengembangan bahasa.*
