# brak-bitcode

Cache eksperimental untuk menyimpan AST, HIR, MIR, dan LIR sebagai JSON di disk.

## Status

Crate ini belum terhubung ke pipeline `brak-tool` atau `brak-easy`. Pemanggil menyediakan hash cache sendiri; hash tersebut tidak otomatis mencakup versi compiler. Karena itu, cache ini belum menjamin incremental compilation atau validasi kompatibilitas entri.

## API

`BitcodeCache::new(path)` menyiapkan direktori untuk tiap level IR. API `get_or_compute_ast`, `get_or_compute_hir`, `get_or_compute_mir`, dan `get_or_compute_lir` membaca entri berdasarkan hash atau menjalankan closure lalu menyimpan hasilnya. Tersedia juga `contains`, `invalidate`, dan `clear_all`.

Entri disimpan sebagai file JSON bernama hash di subdirektori level IR, bukan sebagai format binary.
