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

### 3. Struktur Data (Struct & Enum)

**Struct:**
Digunakan untuk mengelompokkan data terkait.

```brak
struct Point {
    x: I32,
    y: I32,
}

fn main() -> I32 {
    let p: Point = Point { x: 10, y: 20 };
    p.x = 15;
    p.x + p.y
}
```

**Enum:**
Digunakan untuk tipe data yang bisa memiliki beberapa varian.

```brak
enum Result {
    Ok(I32),
    Error,
}
```

### 4. Kontrol Alur

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

## Praktik Terbaik (Best Practices)

1. **Gunakan Penamaan Konsisten**: Gunakan `PascalCase` untuk Struct/Enum dan `snake_case` untuk fungsi/variabel.
2. **Modularitas**: Pecah kode menjadi fungsi-fungsi kecil. Brak memiliki optimizer inlining yang sangat efisien, jadi jangan takut dengan overhead panggilan fungsi.
3. **Pengecekan Tipe**: Selalu perhatikan pesan error dari kompilator. Brak melakukan analisis statis yang mendalam di level HIR untuk mencegah error runtime.
4. **Alur Kerja Aman**: Sebelum melakukan build final, gunakan `--level mir` untuk melihat apakah logika CFG (Control Flow Graph) sudah sesuai dengan ekspektasi Anda.

## Cara Kompilasi
Gunakan `brak-tool` untuk mengompilasi file `.brk` menjadi executable:

```bash
# Build executable
brak build main.brk --output main.exe

# Emit IR untuk debugging
brak emit-ir main.brk --level hir
```
