# brak-ir-mir

**Mid-level Intermediate Representation (MIR)**.

## Kegunaan
- **Control Flow Graph (CFG)**: Mengubah kode linear menjadi graf alur program (cabang `if`, perulangan `while`).
- **Local Variable Management**: Mengelola variabel lokal dan cakupannya (*scope*).
- Menjadi jembatan antara HIR yang masih mirip bahasa manusia ke LIR yang mendekati bahasa mesin.
