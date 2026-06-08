use brak_core::{Diagnostic, Diagnostics, Span, DUMMY_SPAN};
use brak_ir_ast::ast::*;

use crate::lexer::{Token, TokenKind};

pub struct Parser {
    diagnostics: Diagnostics,
}

impl Default for Parser {
    fn default() -> Self {
        Self::new()
    }
}

impl Parser {
    pub fn new() -> Self {
        Self {
            diagnostics: Diagnostics::new(),
        }
    }

    pub fn parse(mut self, tokens: &[Token]) -> Result<Program, Diagnostics> {
        let mut pos = 0;
        let program = self.parse_program(tokens, &mut pos);
        if self.diagnostics.has_errors() {
            Err(self.diagnostics)
        } else {
            Ok(program)
        }
    }

    fn span(&self, tokens: &[Token], pos: usize) -> Span {
        tokens.get(pos).map(|t| t.span).unwrap_or(DUMMY_SPAN)
    }

    fn error(&mut self, msg: impl Into<String>, span: Option<Span>) {
        let diag = Diagnostic::error(msg);
        self.diagnostics.push(match span {
            Some(s) => diag.with_span(s),
            None => diag,
        });
    }

    fn parse_program(&mut self, tokens: &[Token], pos: &mut usize) -> Program {
        let mut items = vec![];
        while *pos < tokens.len() && tokens[*pos].kind != TokenKind::Eof {
            match self.parse_item(tokens, pos) {
                Ok(item) => items.push(item),
                Err(_) => {
                    // sync to next item boundary
                    while *pos < tokens.len() {
                        let k = tokens[*pos].kind;
                        if matches!(k, TokenKind::Fn | TokenKind::Let | TokenKind::Extern
                            | TokenKind::Struct | TokenKind::Enum | TokenKind::Use
                            | TokenKind::Mod | TokenKind::Const | TokenKind::Static
                            | TokenKind::Trait | TokenKind::Impl | TokenKind::Pub
                            | TokenKind::Eof) { break; }
                        *pos += 1;
                    }
                }
            }
        }
        Program { items }
    }

    fn parse_item(&mut self, tokens: &[Token], pos: &mut usize) -> Result<Item, ()> {
        let vis = if tokens.get(*pos).map(|t| t.kind) == Some(TokenKind::Pub) {
            *pos += 1;
            Visibility::Public
        } else {
            Visibility::Private
        };

        match tokens.get(*pos).map(|t| t.kind) {
            Some(TokenKind::Fn) => {
                let fd = self.parse_fn_def(tokens, pos)?;
                // Visibility handled by FnDef if we add it there, for now just wrap
                Ok(Item::FnDef(fd))
            }
            Some(TokenKind::Extern) => {
                let e = self.parse_extern_fn(tokens, pos)?;
                Ok(Item::ExternFn(e))
            }
            Some(TokenKind::Let) => {
                let l = self.parse_let(tokens, pos)?;
                self.expect_noerr(TokenKind::Semicolon, tokens, pos);
                Ok(Item::Let(l))
            }
            Some(TokenKind::Struct) => {
                let s = self.parse_struct_def(tokens, pos, vis)?;
                Ok(Item::Struct(s))
            }
            Some(TokenKind::Enum) => {
                let e = self.parse_enum_def(tokens, pos, vis)?;
                Ok(Item::Enum(e))
            }
            Some(TokenKind::Use) => {
                let u = self.parse_use_stmt(tokens, pos)?;
                Ok(Item::Use(u))
            }
            Some(TokenKind::Mod) => {
                let m = self.parse_mod_def(tokens, pos)?;
                Ok(Item::Mod(m))
            }
            Some(TokenKind::Const) => {
                let c = self.parse_const_def(tokens, pos, vis)?;
                Ok(Item::Const(c))
            }
            Some(TokenKind::Static) => {
                let s = self.parse_static_def(tokens, pos, vis)?;
                Ok(Item::Static(s))
            }
            Some(TokenKind::Trait) => {
                let t = self.parse_trait_def(tokens, pos, vis)?;
                Ok(Item::Trait(t))
            }
            Some(TokenKind::Impl) => {
                let i = self.parse_impl_def(tokens, pos)?;
                Ok(Item::Impl(i))
            }
            Some(other) => {
                self.error(format!("unexpected token {:?} at item level", other), self.span(tokens, *pos).into());
                Err(())
            }
            None => {
                self.error("unexpected end of file".to_string(), None);
                Err(())
            }
        }
    }

    fn parse_struct_def(&mut self, tokens: &[Token], pos: &mut usize, vis: Visibility) -> Result<StructDef, ()> {
        let start = *pos;
        self.expect_noerr(TokenKind::Struct, tokens, pos);
        let name = self.expect_ident(tokens, pos);
        self.expect_noerr(TokenKind::LBrace, tokens, pos);
        
        let mut fields = vec![];
        while tokens.get(*pos).map(|t| t.kind) != Some(TokenKind::RBrace) {
            let f_vis = if tokens.get(*pos).map(|t| t.kind) == Some(TokenKind::Pub) {
                *pos += 1;
                Visibility::Public
            } else {
                Visibility::Private
            };
            let f_name = self.expect_ident(tokens, pos);
            self.expect_noerr(TokenKind::Colon, tokens, pos);
            let ty = self.parse_type(tokens, pos)?;
            if tokens.get(*pos).map(|t| t.kind) == Some(TokenKind::Comma) {
                *pos += 1;
            }
            fields.push(Field {
                vis: f_vis,
                name: f_name.clone(),
                ty,
                span: f_name.span,
            });
        }
        self.expect_noerr(TokenKind::RBrace, tokens, pos);
        
        Ok(StructDef {
            vis,
            name,
            fields,
            span: Span::new(self.span(tokens, start).start, self.span(tokens, *pos - 1).end),
        })
    }

