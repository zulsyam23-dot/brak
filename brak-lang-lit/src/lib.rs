use brak_core::{SourceLoc, Span};
use brak_ir_hir::hir::*;

#[derive(Debug)]
pub struct LitError {
    pub message: String,
    pub span: Span,
    pub path: String,
}

impl std::fmt::Display for LitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} at {:?} in {}", self.message, self.span, self.path)
    }
}

impl std::error::Error for LitError {}

pub fn compile_lit_to_hir(source: &str, path: &str) -> Result<HirProgram, LitError> {
    let mut parser = LitParser::new(source, path);
    parser.parse_program()
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum TokenKind {
    Fn,
    Ident,
    Arrow,
    Equals,
    Semicolon,
    LParen,
    RParen,
    Colon,
    Comma,
    IntLit,
    StringLit,
    Eof,
}

#[derive(Debug, Clone)]
struct Token {
    kind: TokenKind,
    text: String,
    offset: usize,
}

struct LitParser<'a> {
    source: &'a str,
    path: String,
    pos: usize,
    tokens: Vec<Token>,
    tok_pos: usize,
}

impl<'a> LitParser<'a> {
    fn new(source: &'a str, path: &str) -> Self {
        let mut p = LitParser {
            source,
            path: path.to_string(),
            pos: 0,
            tokens: Vec::new(),
            tok_pos: 0,
        };
        p.tokenize();
        p
    }

    fn peek(&self) -> &Token {
        if self.tok_pos < self.tokens.len() {
            &self.tokens[self.tok_pos]
        } else {
            &self.tokens.last().unwrap()
        }
    }

    fn advance(&mut self) {
        if self.tok_pos < self.tokens.len() {
            self.tok_pos += 1;
        }
    }

    fn expect(&mut self, kind: TokenKind) -> Result<&Token, LitError> {
        if self.peek().kind == kind {
            let _tok = self.peek().clone();
            self.advance();
            Ok(&self.tokens[self.tok_pos - 1])
        } else {
            Err(LitError {
                message: format!("expected {:?}, got {:?} ('{}')", kind, self.peek().kind, self.peek().text),
                span: Span::new(SourceLoc::new(0, 0, self.peek().offset), SourceLoc::new(0, 0, self.pos)),
                path: self.path.clone(),
            })
        }
    }

    fn tokenize(&mut self) {
        let chars: Vec<char> = self.source.chars().collect();
        let len = chars.len();
        let mut i = 0;

        while i < len {
            // Whitespace
            if chars[i].is_whitespace() {
                i += 1;
                continue;
            }

            // Comment
            if i + 1 < len && chars[i] == '/' && chars[i + 1] == '/' {
                while i < len && chars[i] != '\n' {
                    i += 1;
                }
                continue;
            }

            let start = i;

            // Ident or keyword
            if chars[i].is_alphabetic() || chars[i] == '_' {
                while i < len && (chars[i].is_alphanumeric() || chars[i] == '_') {
                    i += 1;
                }
                let word: String = chars[start..i].iter().collect();
                let kind = match word.as_str() {
                    "fn" => TokenKind::Fn,
                    _ => TokenKind::Ident,
                };
                self.tokens.push(Token { kind, text: word, offset: start });
                continue;
            }

            // Integer
            if chars[i].is_ascii_digit() {
                while i < len && chars[i].is_ascii_digit() {
                    i += 1;
                }
                let num: String = chars[start..i].iter().collect();
                self.tokens.push(Token { kind: TokenKind::IntLit, text: num, offset: start });
                continue;
            }

            // String literal
            if chars[i] == '"' {
                i += 1;
                while i < len && chars[i] != '"' {
                    i += 1;
                }
                if i < len {
                    i += 1; // closing "
                }
                let s: String = chars[start + 1..i - 1].iter().collect();
                self.tokens.push(Token { kind: TokenKind::StringLit, text: s, offset: start });
                continue;
            }

            // Symbols
            i += 1;
            let ch = chars[start];
            let kind = match ch {
                '-' => {
                    if i < len && chars[i] == '>' {
                        i += 1;
                        TokenKind::Arrow
                    } else {
                        panic!("unexpected '-'");
                    }
                }
                '=' => TokenKind::Equals,
                ';' => TokenKind::Semicolon,
                '(' => TokenKind::LParen,
                ')' => TokenKind::RParen,
                ':' => TokenKind::Colon,
                ',' => TokenKind::Comma,
                _ => panic!("unexpected character '{}'", ch),
            };
            self.tokens.push(Token { kind, text: ch.to_string(), offset: start });
        }

        self.tokens.push(Token { kind: TokenKind::Eof, text: String::new(), offset: len });
    }

