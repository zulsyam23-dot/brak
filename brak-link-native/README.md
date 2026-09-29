# brak-link-native

Linker native Brak yang menggabungkan object file menjadi executable tanpa memanggil linker sistem eksternal.

`NativeLinker::link(objects, entry, base_addr)` menghasilkan executable PE, ELF, atau Mach-O sesuai format object. `link_shared` menghasilkan DLL Windows; shared library ELF belum didukung dan mengembalikan error.

Tipe object bersama (`ObjectFile`, `LinkerOutput`, dan `LinkerBackend`) didefinisikan di `brak-link-traits`.
