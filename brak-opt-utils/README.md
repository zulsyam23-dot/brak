# brak-opt-utils

Utilitas analisis control-flow graph untuk pass optimasi LIR.

## API

- `build_cfg` membangun successor dan predecessor antar basic block.
- `compute_dominance` menghitung immediate dominator; `dominates` menguji relasi dominasi.
- `find_natural_loops` mencari natural loop dari back-edge yang targetnya mendominasi sumber.

Tipe hasil utama adalah `CfgGraph`, `Dominance`, dan `NaturalLoop`. LICM menggunakan utilitas ini untuk memilih loop dan pre-header. Analisis bekerja pada basic block dan label yang direpresentasikan di LIR.
