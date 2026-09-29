# brak-link-archive

Penulis dan parser archive object untuk format library statis `.a` dan `.lib`.

## API

- `ArchiveWriter::new(format)`, `add_entry(name, data)`, dan `write()` untuk membuat archive.
- `ArchiveFormat::Unix` dan `ArchiveFormat::Windows` memilih format keluaran.
- `parse_archive(data)` membaca member object dan mengabaikan member indeks/nama khusus.

Writer membangun indeks simbol dari simbol global terdefinisi yang dapat dibaca dari object ELF, COFF, atau Mach-O. `brak-tool build` menerima archive sebagai input dan menautkan member object-nya.
