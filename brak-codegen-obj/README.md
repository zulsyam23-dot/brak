# brak-codegen-obj

Backend codegen utama Brak untuk menghasilkan object file dari LIR.

## Format

- ELF melalui `ObjectFormat::Elf`.
- COFF melalui `ObjectFormat::Coff`.
- Mach-O melalui `ObjectFormat::Macho`.

`ObjBackend::default()` memilih format berdasarkan host. Gunakan `ObjBackend::emit` atau helper `emit_obj` untuk menghasilkan byte object file; linking dilakukan terpisah oleh `brak-link-native`.

Modul DWARF dan CodeView tersedia, tetapi dukungan debug info masih parsial dan belum diverifikasi penuh dengan debugger nyata.
