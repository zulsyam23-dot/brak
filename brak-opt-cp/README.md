# brak-opt-cp (Constant Propagation)

Pass propagasi konstanta integer untuk LIR. Pass melacak nilai register lintas basic block menggunakan analisis dataflow dan hanya mengganti nilai pada titik ketika semua jalur masuk menyepakati konstanta yang sama.

Operasi `Add`, `Sub`, dan `Mul` dengan operand konstan juga dilipat menjadi `Mov`. Untuk menghindari perubahan semantik backend, substitusi dibatasi pada opcode yang menerima immediate.

Tambahkan `ConstantPropagation` ke `PassManager`; nama pass-nya adalah `cp`.
