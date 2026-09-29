# brak-opt-licm (Loop Invariant Code Motion)

Pass LICM mencari natural loop pada CFG LIR dan memindahkan instruksi invariant ke pre-header agar tidak dieksekusi berulang kali.

Pass hanya memindahkan instruksi dari whitelist tanpa efek samping atau trap (`Mov`, operasi aritmetika dan bitwise tertentu). Loop harus memiliki tepat satu predecessor di luar loop yang mendominasi header; kondisi lain dilewati.

Crate ini menggunakan analisis CFG, dominasi, dan natural loop dari `brak-opt-utils`. Nama pass-nya adalah `licm`.
