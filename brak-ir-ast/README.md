# brak-ir-ast

Definisi Abstract Syntax Tree yang dihasilkan parser Brak.

AST menyimpan item dan ekspresi program beserta span sumber. `Program` menjadi input lowering HIR; tipe AST juga mendukung serialisasi untuk inspeksi dan pengujian.

Crate ini mendefinisikan struktur data, bukan lexer, parser, type checker, atau backend codegen.
