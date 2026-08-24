use brak_core::{ContentHash, combine_hash, SourceMap, Span};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum TokenKind {
    // Literals
    Ident,
    Number,
    String,
    // Delimiters
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Semicolon,
    Colon,
    Comma,
    Dot,
    // Operators
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Equals,
    EqualsEquals,
    Bang,
    BangEquals,
    Less,
    LessEquals,
    Greater,
    GreaterEquals,
    Arrow,
    FatArrow,
    Ampersand,
    AndAnd,
    Pipe,
    PipePipe,
    Caret,
    Tilde,
    Shl,
    Shr,
    DoubleDot,
    ColonColon,
    PlusEquals,
    MinusEquals,
    StarEquals,
    SlashEquals,
    PercentEquals,
    Hash,
    Question,
    Dollar,
    At,
    // Keywords
    Let,
    Fn,
    If,
    Else,
    For,
    While,
    Loop,
    Match,
    Break,
    Continue,
    Return,
    True,
    False,
    Extern,
    Pub,
    Use,
    Mod,
    In,
    As,
    Struct,
    Enum,
    Trait,
    Impl,
    Type,
    Where,
    Const,
    Static,
    SelfLower,
    SelfUpper,
    Async,
    Await,
    // Special
    Comment,
    Whitespace,
    Error,
    Eof,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Token {
    pub kind: TokenKind,
    pub lexeme: String,
    pub span: Span,
}

impl Token {
    pub fn new(kind: TokenKind, lexeme: impl Into<String>, span: Span) -> Self {
        Self { kind, lexeme: lexeme.into(), span }
    }
}

pub trait BrakLexer: Send + Sync {
    fn lex(&mut self, source: &SourceMap) -> Vec<Token>;
    fn reset(&mut self, source: &SourceMap);
}

pub struct AsciiLexer {
    pos: usize,
    source: SourceMap,
    chars: Vec<char>,
}

impl Default for AsciiLexer {
    fn default() -> Self {
        Self::new()
    }
}

impl AsciiLexer {
    pub fn new() -> Self {
        Self {
            pos: 0,
            source: SourceMap::new("", ""),
            chars: vec![],
        }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn advance(&mut self) -> Option<char> {
        let c = self.chars.get(self.pos).copied();
        if c.is_some() {
            self.pos += 1;
        }
        c
    }

    fn skip_whitespace(&mut self) {
        while let Some(c) = self.peek() {
            if c.is_ascii_whitespace() {
                self.advance();
            } else {
                break;
            }
        }
    }

    fn lex_number(&mut self) -> (String, Span) {
        let start = self.pos;
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                self.advance();
            } else {
                break;
            }
        }
        // Check for float: dot followed by digit (not another dot = range)
        if self.peek() == Some('.') && self.chars.get(self.pos + 1).map_or(false, |c| c.is_ascii_digit()) {
            self.advance();
            while let Some(c) = self.peek() {
                if c.is_ascii_digit() {
                    self.advance();
                } else {
                    break;
                }
            }
        }
        let end = self.pos;
        let lexeme = self.chars[start..end].iter().collect::<String>();
        let span = self.source.span_at(start, end).unwrap();
        (lexeme, span)
    }

    fn lex_ident(&mut self) -> (String, Span) {
        let start = self.pos;
        while let Some(c) = self.peek() {
            if c.is_ascii_alphanumeric() || c == '_' {
                self.advance();
            } else {
                break;
            }
        }
        let end = self.pos;
        let lexeme = self.chars[start..end].iter().collect::<String>();
        let span = self.source.span_at(start, end).unwrap();
        (lexeme, span)
    }

    fn lex_string(&mut self) -> (String, Span) {
        let start = self.pos - 1; // including the opening quote
        let mut lexeme = String::new();
        lexeme.push('"');
        while let Some(c) = self.peek() {
            self.advance();
            // BUG-M04: escape sequences were copied verbatim — `"a\"b"`
            // terminated at the escaped quote and left garbage tokens behind.
            if c == '\\' {
                match self.peek() {
                    Some(e @ ('"' | '\\')) => { self.advance(); lexeme.push('\\'); lexeme.push(e); }
                    other => {
                        // Unterminated or stray escape: keep as-is; the parser's
                        // string handling reports the malformed token.
                        let _ = other;
                        lexeme.push(c);
                    }
                }
                continue;
            }
            lexeme.push(c);
            if c == '"' {
                break;
            }
        }
        let end = self.pos;
        let span = self.source.span_at(start, end).unwrap();
        (lexeme, span)
    }
}