    fn parse_enum_def(&mut self, tokens: &[Token], pos: &mut usize, vis: Visibility) -> Result<EnumDef, ()> {
        let start = *pos;
        self.expect_noerr(TokenKind::Enum, tokens, pos);
        let name = self.expect_ident(tokens, pos);
        self.expect_noerr(TokenKind::LBrace, tokens, pos);
        
        let mut variants = vec![];
        while tokens.get(*pos).map(|t| t.kind) != Some(TokenKind::RBrace) {
            let v_name = self.expect_ident(tokens, pos);
            let mut fields = None;
            if tokens.get(*pos).map(|t| t.kind) == Some(TokenKind::LParen) {
                *pos += 1;
                let mut f = vec![];
                while tokens.get(*pos).map(|t| t.kind) != Some(TokenKind::RParen) {
                    f.push(self.parse_type(tokens, pos)?);
                    if tokens.get(*pos).map(|t| t.kind) == Some(TokenKind::Comma) {
                        *pos += 1;
                    }
                }
                self.expect_noerr(TokenKind::RParen, tokens, pos);
                fields = Some(f);
            }
            if tokens.get(*pos).map(|t| t.kind) == Some(TokenKind::Comma) {
                *pos += 1;
            }
            variants.push(Variant {
                name: v_name.clone(),
                fields,
                span: v_name.span,
            });
        }
        self.expect_noerr(TokenKind::RBrace, tokens, pos);
        
        Ok(EnumDef {
            vis,
            name,
            variants,
            span: Span::new(self.span(tokens, start).start, self.span(tokens, *pos - 1).end),
        })
    }

    fn parse_use_stmt(&mut self, tokens: &[Token], pos: &mut usize) -> Result<UseStmt, ()> {
        let start = *pos;
        self.expect_noerr(TokenKind::Use, tokens, pos);
        let mut path = vec![];
        loop {
            path.push(self.expect_ident(tokens, pos));
            if tokens.get(*pos).map(|t| t.kind) == Some(TokenKind::ColonColon) {
                *pos += 1;
            } else {
                break;
            }
        }
        self.expect_noerr(TokenKind::Semicolon, tokens, pos);
        Ok(UseStmt {
            path,
            span: Span::new(self.span(tokens, start).start, self.span(tokens, *pos - 1).end),
        })
    }

    fn parse_mod_def(&mut self, tokens: &[Token], pos: &mut usize) -> Result<ModDef, ()> {
        let start = *pos;
        self.expect_noerr(TokenKind::Mod, tokens, pos);
        let name = self.expect_ident(tokens, pos);
        let items = if tokens.get(*pos).map(|t| t.kind) == Some(TokenKind::LBrace) {
            *pos += 1;
            let mut i = vec![];
            while tokens.get(*pos).map(|t| t.kind) != Some(TokenKind::RBrace) {
                i.push(self.parse_item(tokens, pos)?);
            }
            self.expect_noerr(TokenKind::RBrace, tokens, pos);
            Some(i)
        } else {
            self.expect_noerr(TokenKind::Semicolon, tokens, pos);
            None
        };
        Ok(ModDef {
            name,
            items,
            span: Span::new(self.span(tokens, start).start, self.span(tokens, *pos - 1).end),
        })
    }

    fn parse_const_def(&mut self, tokens: &[Token], pos: &mut usize, vis: Visibility) -> Result<ConstDef, ()> {
        let start = *pos;
        self.expect_noerr(TokenKind::Const, tokens, pos);
        let name = self.expect_ident(tokens, pos);
        self.expect_noerr(TokenKind::Colon, tokens, pos);
        let ty = self.parse_type(tokens, pos)?;
        self.expect_noerr(TokenKind::Equals, tokens, pos);
        let value = self.parse_expr(tokens, pos)?;
        self.expect_noerr(TokenKind::Semicolon, tokens, pos);
        Ok(ConstDef {
            vis,
            name,
            ty,
            value,
            span: Span::new(self.span(tokens, start).start, self.span(tokens, *pos - 1).end),
        })
    }

    fn parse_static_def(&mut self, tokens: &[Token], pos: &mut usize, vis: Visibility) -> Result<StaticDef, ()> {
        let start = *pos;
        self.expect_noerr(TokenKind::Static, tokens, pos);
        let name = self.expect_ident(tokens, pos);
        self.expect_noerr(TokenKind::Colon, tokens, pos);
        let ty = self.parse_type(tokens, pos)?;
        self.expect_noerr(TokenKind::Equals, tokens, pos);
        let value = self.parse_expr(tokens, pos)?;
        self.expect_noerr(TokenKind::Semicolon, tokens, pos);
        Ok(StaticDef {
            vis,
            name,
            ty,
            value,
            span: Span::new(self.span(tokens, start).start, self.span(tokens, *pos - 1).end),
        })
    }

    fn parse_trait_def(&mut self, tokens: &[Token], pos: &mut usize, vis: Visibility) -> Result<TraitDef, ()> {
        let start = *pos;
        self.expect_noerr(TokenKind::Trait, tokens, pos);
        let name = self.expect_ident(tokens, pos);
        self.expect_noerr(TokenKind::LBrace, tokens, pos);
        let mut methods = vec![];
        while tokens.get(*pos).map(|t| t.kind) != Some(TokenKind::RBrace) {
            methods.push(self.parse_fn_def(tokens, pos)?);
        }
        self.expect_noerr(TokenKind::RBrace, tokens, pos);
        Ok(TraitDef {
            vis,
            name,
            methods,
            span: Span::new(self.span(tokens, start).start, self.span(tokens, *pos - 1).end),
        })
    }

    fn parse_impl_def(&mut self, tokens: &[Token], pos: &mut usize) -> Result<ImplDef, ()> {
        let start = *pos;
        self.expect_noerr(TokenKind::Impl, tokens, pos);
        
        let name1 = self.expect_ident(tokens, pos);
        let mut trait_name = None;
        let target_ty;

        if tokens.get(*pos).map(|t| t.kind) == Some(TokenKind::For) {
            *pos += 1;
            trait_name = Some(name1);
            target_ty = self.parse_type(tokens, pos)?;
        } else {
            target_ty = Type::Named(name1.name);
        }

        self.expect_noerr(TokenKind::LBrace, tokens, pos);
        let mut methods = vec![];
        while tokens.get(*pos).map(|t| t.kind) != Some(TokenKind::RBrace) {
            methods.push(self.parse_fn_def(tokens, pos)?);
        }
        self.expect_noerr(TokenKind::RBrace, tokens, pos);

        Ok(ImplDef {
            trait_name,
            target_ty,
            methods,
            span: Span::new(self.span(tokens, start).start, self.span(tokens, *pos - 1).end),
        })
    }

