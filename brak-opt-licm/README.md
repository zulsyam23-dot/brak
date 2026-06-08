# brak-opt-licm (Loop Invariant Code Motion)

Optimasi khusus untuk mempercepat perulangan (loop).

## Kegunaan
Jika ada perhitungan di dalam loop yang hasilnya selalu sama setiap kali berputar, optimasi ini akan "mengangkat" perhitungan tersebut ke luar loop agar hanya dihitung satu kali saja.
