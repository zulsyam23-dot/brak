# brak-opt-gvn (Global Value Numbering)

Pass LIR ini menghilangkan perhitungan redundan dengan memakai kembali hasil yang sudah tersedia di register.

Saat ini tabel nilai berlaku **per basic block**, bukan lintas blok. Pass mengenali operasi aritmetika dan bitwise tertentu serta operasi unary `Neg` dan `Not`; operand untuk operasi komutatif dinormalisasi agar urutannya tidak memengaruhi pencocokan. Penulisan ulang nilai register membatalkan entri terkait.

Nama pass untuk `PassManager` adalah `gvn`. Karena analisis tidak lintas blok, README ini tidak menjanjikan eliminasi redundansi antar blok kontrol alur.
