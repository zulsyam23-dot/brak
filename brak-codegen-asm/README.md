# brak-codegen-asm

Backend eksperimental yang menerjemahkan LIR menjadi teks assembly x86_64 bergaya Intel/NASM. `AsmBackend::emit` mengembalikan byte UTF-8 dari teks assembly, bukan machine code atau executable.

Backend memakai `SimpleAlloc` untuk memetakan virtual register selama emission. Crate ini terpisah dari backend object file yang dipakai pipeline build utama.

## API

Gunakan `brak_codegen_asm::emit_asm(&program)` untuk memperoleh teks assembly, atau `AsmBackend` melalui trait `CodegenBackend` untuk memperoleh byte output.
