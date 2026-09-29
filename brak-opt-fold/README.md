# brak-opt-fold

Pass constant folding untuk LIR Brak. Pass ini menghitung operasi yang seluruh operand-nya konstan dan menyederhanakan beberapa identitas aritmetika.

## Operasi yang ditangani

- `Add`, `Sub`, dan `Mul` untuk operand integer konstan, termasuk identitas seperti `x + 0` dan `x * 1`.
- `And`, `Or`, dan `Xor` untuk operand integer konstan.

Hasil pass direpresentasikan sebagai instruksi `Mov` dengan operand konstan.

## Penggunaan

```rust
use brak_opt_fold::ConstantFolding;
use brak_opt_traits::PassManager;

let mut passes = PassManager::default();
passes.add_pass(Box::new(ConstantFolding));
let optimized = passes.run(program)?;
```

Nama pass yang dilaporkan adalah `fold`.
