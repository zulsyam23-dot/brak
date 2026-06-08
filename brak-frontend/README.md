# brak-frontend

Modul ini bertanggung jawab untuk membaca kode sumber mentah dan mengubahnya menjadi struktur data yang dipahami komputer (AST).

## Kegunaan
- **Lexer**: Mengubah teks program menjadi urutan token (kata kunci, simbol, angka).
- **Parser**: Mengubah token menjadi **Abstract Syntax Tree (AST)** berdasarkan aturan tata bahasa Brak.

## Cara Pemakaian
Input berupa `SourceMap` dari `brak-core` dan outputnya adalah `Program` (AST) dari `brak-ir-ast`.
