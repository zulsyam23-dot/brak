# Brak Language Construction Toolkit

Brak adalah toolkit modular untuk membangun bahasa pemrograman dari nol hingga menjadi file executable native, tanpa ketergantungan pada tool eksternal (seperti LLVM, GCC, atau MSVC).

## Panduan Penggunaan Bahasa

Untuk mempelajari cara menulis kode dan mengintegrasikan Brak dengan bahasa lain, silakan baca dokumen berikut:
- [Panduan Bahasa Brak (.brk)](docs/LANG_BRAK.md) - Bahasa utama sistem.
- [Panduan Bahasa Lit (.lit)](docs/LANG_LIT.md) - Bahasa alternatif sederhana.
- [Panduan Polyglot (C & Python)](docs/POLYGLOT_GUIDE.md) - Cara memanggil fungsi Brak dari bahasa lain.

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
- **brak-codegen-***: Mengubah LIR menjadi kode mesin (x86_64), C, LLVM, atau WASM.
- **brak-link-***: Menggabungkan file objek menjadi executable atau library (.exe, .dll, .a).

### 5. Fitur Unik
- **brak-polyglot**: Bridge untuk memanggil fungsi antar bahasa yang berbeda tanpa biaya (Zero-cost FFI).
- **brak-bitcode**: Sistem caching cerdas agar kompilasi ulang hanya memproses bagian yang berubah.

## Cara Menggunakan

Toolkit ini dikelola melalui satu program utama yaitu **brak-tool**.

```bash
# Menjalankan compiler
cargo run -p brak-tool -- build samples/hello.brk --output hello.exe

# Melihat representasi internal (misal: HIR)
cargo run -p brak-tool -- emit-ir samples/hello.brk --level hir
```

---
*Dibuat secara profesional untuk memastikan modularitas, kejujuran performa, dan kemudahan pengembangan bahasa.*
