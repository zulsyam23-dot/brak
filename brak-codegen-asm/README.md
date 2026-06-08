# brak-codegen-asm

Generator kode mesin asli (Native Machine Code).

## Kegunaan
- Fokus pada arsitektur **x86_64**.
- Melakukan **Register Allocation**: Mengatur variabel mana yang masuk ke register CPU asli (rax, rbx, dll).
- Menghasilkan instruksi binary yang bisa langsung dijalankan oleh CPU.
