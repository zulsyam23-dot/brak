# brak-opt-traits

Kontrak dan pengelola pass optimasi LIR Brak.

## API

- Implementasikan `LirOptimizationPass::name` dan `run` untuk membuat pass.
- `PassManager::add_pass` menambahkan pass; `with_iterations` dan `with_verbose` mengatur eksekusi; `run` menjalankan pipeline sampai batas iterasi atau konvergen.
- `load_external_pass` dapat memuat library dinamis yang mengekspor `create_pass`.

Plugin memakai ABI Rust (`extern "Rust"`) dan harus dibangun dengan toolchain Rust yang kompatibel dengan host. Jangan menganggap format plugin sebagai ABI stabil lintas versi compiler.