    fn parse_extern_fn(&mut self, tokens: &[Token], pos: &mut usize) -> Result<ExternFn, ()> {
        let start_span = self.span(tokens, *pos);
        self.expect_noerr(TokenKind::Extern, tokens, pos);
        
        let abi = if tokens.get(*pos).map(|t| t.kind) == Some(TokenKind::String) {
            let s = tokens[*pos].lexeme.clone();
            *pos += 1;
            s.trim_matches('"').to_string()
        } else {
            "C".to_string()
        };

        self.expect_noerr(TokenKind::Fn, tokens, pos);
        let name = self.expect_ident(tokens, pos);
        self.expect_noerr(TokenKind::LParen, tokens, pos);

        let mut params = vec![];
        if tokens.get(*pos).map(|t| t.kind) != Some(TokenKind::RParen) {
            loop {
                let param_name = self.expect_ident(tokens, pos);
                let ty = if tokens.get(*pos).map(|t| t.kind) == Some(TokenKind::Colon) {
                    *pos += 1;
                    Some(self.parse_type(tokens, pos)?)
                } else {
                    None
                };
                params.push(Param {
                    name: param_name.clone(),
                    ty,
                    span: param_name.span,
                });
                if tokens.get(*pos).map(|t| t.kind) == Some(TokenKind::Comma) {
                    *pos += 1;
                } else {
                    break;
                }
            }
        }
        self.expect_noerr(TokenKind::RParen, tokens, pos);

        let ret_ty = if tokens.get(*pos).map(|t| t.kind) == Some(TokenKind::Arrow) {
            *pos += 1;
            Some(self.parse_type(tokens, pos)?)
        } else {
            None
        };
        self.expect_noerr(TokenKind::Semicolon, tokens, pos);

        let end_span = self.span(tokens, *pos - 1);
        Ok(ExternFn {
            name,
            params,
            ret_ty,
            abi,
            span: Span::new(start_span.start, end_span.end),
        })
    }

    fn parse_fn_def(&mut self, tokens: &[Token], pos: &mut usize) -> Result<FnDef, ()> {
        let fn_span = self.span(tokens, *pos);
        self.expect_noerr(TokenKind::Fn, tokens, pos);
        let name = self.expect_ident(tokens, pos);
        self.expect_noerr(TokenKind::LParen, tokens, pos);

        let mut params = vec![];
        if tokens.get(*pos).map(|t| t.kind) != Some(TokenKind::RParen) {
            loop {
                let param_name = self.expect_ident(tokens, pos);
                let ty = if tokens.get(*pos).map(|t| t.kind) == Some(TokenKind::Colon) {
                    *pos += 1;
                    Some(self.parse_type(tokens, pos)?)
                } else {
                    None
                };
                params.push(Param {
                    name: param_name.clone(),
                    ty,
                    span: param_name.span,
                });
                if tokens.get(*pos).map(|t| t.kind) == Some(TokenKind::Comma) {
                    *pos += 1;
                } else {
                    break;
                }
            }
        }
        self.expect_noerr(TokenKind::RParen, tokens, pos);

        let ret_ty = if tokens.get(*pos).map(|t| t.kind) == Some(TokenKind::Arrow) {
            *pos += 1;
            Some(self.parse_type(tokens, pos)?)
        } else {
            None
        };

        let body = self.parse_block(tokens, pos)?;

        Ok(FnDef {
            name,
            params,
            ret_ty,
            body,
            span: fn_span,
        })
    }

    fn parse_block(&mut self, tokens: &[Token], pos: &mut usize) -> Result<Block, ()> {
        let start = *pos;
        self.expect_noerr(TokenKind::LBrace, tokens, pos);
        let mut stmts = vec![];
        while tokens.get(*pos).map(|t| t.kind) != Some(TokenKind::RBrace) {
            let stmt = self.parse_stmt(tokens, pos)?;
            stmts.push(stmt);
        }
        self.expect_noerr(TokenKind::RBrace, tokens, pos);
        Ok(Block {
            stmts,
            span: Span::new(
                self.span(tokens, start).start,
                self.span(tokens, *pos - 1).end,
            ),
        })
    }

    fn parse_stmt(&mut self, tokens: &[Token], pos: &mut usize) -> Result<Stmt, ()> {
        let stmt_span = self.span(tokens, *pos);
        match tokens.get(*pos).map(|t| t.kind) {
            Some(TokenKind::Let) => {
                let l = self.parse_let(tokens, pos)?;
                self.expect_noerr(TokenKind::Semicolon, tokens, pos);
                Ok(Stmt::Let(l))
            }
            Some(TokenKind::Return) => {
                *pos += 1;
                if tokens.get(*pos).map(|t| t.kind) == Some(TokenKind::Semicolon) {
                    *pos += 1;
                    Ok(Stmt::Return(None, stmt_span))
                } else {
                    let expr = self.parse_expr(tokens, pos)?;
                    self.expect_noerr(TokenKind::Semicolon, tokens, pos);
                    Ok(Stmt::Return(Some(expr), stmt_span))
                }
            }
            Some(TokenKind::Break) => {
                *pos += 1;
                self.expect_noerr(TokenKind::Semicolon, tokens, pos);
                Ok(Stmt::Break(stmt_span))
            }
            Some(TokenKind::Continue) => {
                *pos += 1;
                self.expect_noerr(TokenKind::Semicolon, tokens, pos);
                Ok(Stmt::Continue(stmt_span))
            }
            Some(TokenKind::If) => {
                let if_stmt = self.parse_if_stmt(tokens, pos)?;
                Ok(if_stmt)
            }
            Some(TokenKind::While) => {
                let w = self.parse_while(tokens, pos)?;
                Ok(w)
            }
            Some(TokenKind::Loop) => {
                let l = self.parse_loop(tokens, pos)?;
                Ok(l)
            }
            Some(TokenKind::For) => {
                let f = self.parse_for(tokens, pos)?;
                Ok(f)
            }
            Some(TokenKind::LBrace) => {
                let block = self.parse_block(tokens, pos)?;
                Ok(Stmt::Expr(Expr::Block(block)))
            }
            _ => {
                let expr = self.parse_expr(tokens, pos)?;
                if tokens.get(*pos).map(|t| t.kind) == Some(TokenKind::Semicolon) {
                    *pos += 1;
                }
                Ok(Stmt::Expr(expr))
            }
        }
    }

    fn parse_loop(&mut self, tokens: &[Token], pos: &mut usize) -> Result<Stmt, ()> {
        let start = *pos;
        self.expect_noerr(TokenKind::Loop, tokens, pos);
        let body = self.parse_block(tokens, pos)?;
        Ok(Stmt::Loop {
            body,
            span: Span::new(self.span(tokens, start).start, self.span(tokens, *pos - 1).end),
        })
    }

    fn parse_for(&mut self, tokens: &[Token], pos: &mut usize) -> Result<Stmt, ()> {
        let start = *pos;
        self.expect_noerr(TokenKind::For, tokens, pos);
        let var = self.expect_ident(tokens, pos);
        self.expect_noerr(TokenKind::In, tokens, pos);
        let iterable = self.parse_expr(tokens, pos)?;
        let body = self.parse_block(tokens, pos)?;
        Ok(Stmt::For {
            var,
            iterable: Box::new(iterable),
            body,
            span: Span::new(self.span(tokens, start).start, self.span(tokens, *pos - 1).end),
        })
    }

