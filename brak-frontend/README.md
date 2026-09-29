# brak-frontend

Lexer dan parser untuk mengubah sumber Brak menjadi token lalu AST.

## Alur dasar

`AsciiLexer` menerima `SourceMap` dan menghasilkan token. `Parser` dari modul `parser` menerima token dan mengembalikan `Program` dari `brak-ir-ast` atau error parse.

```rust
use brak_core::SourceMap;
use brak_frontend::lexer::{AsciiLexer, BrakLexer};
use brak_frontend::parser::Parser;

let source_map = SourceMap::new("main.brk", source);
let mut lexer = AsciiLexer::new();
let tokens = lexer.lex(&source_map);
let ast = Parser::new().parse(&tokens)?;
```

Lowering ke HIR dan type checking dilakukan oleh `brak-ir-hir`, bukan crate frontend.
