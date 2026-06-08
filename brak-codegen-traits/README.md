# brak-codegen-traits

Interface standar untuk semua Backend generator kode di Brak.

## Kegunaan
Memastikan semua backend (ASM, C, LLVM, WASM) memiliki cara yang sama untuk menerima LIR dan menghasilkan output, sehingga mudah untuk menambah target CPU baru di masa depan.
