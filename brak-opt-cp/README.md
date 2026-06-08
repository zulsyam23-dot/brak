# brak-opt-cp (Constant Propagation)

Optimasi yang mencari variabel dengan nilai tetap (konstan).

## Kegunaan
Jika ada kode `let x = 5; let y = x + 10;`, optimasi ini akan langsung mengubahnya menjadi `y = 15` saat kompilasi, sehingga CPU tidak perlu menghitungnya lagi saat program dijalankan.
