# brak-opt-dce (Dead Code Elimination)

Pass ini menghapus instruksi yang hasil register-nya tidak digunakan dan fungsi yang tidak direferensikan oleh instruksi `Call`.

Retensi fungsi saat ini selalu memasukkan `main` dan mengumpulkan seluruh target `Call` dalam program; ini bukan analisis reachability transitif yang berawal hanya dari `main`. Instruksi dengan efek samping, terminator, serta `Div`/`Mod` dipertahankan.

Untuk build shared library, CLI melewati pass ini agar fungsi publik tidak terhapus. Nama pass-nya adalah `dce`.
