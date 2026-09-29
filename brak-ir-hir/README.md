# brak-ir-hir

High-level Intermediate Representation Brak, berikut lowering AST dan pemeriksaan tipe.

## Tahap

- `HirLower` mengubah AST menjadi struktur HIR.
- `TypeChecker` memeriksa konsistensi tipe dan menghasilkan diagnostic untuk program yang tidak valid.
- HIR menjadi input `MirLower` pada tahap berikutnya.

HIR mempertahankan struktur tingkat tinggi yang masih berguna untuk validasi sebelum lowering ke control-flow graph MIR.