    fn parse_program(&mut self) -> Result<HirProgram, LitError> {
        let mut items = Vec::new();

        while self.peek().kind != TokenKind::Eof {
            let item = self.parse_function()?;
            items.push(item);
        }

        Ok(HirProgram { items })
    }

    fn parse_function(&mut self) -> Result<HirItem, LitError> {
        self.expect(TokenKind::Fn)?;
        let name_text = self.expect(TokenKind::Ident)?.text.clone();
        self.expect(TokenKind::LParen)?;

        let mut params = Vec::new();
        if self.peek().kind != TokenKind::RParen {
            loop {
                let pname = self.expect(TokenKind::Ident)?;
                let pname_text = pname.text.clone();
                let pname_offset = pname.offset;
                self.expect(TokenKind::Colon)?;
                let pty = self.parse_type()?;
                params.push(HirParam {
                    name: pname_text,
                    ty: pty,
                    span: Span::new(SourceLoc::new(0, 0, pname_offset), SourceLoc::new(0, 0, self.pos)),
                });
                if self.peek().kind == TokenKind::Comma {
                    self.advance();
                } else {
                    break;
                }
            }
        }
        self.expect(TokenKind::RParen)?;
        self.expect(TokenKind::Arrow)?;
        let ret_ty = self.parse_type()?;
        self.expect(TokenKind::Equals)?;

        let expr = self.parse_expr()?;
        self.expect(TokenKind::Semicolon)?;

        Ok(HirItem::Function(HirFunction {
            name: name_text,
            params,
            ret_ty,
            body: HirBlock {
                stmts: vec![HirStmt::Return(Some(Box::new(expr)), Span::new(SourceLoc::new(0, 0, 0), SourceLoc::new(0, 0, self.pos)))],
                span: Span::new(SourceLoc::new(0, 0, 0), SourceLoc::new(0, 0, self.pos)),
            },
            span: Span::new(SourceLoc::new(0, 0, 0), SourceLoc::new(0, 0, self.pos)),
        }))
    }

    fn parse_type(&mut self) -> Result<HirType, LitError> {
        let tok = self.expect(TokenKind::Ident)?;
        match tok.text.as_str() {
            "I32" => Ok(HirType::I32),
            "I64" => Ok(HirType::I64),
            "F32" => Ok(HirType::F32),
            "F64" => Ok(HirType::F64),
            "Bool" => Ok(HirType::Bool),
            "String" => Ok(HirType::String),
            "Void" => Ok(HirType::Void),
            other => Ok(HirType::Named(other.to_string())),
        }
    }

    fn parse_expr(&mut self) -> Result<HirExpr, LitError> {
        let tok = self.peek().clone();
        match tok.kind {
            TokenKind::IntLit => {
                self.advance();
                let val: i64 = tok.text.parse().unwrap_or(0);
                Ok(HirExpr::Int(val, Span::new(SourceLoc::new(0, 0, tok.offset), SourceLoc::new(0, 0, self.pos))))
            }
            TokenKind::StringLit => {
                self.advance();
                Ok(HirExpr::String(tok.text, Span::new(SourceLoc::new(0, 0, tok.offset), SourceLoc::new(0, 0, self.pos))))
            }
            _ => Err(LitError {
                message: format!("unexpected token {:?} in expression", tok.kind),
                span: Span::new(SourceLoc::new(0, 0, tok.offset), SourceLoc::new(0, 0, self.pos)),
                path: self.path.clone(),
            }),
        }
    }
}

impl std::fmt::Display for TokenKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}
