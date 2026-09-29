# brak-opt-tco

Pass tail-call optimization untuk LIR Brak. Saat pemanggilan langsung ke fungsi yang sama berada pada posisi tail, pass menggantinya dengan pemindahan argumen ke parameter dan lompatan ke blok masuk fungsi.

Pass mengenali bentuk `Call; Ret` dan bentuk `Call; Mov hasil; Ret hasil`. Argumen ditampung lebih dahulu agar penyalinan parameter tidak mengubah nilai argumen yang masih dibutuhkan.

Pemanggilan rekursif mutual, pemanggilan tidak langsung, dan pemanggilan yang bukan tail call tidak dioptimalkan.

## Penggunaan

```rust
use brak_opt_tco::TailCallOptimization;
use brak_opt_traits::PassManager;

let mut passes = PassManager::default();
passes.add_pass(Box::new(TailCallOptimization));
let optimized = passes.run(program)?;
```

Nama pass yang dilaporkan adalah `tco`.
