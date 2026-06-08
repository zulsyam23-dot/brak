# brak-link-native

Linker asli untuk membuat executable mandiri.

## Kegunaan
- Menggabungkan beberapa file objek menjadi satu `.exe` (Windows) atau binary (Linux/macOS).
- Mengatur alamat memori agar semua pemanggilan fungsi tersambung dengan benar.
- **Tanpa dependensi external**: Anda tidak butuh `link.exe` dari Visual Studio atau `ld` dari GCC.
