# Panduan Bahasa Brak (.brk)

Brak adalah bahasa yang dikompilasi oleh toolkit ini. Bahasa dan compiler masih berkembang; contoh di bawah menggunakan sintaks yang didukung pipeline saat ini.

## Sintaks Dasar

### 1. Fungsi
Fungsi didefinisikan dengan kata kunci `fn`. Tipe kembalian ditulis eksplisit; gunakan `void` untuk fungsi tanpa nilai kembalian.

```brak
fn add(a: i32, b: i32) -> i32 {
    let result: i32 = a + b;
    result
}

fn main() -> i32 {
    let x: i32 = 10;
    let y: i32 = 20;
    add(x, y)
}
```

### 2. Variabel
Variabel didefinisikan menggunakan `let`. Brak adalah bahasa *statically typed*.

```brak
let angka: i32 = 42;
let desimal: f64 = 3.14;
let benar: bool = true;
let teks: string = "Halo Brak";
```

### 3. Struktur Data (Struct & Enum)

**Struct:**
Digunakan untuk mengelompokkan data terkait.

```brak
struct Point {
    x: i32,
    y: i32,
}

fn main() -> i32 {
    let p: Point = Point { x: 10, y: 20 };
    p.x = 15;
    p.x + p.y
}
```

**Enum:**
Digunakan untuk tipe data yang bisa memiliki beberapa varian.

```brak
enum State {
    Idle,
    Running,
    Done,
}

fn label(s: State) -> i32 {
    match s {
        State.Idle => 0,
        State.Running => 1,
        State.Done => 2,
    }
}
```

- Konstruksi: `EnumName.Variant` atau `EnumName.Variant(arg, ...)`.
- Match **wajib exhaustif**: jika tidak ada arm wildcard (`_`) atau binding,
  semua varian harus tercakup — compiler akan menolak program yang kurang.
- Payload destructuring: `Shape.Circle(r) => r * r * 3`.
- Nilai enum direpresentasikan sebagai pointer ke agregat stack `[tag, payload...]`.

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
let i: i32 = 0;
while i < 5 {
    i = i + 1;
}
```

## Tipe Data yang Didukung
- `i32`, `i64`: Bilangan bulat 32-bit dan 64-bit.
- `f32`, `f64`: Bilangan desimal (float) 32-bit dan 64-bit.
- `bool`: Nilai kebenaran (`true` atau `false`).
- `string`: Teks (UTF-8).
- `void`: Digunakan untuk fungsi yang tidak mengembalikan nilai.

Operasi float dan beberapa fitur tipe agregat masih memiliki keterbatasan di tahap codegen; keberhasilan parse/type-check tidak menjamin semua operasi tersedia end-to-end.

## Praktik Terbaik (Best Practices)

1. **Gunakan Penamaan Konsisten**: Gunakan `PascalCase` untuk Struct/Enum dan `snake_case` untuk fungsi/variabel.
2. **Modularitas**: Pecah program menjadi fungsi-fungsi kecil; optimizer dapat melakukan inlining pada sebagian call site yang memenuhi syarat.
3. **Pengecekan Tipe**: Perhatikan diagnostic dari compiler. Type checking mendeteksi sejumlah kesalahan statis, tetapi tidak menjamin bebas dari semua error runtime.
4. **Alur Kerja Aman**: Sebelum melakukan build final, gunakan `--level mir` untuk melihat apakah logika CFG (Control Flow Graph) sudah sesuai dengan ekspektasi Anda.

## Cara Kompilasi
Gunakan `brak-tool` untuk mengompilasi file `.brk` menjadi executable:

```bash
# Build executable
brak build main.brk --output main.exe

# Emit IR untuk debugging
brak emit-ir main.brk --level hir
```

