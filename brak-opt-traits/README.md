# brak-opt-traits

Modul standar untuk semua sistem optimasi di Brak.

## Kegunaan
- Mendefinisikan interface (Trait) `LirOptimizationPass`.
- Menyediakan **Pass Manager** yang mengatur urutan jalannya berbagai optimasi.
- Mendukung pemuatan plugin optimasi eksternal secara dinamis (.dll / .so).
