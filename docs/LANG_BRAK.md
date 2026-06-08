# Panduan Bahasa Brak (.brk)

Brak adalah bahasa pemrograman utama dalam toolkit ini. Ia dirancang untuk menjadi bahasa sistem yang aman, modular, dan memiliki performa tinggi.

## Sintaks Dasar

### 1. Fungsi
Fungsi didefinisikan dengan kata kunci `fn`. Setiap fungsi harus memiliki tipe kembalian eksplisit (gunakan `Void` jika tidak ada).

```brak
fn add(a: I32, b: I32) -> I32 {
    let result: I32 = a + b;
    result
}

fn main() -> I32 {
    let x: I32 = 10;
    let y: I32 = 20;
    add(x, y)
}
```

### 2. Variabel
Variabel didefinisikan menggunakan `let`. Brak adalah bahasa *statically typed*.

```brak
let angka: I32 = 42;
let desimal: F64 = 3.14;
let benar: Bool = true;
let teks: String = "Halo Brak";
```

### 3. Kontrol Alur

**If-Else:**
```brak
if x > 10 {
    // lakukan sesuatu
} else {
    // lakukan yang lain
}
```

**While Loop:**
```brak
let i: I32 = 0;
while i < 5 {
    i = i + 1;
}
```

## Tipe Data yang Didukung
- `I32`, `I64`: Bilangan bulat 32-bit dan 64-bit.
- `F32`, `F64`: Bilangan desimal (float) 32-bit dan 64-bit.
- `Bool`: Nilai kebenaran (`true` atau `false`).
- `String`: Teks (UTF-8).
- `Void`: Digunakan untuk fungsi yang tidak mengembalikan nilai.

## Cara Kompilasi
Gunakan `brak-tool` untuk mengompilasi file `.brk` menjadi executable:

```bash
cargo run -p brak-tool -- build nama_file.brk --output hasil.exe
```