impl BrakLexer for AsciiLexer {
    fn lex(&mut self, source: &SourceMap) -> Vec<Token> {
        self.reset(source);
        let mut tokens = vec![];

        loop {
            self.skip_whitespace();
            match self.peek() {
                None => {
                    let end = self.pos;
                    let span = self.source.span_at(end, end).unwrap();
                    tokens.push(Token::new(TokenKind::Eof, "", span));
                    break;
                }
                Some('/') => {
                    self.advance();
                    if self.peek() == Some('/') {
                        while let Some(c) = self.peek() {
                            if c == '\n' || c == '\r' { break; }
                            self.advance();
                        }
                    } else if self.peek() == Some('=') {
                        self.advance();
                        let span = self.source.span_at(self.pos - 2, self.pos).unwrap();
                        tokens.push(Token::new(TokenKind::SlashEquals, "/=", span));
                    } else {
                        let span = self.source.span_at(self.pos - 1, self.pos).unwrap();
                        tokens.push(Token::new(TokenKind::Slash, "/", span));
                    }
                }
                Some(c) if c.is_ascii_digit() => {
                    let (lexeme, span) = self.lex_number();
                    tokens.push(Token::new(TokenKind::Number, lexeme, span));
                }
                Some(c) if c.is_ascii_alphabetic() || c == '_' => {
                    let (lexeme, span) = self.lex_ident();
                    let kind = match lexeme.as_str() {
                        "let" => TokenKind::Let,
                        "fn" => TokenKind::Fn,
                        "if" => TokenKind::If,
                        "else" => TokenKind::Else,
                        "for" => TokenKind::For,
                        "while" => TokenKind::While,
                        "loop" => TokenKind::Loop,
                        "match" => TokenKind::Match,
                        "break" => TokenKind::Break,
                        "continue" => TokenKind::Continue,
                        "return" => TokenKind::Return,
                        "true" => TokenKind::True,
                        "false" => TokenKind::False,
                        "extern" => TokenKind::Extern,
                        "pub" => TokenKind::Pub,
                        "use" => TokenKind::Use,
                        "mod" => TokenKind::Mod,
                        "in" => TokenKind::In,
                        "as" => TokenKind::As,
                        "struct" => TokenKind::Struct,
                        "enum" => TokenKind::Enum,
                        "trait" => TokenKind::Trait,
                        "impl" => TokenKind::Impl,
                        "type" => TokenKind::Type,
                        "where" => TokenKind::Where,
                        "const" => TokenKind::Const,
                        "static" => TokenKind::Static,
                        "self" => TokenKind::SelfLower,
                        "Self" => TokenKind::SelfUpper,
                        "async" => TokenKind::Async,
                        "await" => TokenKind::Await,
                        _ => TokenKind::Ident,
                    };
                    tokens.push(Token::new(kind, lexeme, span));
                }
                Some(c) => {
                    self.advance();
                    let (kind, lexeme) = match c {
                        '(' => (TokenKind::LParen, "("),
                        ')' => (TokenKind::RParen, ")"),
                        '{' => (TokenKind::LBrace, "{"),
                        '}' => (TokenKind::RBrace, "}"),
                        '[' => (TokenKind::LBracket, "["),
                        ']' => (TokenKind::RBracket, "]"),
                        ';' => (TokenKind::Semicolon, ";"),
                        ':' => {
                            if self.peek() == Some(':') {
                                self.advance();
                                (TokenKind::ColonColon, "::")
                            } else {
                                (TokenKind::Colon, ":")
                            }
                        }
                        ',' => (TokenKind::Comma, ","),
                        '.' => {
                            if self.peek() == Some('.') {
                                self.advance();
                                (TokenKind::DoubleDot, "..")
                            } else {
                                (TokenKind::Dot, ".")
                            }
                        }
                        '+' => {
                            if self.peek() == Some('=') {
                                self.advance();
                                (TokenKind::PlusEquals, "+=")
                            } else {
                                (TokenKind::Plus, "+")
                            }
                        }
                        '%' => {
                            if self.peek() == Some('=') {
                                self.advance();
                                (TokenKind::PercentEquals, "%=")
                            } else {
                                (TokenKind::Percent, "%")
                            }
                        }
                        '-' => {
                            if self.peek() == Some('>') {
                                self.advance();
                                (TokenKind::Arrow, "->")
                            } else if self.peek() == Some('=') {
                                self.advance();
                                (TokenKind::MinusEquals, "-=")
                            } else {
                                (TokenKind::Minus, "-")
                            }
                        }
                        '*' => {
                            if self.peek() == Some('=') {
                                self.advance();
                                (TokenKind::StarEquals, "*=")
                            } else {
                                (TokenKind::Star, "*")
                            }
                        }
                        '/' => {
                            if self.peek() == Some('=') {
                                self.advance();
                                (TokenKind::SlashEquals, "/=")
                            } else {
                                (TokenKind::Slash, "/")
                            }
                        }
                        '&' => {
                            if self.peek() == Some('&') {
                                self.advance();
                                (TokenKind::AndAnd, "&&")
                            } else {
                                (TokenKind::Ampersand, "&")
                            }
                        }
                        '|' => {
                            if self.peek() == Some('|') {
                                self.advance();
                                (TokenKind::PipePipe, "||")
                            } else {
                                (TokenKind::Pipe, "|")
                            }
                        }
                        '^' => (TokenKind::Caret, "^"),
                        '~' => (TokenKind::Tilde, "~"),
                        '#' => (TokenKind::Hash, "#"),
                        '?' => (TokenKind::Question, "?"),
                        '$' => (TokenKind::Dollar, "$"),
                        '@' => (TokenKind::At, "@"),
                        '!' => {
                            if self.peek() == Some('=') {
                                self.advance();
                                (TokenKind::BangEquals, "!=")
                            } else {
                                (TokenKind::Bang, "!")
                            }
                        }
                        '=' => {
                            if self.peek() == Some('=') {
                                self.advance();
                                (TokenKind::EqualsEquals, "==")
                            } else if self.peek() == Some('>') {
                                self.advance();
                                (TokenKind::FatArrow, "=>")
                            } else {
                                (TokenKind::Equals, "=")
                            }
                        }
                        '<' => {
                            if self.peek() == Some('<') {
                                self.advance();
                                (TokenKind::Shl, "<<")
                            } else if self.peek() == Some('=') {
                                self.advance();
                                (TokenKind::LessEquals, "<=")
                            } else {
                                (TokenKind::Less, "<")
                            }
                        }
                        '>' => {
                            if self.peek() == Some('>') {
                                self.advance();
                                (TokenKind::Shr, ">>")
                            } else if self.peek() == Some('=') {
                                self.advance();
                                (TokenKind::GreaterEquals, ">=")
                            } else {
                                (TokenKind::Greater, ">")
                            }
                        }
                        '"' => {
                            let (lexeme, span) = self.lex_string();
                            tokens.push(Token::new(TokenKind::String, lexeme, span));
                            continue;
                        }
                        _ => (TokenKind::Error, "?"),
                    };
                    let span = self.source.span_at(self.pos - 1, self.pos).unwrap();
                    tokens.push(Token::new(kind, lexeme, span));
                }
            }
        }

        tokens
    }