    fn parse_if_stmt(&mut self, tokens: &[Token], pos: &mut usize) -> Result<Stmt, ()> {
        let start = *pos;
        self.expect_noerr(TokenKind::If, tokens, pos);
        let cond = self.parse_expr(tokens, pos)?;
        let then = self.parse_block(tokens, pos)?;
        let else_ = if tokens.get(*pos).map(|t| t.kind) == Some(TokenKind::Else) {
            *pos += 1;
            Some(self.parse_block(tokens, pos)?)
        } else {
            None
        };
        let span = Span::new(
            self.span(tokens, start).start,
            else_.as_ref().map(|b| b.span.end).unwrap_or(then.span.end),
        );
        Ok(Stmt::If {
            cond: Box::new(cond),
            then,
            else_,
            span,
        })
    }

    fn parse_while(&mut self, tokens: &[Token], pos: &mut usize) -> Result<Stmt, ()> {
        let start = *pos;
        self.expect_noerr(TokenKind::While, tokens, pos);
        let cond = self.parse_expr(tokens, pos)?;
        let body = self.parse_block(tokens, pos)?;
        let span = Span::new(
            self.span(tokens, start).start,
            body.span.end,
        );
        Ok(Stmt::While {
            cond: Box::new(cond),
            body,
            span,
        })
    }

    fn parse_let(&mut self, tokens: &[Token], pos: &mut usize) -> Result<Let, ()> {
        let start = *pos;
        self.expect_noerr(TokenKind::Let, tokens, pos);
        let mut name = self.expect_ident(tokens, pos);
        while tokens.get(*pos).map(|t| t.kind) == Some(TokenKind::Dot) {
            *pos += 1;
            let field = self.expect_ident(tokens, pos);
            name.name.push('.');
            name.name.push_str(&field.name);
            name.span = Span::new(name.span.start, field.span.end);
        }

        let ty = if tokens.get(*pos).map(|t| t.kind) == Some(TokenKind::Colon) {
            *pos += 1;
            Some(self.parse_type(tokens, pos)?)
        } else {
            None
        };

        let value = if tokens.get(*pos).map(|t| t.kind) == Some(TokenKind::Equals) {
            *pos += 1;
            Some(Box::new(self.parse_expr(tokens, pos)?))
        } else {
            None
        };

        let end = *pos;
        Ok(Let {
            name,
            ty,
            value,
            span: Span::new(
                self.span(tokens, start).start,
                self.span(tokens, end - 1).end,
            ),
        })
    }

    fn parse_expr(&mut self, tokens: &[Token], pos: &mut usize) -> Result<Expr, ()> {
        let lhs = self.parse_binary(tokens, pos, 0)?;
        if tokens.get(*pos).map(|t| t.kind) == Some(TokenKind::Equals) {
            match &lhs {
                Expr::Ident(_) | Expr::Field { .. } => {
                    *pos += 1;
                    let rhs = self.parse_expr(tokens, pos)?;
                    let span = Span::new(lhs.span().start, rhs.span().end);
                    Ok(Expr::Assign(Box::new(lhs), Box::new(rhs), span))
                }
                _ => { self.error("left-hand side of assignment must be an identifier or field".to_string(), None); Err(()) },
            }
        } else {
            Ok(lhs)
        }
    }

    fn parse_binary(&mut self, tokens: &[Token], pos: &mut usize, min_prec: u8) -> Result<Expr, ()> {
        let mut lhs = self.parse_unary(tokens, pos)?;

        loop {
            let tok = tokens.get(*pos);
            let prec = tok.and_then(|t| self.precedence(t.kind));

            match prec {
                Some(p) if p >= min_prec => {
                    let _op_span = tok.unwrap().span;
                    let op = match tok.unwrap().kind {
                        TokenKind::Plus => BinOp::Add,
                        TokenKind::Minus => BinOp::Sub,
                        TokenKind::Star => BinOp::Mul,
                        TokenKind::Slash => BinOp::Div,
                        TokenKind::Percent => BinOp::Mod,
                        TokenKind::EqualsEquals => BinOp::Eq,
                        TokenKind::BangEquals => BinOp::Ne,
                        TokenKind::Less => BinOp::Lt,
                        TokenKind::LessEquals => BinOp::Le,
                        TokenKind::Greater => BinOp::Gt,
                        TokenKind::GreaterEquals => BinOp::Ge,
                        TokenKind::AndAnd => BinOp::And,
                        TokenKind::PipePipe => BinOp::Or,
                        TokenKind::Ampersand => BinOp::BitAnd,
                        TokenKind::Pipe => BinOp::BitOr,
                        TokenKind::Caret => BinOp::BitXor,
                        TokenKind::Shl => BinOp::Shl,
                        TokenKind::Shr => BinOp::Shr,
                        TokenKind::DoubleDot => BinOp::Range,
                        _ => { self.error("unexpected operator".to_string(), None); return Err(()) },
                    };
                    *pos += 1;
                    let rhs = self.parse_binary(tokens, pos, p + 1)?;
                    let lhs_span = lhs.span();
                    let rhs_span = rhs.span();
                    lhs = Expr::BinOp {
                        op,
                        lhs: Box::new(lhs),
                        rhs: Box::new(rhs),
                        span: Span::new(lhs_span.start, rhs_span.end),
                    };
                }
                _ => break,
            }
        }

        Ok(lhs)
    }

    fn parse_postfix(&mut self, mut expr: Expr, tokens: &[Token], pos: &mut usize) -> Result<Expr, ()> {
        loop {
            match tokens.get(*pos).map(|t| t.kind) {
                Some(TokenKind::Dot) => {
                    *pos += 1;
                    let field = self.expect_ident(tokens, pos);
                    let span = Span::new(expr.span().start, field.span.end);
                    expr = Expr::Field {
                        object: Box::new(expr),
                        field,
                        span,
                    };
                }
                _ => break,
            }
        }
        Ok(expr)
    }

