# brak-codegen-traits

Kontrak bersama untuk backend codegen Brak yang menerima `LirProgram`.

## API

- `CodegenBackend` menyediakan `name()` dan `emit()`, yang menghasilkan byte output atau `brak_core::Result`.
- `CodegenExecutable` memperluas kontrak tersebut dengan `emit_executable(program, entry)` untuk backend yang dapat menghasilkan executable.

Backend yang tersedia di workspace mencakup object file (`brak-codegen-obj`) dan teks assembly (`brak-codegen-asm`). Trait ini mendefinisikan antarmuka saja; ia tidak memilih target atau menjamin dukungan fitur tertentu pada tiap backend.
