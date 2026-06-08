# brak-opt-gvn (Global Value Numbering)

Optimasi untuk mencari perhitungan yang redundan.

## Kegunaan
Jika Anda menulis `(a+b)` berkali-kali, optimasi ini akan menyadari bahwa hasilnya sama, menyimpannya di satu register, dan menggunakan kembali hasil tersebut daripada menghitung ulang.
