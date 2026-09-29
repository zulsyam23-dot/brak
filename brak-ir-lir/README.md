# brak-ir-lir

Low-level Intermediate Representation Brak berbasis virtual register dan basic block.

`LirLower` mengubah MIR menjadi LIR. LIR menjadi input pass optimasi dan backend codegen; object file atau executable dihasilkan oleh crate backend dan linker terpisah.