    fn parse_unary(&mut self, tokens: &[Token], pos: &mut usize) -> Result<Expr, ()> {
        match tokens.get(*pos).map(|t| t.kind) {
            Some(TokenKind::Minus) => {
                let op_span = self.span(tokens, *pos);
                *pos += 1;
                let expr = self.parse_unary(tokens, pos)?;
                let expr_span = expr.span();
                Ok(Expr::UnOp {
                    op: UnOp::Neg,
                    expr: Box::new(expr),
                    span: Span::new(op_span.start, expr_span.end),
                })
            }
            Some(TokenKind::Bang) => {
                let op_span = self.span(tokens, *pos);
                *pos += 1;
                let expr = self.parse_unary(tokens, pos)?;
                let expr_span = expr.span();
                Ok(Expr::UnOp {
                    op: UnOp::Not,
                    expr: Box::new(expr),
                    span: Span::new(op_span.start, expr_span.end),
                })
            }
            Some(TokenKind::Tilde) => {
                let op_span = self.span(tokens, *pos);
                *pos += 1;
                let expr = self.parse_unary(tokens, pos)?;
                let expr_span = expr.span();
                Ok(Expr::UnOp {
                    op: UnOp::BitNot,
                    expr: Box::new(expr),
                    span: Span::new(op_span.start, expr_span.end),
                })
            }
            _ => {
                let expr = self.parse_atom(tokens, pos)?;
                self.parse_postfix(expr, tokens, pos)
            }
        }
    }

    fn parse_atom(&mut self, tokens: &[Token], pos: &mut usize) -> Result<Expr, ()> {
        let tok = match tokens.get(*pos) { Some(t) => t.clone(), None => { self.error("unexpected end".to_string(), None); return Err(()); } };
        *pos += 1;

        match tok.kind {
            TokenKind::Number => {
                let val: i64 = match tok.lexeme.parse() { Ok(v) => v, Err(_) => { self.error("invalid number".to_string(), None); return Err(()); } };
                Ok(Expr::Int(val, tok.span))
            }
            TokenKind::String => {
                let s = tok.lexeme.trim_matches('"').to_string();
                Ok(Expr::String(s, tok.span))
            }
            TokenKind::True => Ok(Expr::Bool(true, tok.span)),
            TokenKind::False => Ok(Expr::Bool(false, tok.span)),
            TokenKind::Match => {
                let match_expr = self.parse_match(tokens, pos)?;
                Ok(match_expr)
            }
            TokenKind::Ident => {
                if tokens.get(*pos).map(|t| t.kind) == Some(TokenKind::LParen) {
                    let call_start = tok.span.start;
                    *pos += 1;
                    let mut args = vec![];
                    if tokens.get(*pos).map(|t| t.kind) != Some(TokenKind::RParen) {
                        loop {
                            let arg = self.parse_expr(tokens, pos)?;
                            args.push(arg);
                            if tokens.get(*pos).map(|t| t.kind) == Some(TokenKind::Comma) {
                                *pos += 1;
                            } else {
                                break;
                            }
                        }
                    }
                    self.expect_noerr(TokenKind::RParen, tokens, pos);
                    let end = self.span(tokens, *pos - 1).end;
                    Ok(Expr::Call {
                        callee: Box::new(Expr::Ident(Ident { name: tok.lexeme, span: tok.span })),
                        args,
                        span: Span::new(call_start, end),
                    })
                } else {
                    Ok(Expr::Ident(Ident { name: tok.lexeme, span: tok.span }))
                }
            }
            TokenKind::LParen => {
                let expr = self.parse_expr(tokens, pos)?;
                self.expect_noerr(TokenKind::RParen, tokens, pos);
                Ok(expr)
            }
            TokenKind::LBrace => {
                *pos -= 1;
                let block = self.parse_block(tokens, pos)?;
                Ok(Expr::Block(block))
            }
            TokenKind::If => {
                *pos -= 1;
                let stmt = self.parse_if_stmt(tokens, pos)?;
                match stmt {
                    Stmt::If { cond, then, else_, span } => {
                        let else_expr = else_
                            .map(Expr::Block)
                            .unwrap_or(Expr::Block(Block { stmts: vec![], span: DUMMY_SPAN }));
                        Ok(Expr::If {
                            cond,
                            then: Box::new(Expr::Block(then)),
                            else_: Box::new(else_expr),
                            span,
                        })
                    }
                    _ => { self.error("expected if expression".to_string(), None); Err(()) },
                }
            }
            _ => { self.error(format!("unexpected token {:?} at expression", tok.kind), None); Err(()) },
        }
    }

    fn parse_match(&mut self, tokens: &[Token], pos: &mut usize) -> Result<Expr, ()> {
        let start = *pos - 1; // 'match' already consumed
        let expr = self.parse_expr(tokens, pos)?;
        self.expect_noerr(TokenKind::LBrace, tokens, pos);
        let mut arms = vec![];
        while tokens.get(*pos).map(|t| t.kind) != Some(TokenKind::RBrace) {
            let pattern = self.parse_pattern(tokens, pos)?;
            self.expect_noerr(TokenKind::Arrow, tokens, pos);
            let body = self.parse_expr(tokens, pos)?;
            if tokens.get(*pos).map(|t| t.kind) == Some(TokenKind::Comma) {
                *pos += 1;
            }
            arms.push(MatchArm {
                pattern,
                body,
                span: DUMMY_SPAN, // TODO: Calculate span
            });
        }
        self.expect_noerr(TokenKind::RBrace, tokens, pos);
        Ok(Expr::Match {
            expr: Box::new(expr),
            arms,
            span: Span::new(self.span(tokens, start).start, self.span(tokens, *pos - 1).end),
        })
    }

    fn parse_pattern(&mut self, tokens: &[Token], pos: &mut usize) -> Result<Pattern, ()> {
        let tok = match tokens.get(*pos) { Some(t) => t, None => { self.error("unexpected end".to_string(), None); return Err(()); } };
        match tok.kind {
            TokenKind::Ident if tok.lexeme == "_" => {
                let span = tok.span;
                *pos += 1;
                Ok(Pattern::Wildcard(span))
            }
            TokenKind::Ident => {
                let id = self.expect_ident(tokens, pos);
                Ok(Pattern::Ident(id))
            }
            _ => {
                let expr = self.parse_expr(tokens, pos)?;
                Ok(Pattern::Literal(expr))
            }
        }
    }

