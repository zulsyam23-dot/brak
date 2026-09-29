# brak-tool

CLI utama Brak, dengan subcommand `build` dan `emit-ir`.

## Perintah

- `brak build <FILES...>` mengompilasi `.brk`/`.lit` dan menerima object `.o`/`.obj` serta archive `.a`/`.lib` sebagai input. Opsi mencakup `--entry`, `--output`, `--shared`, dan konfigurasi optimizer.
- `brak emit-ir <FILE>` menampilkan token, AST, HIR, MIR, LIR, assembly, atau object file melalui opsi `--level`. Format JSON/YAML tersedia untuk representasi IR yang mendukung serialisasi.

Codegen C, LLVM, WASM, dan generator binding bahasa lain tidak tersedia pada CLI saat ini. Jalankan `brak --help` atau `brak <COMMAND> --help` untuk daftar opsi terbaru.
