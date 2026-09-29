# brak-opt-jt

Pass jump threading untuk LIR Brak. Pass ini melewati rantai blok perantara yang hanya berisi instruksi `Jmp`, lalu memperbarui target label pada instruksi `Jmp` dan `Br`.

Resolusi rantai berhenti saat target berulang untuk menghindari loop. Pass ini tidak menghapus blok perantara dari fungsi.

## Penggunaan

```rust
use brak_opt_jt::JumpThreading;
use brak_opt_traits::PassManager;

let mut passes = PassManager::default();
passes.add_pass(Box::new(JumpThreading));
let optimized = passes.run(program)?;
```

Nama pass yang dilaporkan adalah `jt`.