    fn parse_type(&mut self, tokens: &[Token], pos: &mut usize) -> Result<Type, ()> {
        let tok = match tokens.get(*pos) { Some(t) => t, None => { self.error("expected type".to_string(), None); return Err(()); } };
        *pos += 1;
        match tok.kind {
            TokenKind::Ident => match tok.lexeme.as_str() {
                "i32" => Ok(Type::I32),
                "i64" => Ok(Type::I64),
                "f32" => Ok(Type::F32),
                "f64" => Ok(Type::F64),
                "bool" => Ok(Type::Bool),
                "string" => Ok(Type::String),
                "void" => Ok(Type::Void),
                name => Ok(Type::Named(name.to_string())),
            },
            TokenKind::Star => {
                let inner = self.parse_type(tokens, pos)?;
                Ok(Type::Ptr(Box::new(inner)))
            }
            TokenKind::Ampersand => {
                let inner = self.parse_type(tokens, pos)?;
                Ok(Type::Ref(Box::new(inner)))
            }
            TokenKind::LBracket => {
                let inner = self.parse_type(tokens, pos)?;
                if tokens.get(*pos).map(|t| t.kind) == Some(TokenKind::Semicolon) {
                    *pos += 1;
                    let len_tok = match tokens.get(*pos) { Some(t) => t.clone(), None => { self.error("expected array length".to_string(), None); return Err(()); } };
                    self.expect_noerr(TokenKind::Number, tokens, pos);
                    let len: usize = match len_tok.lexeme.parse() { Ok(v) => v, Err(_) => { self.error("invalid array length".to_string(), None); return Err(()); } };
                    self.expect_noerr(TokenKind::RBracket, tokens, pos);
                    Ok(Type::Array(Box::new(inner), len))
                } else {
                    self.expect_noerr(TokenKind::RBracket, tokens, pos);
                    Ok(Type::Slice(Box::new(inner)))
                }
            }
            TokenKind::Fn => {
                self.expect_noerr(TokenKind::LParen, tokens, pos);
                let mut args = vec![];
                while tokens.get(*pos).map(|t| t.kind) != Some(TokenKind::RParen) {
                    args.push(self.parse_type(tokens, pos)?);
                    if tokens.get(*pos).map(|t| t.kind) == Some(TokenKind::Comma) {
                        *pos += 1;
                    }
                }
                self.expect_noerr(TokenKind::RParen, tokens, pos);
                self.expect_noerr(TokenKind::Arrow, tokens, pos);
                let ret = self.parse_type(tokens, pos)?;
                Ok(Type::Fn(args, Box::new(ret)))
            }
            _ => { self.error(format!("unexpected token {:?} in type", tok.kind), None); Err(()) },
        }
    }

    fn expect_noerr(&mut self, kind: TokenKind, tokens: &[Token], pos: &mut usize) {
        match tokens.get(*pos) {
            Some(t) if t.kind == kind => {
                *pos += 1;
            }
            Some(t) => {
                self.error(format!("expected {:?}, got {:?} ({:?})", kind, t.kind, t.lexeme), Some(t.span));
                *pos += 1;
            }
            None => {
                self.error(format!("expected {:?}, got EOF", kind), None);
            }
        }
    }

    fn expect_ident(&mut self, tokens: &[Token], pos: &mut usize) -> Ident {
        match tokens.get(*pos) {
            Some(t) if t.kind == TokenKind::Ident => {
                *pos += 1;
                Ident { name: t.lexeme.clone(), span: t.span }
            }
            Some(t) => {
                self.error(format!("expected identifier, got {:?}", t.kind), Some(t.span));
                *pos += 1;
                Ident { name: String::new(), span: t.span }
            }
            None => {
                self.error("expected identifier, got EOF".to_string(), None);
                Ident { name: String::new(), span: DUMMY_SPAN }
            }
        }
    }

