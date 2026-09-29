# brak-link-traits

Kontrak bersama untuk backend linker Brak. Crate ini mendefinisikan format object input, hasil linking, dan trait yang diimplementasikan oleh linker konkret.

## API utama

- `ObjectFile`: nama dan byte sebuah object file.
- `LinkerOutput`: byte hasil linking beserta label formatnya.
- `LinkerBackend`: trait `Send + Sync` dengan nama backend dan operasi `link`.

```rust
pub trait LinkerBackend: Send + Sync {
    fn name(&self) -> &'static str;
    fn link(
        &self,
        objects: &[ObjectFile],
        entry: &str,
        base_addr: u64,
    ) -> brak_core::Result<LinkerOutput>;
}
```

Implementasi konkret berada di crate linker seperti `brak-link-native`. Crate ini hanya menyediakan abstraksi dan tipe bersama.