    fn reset(&mut self, source: &SourceMap) {
        self.source = source.clone();
        self.chars = source.source.chars().collect();
        self.pos = 0;
    }
}

impl ContentHash for Token {
    fn content_hash(&self) -> u64 {
        let mut h = self.kind.content_hash();
        h = combine_hash(h, self.lexeme.content_hash());
        h
    }
}

impl ContentHash for TokenKind {
    fn content_hash(&self) -> u64 {
        *self as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use brak_core::SourceMap;

    fn sm(src: &str) -> SourceMap {
        SourceMap::new("test.brk", src)
    }

    #[test]
    fn test_lex_keywords() {
        let sm = sm("let fn if else for while loop match break continue return true false extern pub use mod in as struct enum trait impl type where const static self Self async await");
        let mut lexer = AsciiLexer::new();
        let tokens = lexer.lex(&sm);
        let kinds: Vec<_> = tokens.iter().map(|t| t.kind).collect();
        assert!(kinds.contains(&TokenKind::Let));
        assert!(kinds.contains(&TokenKind::Fn));
        assert!(kinds.contains(&TokenKind::If));
        assert!(kinds.contains(&TokenKind::Else));
        assert!(kinds.contains(&TokenKind::For));
        assert!(kinds.contains(&TokenKind::While));
        assert!(kinds.contains(&TokenKind::Loop));
        assert!(kinds.contains(&TokenKind::Match));
        assert!(kinds.contains(&TokenKind::Break));
        assert!(kinds.contains(&TokenKind::Continue));
        assert!(kinds.contains(&TokenKind::Return));
        assert!(kinds.contains(&TokenKind::True));
        assert!(kinds.contains(&TokenKind::False));
        assert!(kinds.contains(&TokenKind::Extern));
        assert!(kinds.contains(&TokenKind::Pub));
        assert!(kinds.contains(&TokenKind::Use));
        assert!(kinds.contains(&TokenKind::Mod));
        assert!(kinds.contains(&TokenKind::In));
        assert!(kinds.contains(&TokenKind::As));
        assert!(kinds.contains(&TokenKind::Struct));
        assert!(kinds.contains(&TokenKind::Enum));
        assert!(kinds.contains(&TokenKind::Trait));
        assert!(kinds.contains(&TokenKind::Impl));
        assert!(kinds.contains(&TokenKind::Type));
        assert!(kinds.contains(&TokenKind::Where));
        assert!(kinds.contains(&TokenKind::Const));
        assert!(kinds.contains(&TokenKind::Static));
        assert!(kinds.contains(&TokenKind::SelfLower));
        assert!(kinds.contains(&TokenKind::SelfUpper));
        assert!(kinds.contains(&TokenKind::Async));
        assert!(kinds.contains(&TokenKind::Await));
        assert_eq!(tokens.last().unwrap().kind, TokenKind::Eof);
    }

    #[test]
    fn test_lex_numbers() {
        let sm = sm("42 100");
        let mut lexer = AsciiLexer::new();
        let tokens = lexer.lex(&sm);
        let nums: Vec<&str> = tokens.iter()
            .filter(|t| t.kind == TokenKind::Number)
            .map(|t| t.lexeme.as_str())
            .collect();
        assert_eq!(nums, vec!["42", "100"]);
    }

    #[test]
    fn test_lex_operators() {
        let sm = sm("+ - * / % = == ! != < <= > >= -> & && | || ^ ~ .. :: += -= *= /= %= # ? $ @");
        let mut lexer = AsciiLexer::new();
        let tokens = lexer.lex(&sm);
        let kinds: Vec<TokenKind> = tokens.iter().map(|t| t.kind).collect();
        assert!(kinds.contains(&TokenKind::Plus));
        assert!(kinds.contains(&TokenKind::Minus));
        assert!(kinds.contains(&TokenKind::Star));
        assert!(kinds.contains(&TokenKind::Slash));
        assert!(kinds.contains(&TokenKind::Percent));
        assert!(kinds.contains(&TokenKind::EqualsEquals));
        assert!(kinds.contains(&TokenKind::BangEquals));
        assert!(kinds.contains(&TokenKind::Less));
        assert!(kinds.contains(&TokenKind::LessEquals));
        assert!(kinds.contains(&TokenKind::Greater));
        assert!(kinds.contains(&TokenKind::GreaterEquals));
        assert!(kinds.contains(&TokenKind::Equals));
        assert!(kinds.contains(&TokenKind::Bang));
        assert!(kinds.contains(&TokenKind::Arrow));
        assert!(kinds.contains(&TokenKind::Ampersand));
        assert!(kinds.contains(&TokenKind::AndAnd));
        assert!(kinds.contains(&TokenKind::Pipe));
        assert!(kinds.contains(&TokenKind::PipePipe));
        assert!(kinds.contains(&TokenKind::Caret));
        assert!(kinds.contains(&TokenKind::Tilde));
        assert!(kinds.contains(&TokenKind::DoubleDot));
        assert!(kinds.contains(&TokenKind::ColonColon));
        assert!(kinds.contains(&TokenKind::PlusEquals));
        assert!(kinds.contains(&TokenKind::MinusEquals));
        assert!(kinds.contains(&TokenKind::StarEquals));
        assert!(kinds.contains(&TokenKind::SlashEquals));
        assert!(kinds.contains(&TokenKind::PercentEquals));
        assert!(kinds.contains(&TokenKind::Hash));
        assert!(kinds.contains(&TokenKind::Question));
        assert!(kinds.contains(&TokenKind::Dollar));
        assert!(kinds.contains(&TokenKind::At));
    }

    #[test]
    fn test_lex_delimiters() {
        let sm = sm("( ) { } [ ] ; : , .");
        let mut lexer = AsciiLexer::new();
        let tokens = lexer.lex(&sm);
        let kinds: Vec<TokenKind> = tokens.iter().map(|t| t.kind).collect();
        assert!(kinds.contains(&TokenKind::LParen));
        assert!(kinds.contains(&TokenKind::RParen));
        assert!(kinds.contains(&TokenKind::LBrace));
        assert!(kinds.contains(&TokenKind::RBrace));
        assert!(kinds.contains(&TokenKind::LBracket));
        assert!(kinds.contains(&TokenKind::RBracket));
        assert!(kinds.contains(&TokenKind::Semicolon));
        assert!(kinds.contains(&TokenKind::Colon));
        assert!(kinds.contains(&TokenKind::Comma));
        assert!(kinds.contains(&TokenKind::Dot));
    }

    #[test]
    fn test_lex_comment() {
        let sm = sm("let x = 42; // this is a comment");
        let mut lexer = AsciiLexer::new();
        let tokens = lexer.lex(&sm);
        let kinds: Vec<TokenKind> = tokens.iter().map(|t| t.kind).collect();
        assert!(kinds.contains(&TokenKind::Let));
        assert!(kinds.contains(&TokenKind::Ident));
        assert!(kinds.contains(&TokenKind::Equals));
        assert!(kinds.contains(&TokenKind::Number));
        assert!(kinds.contains(&TokenKind::Semicolon));
        assert!(!kinds.contains(&TokenKind::Comment));
    }

    #[test]
    fn test_lex_empty() {
        let sm = sm("");
        let mut lexer = AsciiLexer::new();
        let tokens = lexer.lex(&sm);
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].kind, TokenKind::Eof);
    }

    #[test]
    fn test_lex_identifiers() {
        let sm = sm("foo bar _baz myVar123");
        let mut lexer = AsciiLexer::new();
        let tokens = lexer.lex(&sm);
        let idents: Vec<&str> = tokens.iter()
            .filter(|t| t.kind == TokenKind::Ident)
            .map(|t| t.lexeme.as_str())
            .collect();
        assert_eq!(idents, vec!["foo", "bar", "_baz", "myVar123"]);
    }

    #[test]
    fn test_lex_shift_ops() {
        let sm = sm("<< >>");
        let mut lexer = AsciiLexer::new();
        let tokens = lexer.lex(&sm);
        let kinds: Vec<TokenKind> = tokens.iter().map(|t| t.kind).collect();
        assert!(kinds.contains(&TokenKind::Shl));
        assert!(kinds.contains(&TokenKind::Shr));
    }

    #[test]
    fn test_lex_compound_assign() {
        let sm = sm("+= -= *= /= %=");
        let mut lexer = AsciiLexer::new();
        let tokens = lexer.lex(&sm);
        let kinds: Vec<TokenKind> = tokens.iter().map(|t| t.kind).collect();
        assert!(kinds.contains(&TokenKind::PlusEquals));
        assert!(kinds.contains(&TokenKind::MinusEquals));
        assert!(kinds.contains(&TokenKind::StarEquals));
        assert!(kinds.contains(&TokenKind::SlashEquals));
        assert!(kinds.contains(&TokenKind::PercentEquals));
    }

    #[test]
    fn test_lex_mixed_operators() {
        let sm = sm("a + b * (c - d) / e % f");
        let mut lexer = AsciiLexer::new();
        let tokens = lexer.lex(&sm);
        let kinds: Vec<TokenKind> = tokens.iter().map(|t| t.kind).collect();
        assert!(kinds.contains(&TokenKind::Plus));
        assert!(kinds.contains(&TokenKind::Star));
        assert!(kinds.contains(&TokenKind::Minus));
        assert!(kinds.contains(&TokenKind::Slash));
        assert!(kinds.contains(&TokenKind::Percent));
        assert!(kinds.contains(&TokenKind::LParen));
        assert!(kinds.contains(&TokenKind::RParen));
    }

    #[test]
    fn test_lex_string_literal() {
        let sm = sm("\"hello world\"");
        let mut lexer = AsciiLexer::new();
        let tokens = lexer.lex(&sm);
        assert_eq!(tokens[0].kind, TokenKind::String);
        assert_eq!(tokens[0].lexeme, "\"hello world\"");
    }

    #[test]
    fn test_lex_float_number() {
        let sm = sm("3.14 0.5 100.0");
        let mut lexer = AsciiLexer::new();
        let tokens = lexer.lex(&sm);
        let nums: Vec<&str> = tokens.iter()
            .filter(|t| t.kind == TokenKind::Number)
            .map(|t| t.lexeme.as_str())
            .collect();
        assert_eq!(nums, vec!["3.14", "0.5", "100.0"]);
    }

    #[test]
    fn test_lex_all_arrow_and_range() {
        let sm = sm("-> ..");
        let mut lexer = AsciiLexer::new();
        let tokens = lexer.lex(&sm);
        let kinds: Vec<TokenKind> = tokens.iter().map(|t| t.kind).collect();
        assert!(kinds.contains(&TokenKind::Arrow));
        assert!(kinds.contains(&TokenKind::DoubleDot));
    }

    #[test]
    fn test_lex_tilde_and_caret() {
        let sm = sm("~ ^");
        let mut lexer = AsciiLexer::new();
        let tokens = lexer.lex(&sm);
        let kinds: Vec<TokenKind> = tokens.iter().map(|t| t.kind).collect();
        assert!(kinds.contains(&TokenKind::Tilde));
        assert!(kinds.contains(&TokenKind::Caret));
    }

    #[test]
    fn test_lex_consecutive_operators_no_space() {
        let src = sm("a<<b");
        let mut lexer = AsciiLexer::new();
        let tokens = lexer.lex(&src);
        let kinds: Vec<TokenKind> = tokens.iter().map(|t| t.kind).collect();
        assert!(kinds.contains(&TokenKind::Shl));
        let src = sm("a>>b");
        let mut lexer = AsciiLexer::new();
        let tokens = lexer.lex(&src);
        let kinds: Vec<TokenKind> = tokens.iter().map(|t| t.kind).collect();
        assert!(kinds.contains(&TokenKind::Shr));
    }
}
