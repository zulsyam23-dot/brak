# Samples

Contoh source untuk Brak (`.brk`) dan Lit (`.lit`). Jalankan file Brak dengan `cargo run -p brak-tool -- build samples/hello.brk`; file Lit dapat diberikan sebagai input `brak build`.

## Contoh utama

- `hello.brk`, `simple.brk`, dan `calc.brk`: program dasar dan ekspresi.
- `fib.brk`: fungsi rekursif.
- `call.brk`, `math_lib.brk`, `use_math.brk`, dan `multi_call.brk`: komposisi dan pemanggilan fungsi.
- `hello_lit.lit` dan `greet_lib.lit`: sintaks Lit yang didukung saat ini.
- `cross_lit.brk`: contoh integrasi `.brk` dan `.lit` melalui pipeline compiler bersama.
- `demo/`: contoh tambahan.

Lihat setiap file untuk fitur spesifik yang didemonstrasikan; contoh ini bukan jaminan bahwa seluruh fitur bahasa sudah matang.
