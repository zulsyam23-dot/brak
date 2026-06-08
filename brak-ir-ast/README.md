# brak-ir-ast

Mendefinisikan **Abstract Syntax Tree (AST)**, yaitu representasi paling awal dari struktur kode setelah di-parse.

## Kegunaan
- Menjadi "bahasa komunikasi" antara Frontend dan tahap Lowering berikutnya.
- Menyimpan struktur asli program (fungsi, variabel, ekspresi) beserta lokasi kode aslinya (span).
- Mendukung serialisasi (JSON/YAML) untuk keperluan debugging.
