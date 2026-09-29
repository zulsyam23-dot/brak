# brak-core

Tipe dasar bersama yang digunakan crate Brak.

## API

- `Span`, `SourceLoc`, dan `SourceMap` untuk merepresentasikan rentang serta lokasi sumber.
- `Diagnostic`, `Diagnostics`, dan `Severity` untuk mengumpulkan diagnostic.
- `ContentHash` untuk hashing konten IR.
- `Result<T>` sebagai alias `Result` dengan error dinamis.
- `Version` dan `BRAK_VERSION` untuk versi toolkit.

Crate lain mengandalkan tipe-tipe ini agar format lokasi, diagnostic, dan error konsisten.