    fn precedence(&self, kind: TokenKind) -> Option<u8> {
        match kind {
            TokenKind::PipePipe => Some(1),
            TokenKind::AndAnd => Some(2),
            TokenKind::Pipe => Some(3),
            TokenKind::Caret => Some(4),
            TokenKind::Ampersand => Some(5),
            TokenKind::EqualsEquals | TokenKind::BangEquals => Some(6),
            TokenKind::Less | TokenKind::LessEquals | TokenKind::Greater | TokenKind::GreaterEquals => Some(7),
            TokenKind::Shl | TokenKind::Shr => Some(8),
            TokenKind::Plus | TokenKind::Minus => Some(9),
            TokenKind::Star | TokenKind::Slash | TokenKind::Percent => Some(10),
            TokenKind::DoubleDot => Some(11),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::{AsciiLexer, BrakLexer};
    use brak_core::SourceMap;

    fn parse(src: &str) -> Program {
        let sm = SourceMap::new("test.brk", src);
        let mut lexer = AsciiLexer::new();
        let tokens = lexer.lex(&sm);
        let parser = Parser::new();
        parser.parse(&tokens).unwrap()
    }

    #[test]
    fn test_parse_empty() {
        let prog = parse("");
        assert!(prog.items.is_empty());
    }

    #[test]
    fn test_parse_fn_no_params() {
        let prog = parse("fn main() { return 42; }");
        assert_eq!(prog.items.len(), 1);
        match &prog.items[0] {
            Item::FnDef(f) => {
                assert_eq!(f.name.name, "main");
                assert!(f.params.is_empty());
            }
            _ => panic!("expected fn"),
        }
    }

    #[test]
    fn test_parse_fn_with_params() {
        let prog = parse("fn add(x, y) { return x + y; }");
        match &prog.items[0] {
            Item::FnDef(f) => {
                assert_eq!(f.params.len(), 2);
                assert_eq!(f.params[0].name.name, "x");
                assert_eq!(f.params[1].name.name, "y");
            }
            _ => panic!("expected fn"),
        }
    }

    #[test]
    fn test_parse_let() {
        let prog = parse("let x = 42;");
        match &prog.items[0] {
            Item::Let(l) => {
                assert_eq!(l.name.name, "x");
                assert!(l.value.is_some());
            }
            _ => panic!("expected let"),
        }
    }

    #[test]
    fn test_parse_if_else() {
        let prog = parse("fn test() { if x { return 1; } else { return 2; } }");
        match &prog.items[0] {
            Item::FnDef(f) => {
                assert_eq!(f.body.stmts.len(), 1);
                match &f.body.stmts[0] {
                    Stmt::If { else_, .. } => assert!(else_.is_some()),
                    _ => panic!("expected if"),
                }
            }
            _ => panic!("expected fn"),
        }
    }

    #[test]
    fn test_parse_while() {
        let prog = parse("fn test_while() { while x { doStuff(); } }");
        match &prog.items[0] {
            Item::FnDef(f) => {
                match &f.body.stmts[0] {
                    Stmt::While { .. } => {}
                    _ => panic!("expected while"),
                }
            }
            _ => panic!("expected fn"),
        }
    }

    #[test]
    fn test_parse_binary_expr() {
        let prog = parse("fn f() { return 1 + 2 * 3; }");
        match &prog.items[0] {
            Item::FnDef(f) => {
                match &f.body.stmts[0] {
                    Stmt::Return(Some(e), _) => {
                        assert!(matches!(e, Expr::BinOp { .. }));
                    }
                    _ => panic!("expected return"),
                }
            }
            _ => panic!("expected fn"),
        }
    }

    #[test]
    fn test_parse_call() {
        let prog = parse("fn f() { foo(1, 2); }");
        match &prog.items[0] {
            Item::FnDef(f) => {
                match &f.body.stmts[0] {
                    Stmt::Expr(e) => {
                        match e {
                            Expr::Call { args, .. } => assert_eq!(args.len(), 2),
                            _ => panic!("expected call"),
                        }
                    }
                    _ => panic!("expected expr stmt"),
                }
            }
            _ => panic!("expected fn"),
        }
    }

    #[test]
    fn test_parse_nested_block() {
        let prog = parse("fn f() { { let x = 1; } }");
        match &prog.items[0] {
            Item::FnDef(f) => {
                match &f.body.stmts[0] {
                    Stmt::Expr(e) => {
                        assert!(matches!(e, Expr::Block(_)));
                    }
                    _ => panic!("expected expr block"),
                }
            }
            _ => panic!("expected fn"),
        }
    }

    // --- Comprehensive Parser Tests ---

    #[test]
    fn test_parse_loop_stmt() {
        let prog = parse("fn f() { loop { break; } }");
        match &prog.items[0] {
            Item::FnDef(f) => {
                assert!(matches!(&f.body.stmts[0], Stmt::Loop { .. }));
                match &f.body.stmts[0] {
                    Stmt::Loop { body, .. } => {
                        assert!(matches!(&body.stmts[0], Stmt::Break(_)));
                    }
                    _ => panic!("expected loop"),
                }
            }
            _ => panic!("expected fn"),
        }
    }

    #[test]
    fn test_parse_for_loop() {
        let prog = parse("fn f() { for i in range { body(); } }");
        match &prog.items[0] {
            Item::FnDef(f) => {
                match &f.body.stmts[0] {
                    Stmt::For { var, iterable, .. } => {
                        assert_eq!(var.name, "i");
                        assert!(matches!(iterable.as_ref(), Expr::Ident(_)));
                    }
                    _ => panic!("expected for"),
                }
            }
            _ => panic!("expected fn"),
        }
    }

    #[test]
    fn test_parse_continue() {
        let prog = parse("fn f() { loop { continue; } }");
        match &prog.items[0] {
            Item::FnDef(f) => {
                match &f.body.stmts[0] {
                    Stmt::Loop { body, .. } => {
                        assert!(matches!(&body.stmts[0], Stmt::Continue(_)));
                    }
                    _ => panic!("expected loop"),
                }
            }
            _ => panic!("expected fn"),
        }
    }

    #[test]
    fn test_parse_unary_ops() {
        let prog = parse("fn f() { return -x; }");
        match &prog.items[0] {
            Item::FnDef(f) => {
                match &f.body.stmts[0] {
                    Stmt::Return(Some(e), _) => {
                        assert!(matches!(e, Expr::UnOp { op: UnOp::Neg, .. }));
                    }
                    _ => panic!("expected return"),
                }
            }
            _ => panic!("expected fn"),
        }
    }

    #[test]
    fn test_parse_not_op() {
        let prog = parse("fn f() { return !flag; }");
        match &prog.items[0] {
            Item::FnDef(f) => {
                match &f.body.stmts[0] {
                    Stmt::Return(Some(e), _) => {
                        assert!(matches!(e, Expr::UnOp { op: UnOp::Not, .. }));
                    }
                    _ => panic!("expected return"),
                }
            }
            _ => panic!("expected fn"),
        }
    }

    #[test]
    fn test_parse_comparison_ops() {
        let prog = parse("fn f() { return a < b; }");
        assert!(prog.items.len() == 1);
        let prog = parse("fn f() { return a <= b; }");
        assert!(prog.items.len() == 1);
        let prog = parse("fn f() { return a > b; }");
        assert!(prog.items.len() == 1);
        let prog = parse("fn f() { return a >= b; }");
        assert!(prog.items.len() == 1);
        let prog = parse("fn f() { return a == b; }");
        assert!(prog.items.len() == 1);
        let prog = parse("fn f() { return a != b; }");
        assert!(prog.items.len() == 1);
    }

    #[test]
    fn test_parse_bool_literals() {
        let prog = parse("fn f() { return true; }");
        match &prog.items[0] {
            Item::FnDef(f) => {
                match &f.body.stmts[0] {
                    Stmt::Return(Some(Expr::Bool(true, _)), _) => {}
                    _ => panic!("expected bool true"),
                }
            }
            _ => panic!("expected fn"),
        }
        let prog = parse("fn f() { return false; }");
        match &prog.items[0] {
            Item::FnDef(f) => {
                match &f.body.stmts[0] {
                    Stmt::Return(Some(Expr::Bool(false, _)), _) => {}
                    _ => panic!("expected bool false"),
                }
            }
            _ => panic!("expected fn"),
        }
    }

    #[test]
    fn test_parse_extern_fn() {
        let prog = parse("extern \"C\" fn puts(s);");
        match &prog.items[0] {
            Item::ExternFn(e) => {
                assert_eq!(e.name.name, "puts");
                assert_eq!(e.abi, "C");
                assert_eq!(e.params.len(), 1);
            }
            _ => panic!("expected extern fn"),
        }
    }

    #[test]
    fn test_parse_bitwise_ops() {
        let prog = parse("fn f() { return a & b; }");
        assert!(prog.items.len() == 1);
        let prog = parse("fn f() { return a | b; }");
        assert!(prog.items.len() == 1);
        let prog = parse("fn f() { return a ^ b; }");
        assert!(prog.items.len() == 1);
        let prog = parse("fn f() { return a << b; }");
        assert!(prog.items.len() == 1);
    }

    #[test]
    fn test_parse_assign_expr() {
        let prog = parse("fn f() { x = 42; }");
        match &prog.items[0] {
            Item::FnDef(f) => {
                match &f.body.stmts[0] {
                    Stmt::Expr(Expr::Assign(lhs, rhs, _)) => {
                        assert!(matches!(lhs.as_ref(), Expr::Ident(_)));
                        assert!(matches!(rhs.as_ref(), Expr::Int(42, _)));
                    }
                    _ => panic!("expected assign"),
                }
            }
            _ => panic!("expected fn"),
        }
    }

    #[test]
    fn test_parse_if_no_else() {
        let prog = parse("fn f() { if x { return 1; } }");
        match &prog.items[0] {
            Item::FnDef(f) => {
                match &f.body.stmts[0] {
                    Stmt::If { else_, .. } => assert!(else_.is_none()),
                    _ => panic!("expected if"),
                }
            }
            _ => panic!("expected fn"),
        }
    }

    #[test]
    fn test_parse_nested_if() {
        let prog = parse("fn f() { if a { if b { return 1; } } }");
        match &prog.items[0] {
            Item::FnDef(f) => {
                match &f.body.stmts[0] {
                    Stmt::If { then, .. } => {
                        match &then.stmts[0] {
                            Stmt::If { .. } => {}
                            _ => panic!("expected nested if"),
                        }
                    }
                    _ => panic!("expected if"),
                }
            }
            _ => panic!("expected fn"),
        }
    }

    #[test]
    fn test_parse_empty_return() {
        let prog = parse("fn f() { return; }");
        match &prog.items[0] {
            Item::FnDef(f) => {
                match &f.body.stmts[0] {
                    Stmt::Return(None, _) => {}
                    _ => panic!("expected empty return"),
                }
            }
            _ => panic!("expected fn"),
        }
    }

    #[test]
    fn test_parse_precedence() {
        // Logical OR has lowest precedence
        let prog = parse("fn f() { return a || b && c; }");
        match &prog.items[0] {
            Item::FnDef(f) => {
                match &f.body.stmts[0] {
                    Stmt::Return(Some(e), _) => {
                        assert!(matches!(e, Expr::BinOp { op: BinOp::Or, .. }));
                    }
                    _ => panic!("expected return"),
                }
            }
            _ => panic!("expected fn"),
        }
    }

    #[test]
    fn test_parse_pub_item() {
        let prog = parse("pub fn f() { }");
        match &prog.items[0] {
            Item::FnDef(_) => {}
            _ => panic!("expected fn"),
        }
        let prog = parse("pub let x = 1;");
        match &prog.items[0] {
            Item::Let(_) => {}
            _ => panic!("expected let"),
        }
    }

    #[test]
    fn test_parse_multiple_items() {
        let prog = parse("fn a() { } fn b() { } fn c() { }");
        assert_eq!(prog.items.len(), 3);
    }

    #[test]
    fn test_parse_struct_def() {
        let prog = parse("struct Point { x: i32, y: i32 }");
        assert_eq!(prog.items.len(), 1);
        match &prog.items[0] {
            Item::Struct(s) => {
                assert_eq!(s.name.name, "Point");
                assert_eq!(s.fields.len(), 2);
            }
            _ => panic!("expected struct"),
        }
    }

    #[test]
    fn test_parse_struct_empty() {
        let prog = parse("struct Empty { }");
        assert_eq!(prog.items.len(), 1);
        match &prog.items[0] {
            Item::Struct(s) => assert_eq!(s.fields.len(), 0),
            _ => panic!("expected struct"),
        }
    }

    #[test]
    fn test_parse_enum_def() {
        let prog = parse("enum Option { Some(i32), None }");
        assert_eq!(prog.items.len(), 1);
        match &prog.items[0] {
            Item::Enum(e) => {
                assert_eq!(e.name.name, "Option");
                assert_eq!(e.variants.len(), 2);
            }
            _ => panic!("expected enum"),
        }
    }

    #[test]
    fn test_parse_use_stmt() {
        let prog = parse("use std::io::print;");
        assert_eq!(prog.items.len(), 1);
        match &prog.items[0] {
            Item::Use(u) => {
                assert!(u.path.len() >= 3);
            }
            _ => panic!("expected use"),
        }
    }

    #[test]
    fn test_parse_const_def() {
        let prog = parse("const MAX: i32 = 100;");
        assert_eq!(prog.items.len(), 1);
        match &prog.items[0] {
            Item::Const(c) => {
                assert_eq!(c.name.name, "MAX");
            }
            _ => panic!("expected const"),
        }
    }

    #[test]
    fn test_parse_static_def() {
        let prog = parse("static NAME: string = \"hello\";");
        assert_eq!(prog.items.len(), 1);
        match &prog.items[0] {
            Item::Static(s) => {
                assert_eq!(s.name.name, "NAME");
            }
            _ => panic!("expected static"),
        }
    }

    #[test]
    fn test_parse_range_expr() {
        let prog = parse("fn f() { let r = 0..10; }");
        assert_eq!(prog.items.len(), 1);
    }

    #[test]
    fn test_parse_bitwise_not() {
        let prog = parse("fn f(x) { return ~x; }");
        assert_eq!(prog.items.len(), 1);
    }

    #[test]
    fn test_parse_string_literal() {
        let prog = parse("fn f() { let s = \"hello world\"; }");
        assert_eq!(prog.items.len(), 1);
    }

    #[test]
    fn test_parse_match_expr() {
        let prog = parse("fn f(x) { return match x { 1 -> 2, _ -> 0 }; }");
        assert_eq!(prog.items.len(), 1);
    }

    #[test]
    fn test_parse_let_with_type() {
        let prog = parse("fn f() { let x: i32 = 42; }");
        assert_eq!(prog.items.len(), 1);
    }

    #[test]
    fn test_parse_trait_def() {
        let prog = parse("trait Foo { fn bar(x) { } fn baz() -> i32 { return 0; } }");
        assert_eq!(prog.items.len(), 1);
        match &prog.items[0] {
            Item::Trait(t) => {
                assert_eq!(t.name.name, "Foo");
                assert_eq!(t.methods.len(), 2);
            }
            _ => panic!("expected trait"),
        }
    }

    #[test]
    fn test_parse_impl_def() {
        let prog = parse("impl Foo for i32 { fn bar(x) { return x; } }");
        assert_eq!(prog.items.len(), 1);
        match &prog.items[0] {
            Item::Impl(i) => {
                assert!(i.trait_name.is_some());
                assert_eq!(i.methods.len(), 1);
            }
            _ => panic!("expected impl"),
        }
    }

    #[test]
    fn test_parse_visibility_pub_struct() {
        let prog = parse("pub struct Point { pub x: i32, y: i32 }");
        assert_eq!(prog.items.len(), 1);
        match &prog.items[0] {
            Item::Struct(s) => {
                assert_eq!(s.fields[0].vis, Visibility::Public);
                assert_eq!(s.fields[1].vis, Visibility::Private);
            }
            _ => panic!("expected struct"),
        }
    }

    #[test]
    fn test_parse_for_with_block() {
        let prog = parse("fn f() { for i in 0..10 { let x = i; } }");
        assert_eq!(prog.items.len(), 1);
    }

    #[test]
    fn test_parse_nested_struct_type() {
        let prog = parse("fn f() { let p: *i32 = 0; }");
        assert_eq!(prog.items.len(), 1);
    }

    #[test]
    fn test_parse_fn_type() {
        let prog = parse("fn f() { let cb: fn(i32) -> void = 0; }");
        assert_eq!(prog.items.len(), 1);
    }
}
