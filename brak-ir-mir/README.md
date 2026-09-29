# brak-ir-mir

Mid-level Intermediate Representation yang menurunkan HIR menjadi instruksi dan basic block dengan control flow eksplisit.

Gunakan `MirLower::new().lower(hir)` untuk membuat MIR. Hasilnya menjadi input `LirLower` di `brak-ir-lir`; crate ini tidak menjalankan optimasi atau menghasilkan object file.
