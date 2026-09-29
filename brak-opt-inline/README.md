# brak-opt-inline (Function Inlining)

Pass ini mengganti pemanggilan fungsi yang memenuhi syarat dengan blok LIR callee di dalam caller.

Saat ini callee harus memiliki kurang dari 20 instruksi dan tidak boleh memanggil dirinya sendiri. Pass mengulang proses sampai tidak ada call site lain yang memenuhi syarat. Fungsi rekursif langsung dibiarkan untuk pass tail-call optimization.

Tambahkan `Inlining` ke `PassManager`; nama pass-nya adalah `inline`.
