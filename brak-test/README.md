# brak-test

Helper pengujian compiler untuk snapshot IR, diagnostic, dan stdout executable.

## API

- `SnapshotTester::new(directory, update)` menyimpan IR sebagai YAML; `assert_snapshot` membuat snapshot yang belum ada atau membandingkannya dengan yang tersimpan.
- `DiagnosticTester::assert_has_error` dan `assert_has_warning` mencari pesan diagnostic berdasarkan severity dan substring.
- `ExecutionTester::assert_output(path, expected)` menjalankan executable dan membandingkan stdout yang sudah di-trim. Helper ini tidak membandingkan exit code.

Semua helper mengembalikan `brak_core::Result`, sehingga kegagalan dapat diteruskan sebagai error pengujian.
