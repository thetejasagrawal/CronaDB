//! Recursive-descent parser for the Chrona DSL.
//!
//! Grammar (informal):
//! ```text
//! query       := neighbor_q | hop_q | path_q | whoat_q | diff_q | changed_q
//! neighbor_q  := FIND NEIGHBORS OF string time_clause?
//! hop_q       := FIND INT HOPS FROM string time_clause?
//! path_q      := SHOW PATH FROM string TO string time_clause?
//! whoat_q     := WHO WAS CONNECTED TO string ON string
//! diff_q      := DIFF GRAPH BETWEEN string AND string (FOR NODE string)?
//! changed_q   := WHAT CHANGED BETWEEN string AND string (FOR NODE string)?
//! time_clause := (AT string) | (BEFORE string) | (AFTER string)
//! ```

use crate::ast::{Query, TimeClause};
use crate::lexer::{tokenize, Token};
use chrona_core::Error;

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0 }
    }

    fn peek(&self) -> &Token {
        &self.tokens[self.pos]
    }

    fn advance(&mut self) -> Token {
        let t = self.tokens[self.pos].clone();
        if self.pos + 1 < self.tokens.len() {
            self.pos += 1;
        }
        t
    }

    fn expect(&mut self, expected: &Token) -> Result<(), Error> {
        if std::mem::discriminant(self.peek()) == std::mem::discriminant(expected) {
            self.advance();
            Ok(())
        } else {
            Err(Error::Query(format!(
                "expected {}, got {}",
                expected.label(),
                self.peek().label()
            )))
        }
    }

    fn expect_string(&mut self) -> Result<String, Error> {
        match self.advance() {
            Token::String(s) => Ok(s),
            other => Err(Error::Query(format!(
                "expected STRING, got {}",
                other.label()
            ))),
        }
    }

    fn expect_integer(&mut self) -> Result<u64, Error> {
        match self.advance() {
            Token::Integer(n) => Ok(n),
            other => Err(Error::Query(format!(
                "expected INTEGER, got {}",
                other.label()
            ))),
        }
    }

    fn parse_time_clause(&mut self) -> Result<Option<TimeClause>, Error> {
        match self.peek() {
            Token::At => {
                self.advance();
                let s = self.expect_string()?;
                Ok(Some(TimeClause::At(s)))
            }
            Token::Before => {
                self.advance();
                let s = self.expect_string()?;
                Ok(Some(TimeClause::Before(s)))
            }
            Token::After => {
                self.advance();
                let s = self.expect_string()?;
                Ok(Some(TimeClause::After(s)))
            }
            _ => Ok(None),
        }
    }

    fn parse_optional_for_node(&mut self) -> Result<Option<String>, Error> {
        if matches!(self.peek(), Token::For) {
            self.advance();
            self.expect(&Token::Node)?;
            Ok(Some(self.expect_string()?))
        } else {
            Ok(None)
        }
    }

    fn parse_find(&mut self) -> Result<Query, Error> {
        // Already consumed FIND. Next is either NEIGHBORS or INT.
        match self.peek() {
            Token::Neighbors => {
                self.advance();
                self.expect(&Token::Of)?;
                let node = self.expect_string()?;
                let time = self.parse_time_clause()?;
                Ok(Query::Neighbors { node, time })
            }
            Token::Integer(_) => {
                let n = self.expect_integer()?;
                if n > u8::MAX as u64 {
                    return Err(Error::Query(format!(
                        "hops count {} exceeds maximum {}",
                        n,
                        u8::MAX
                    )));
                }
                self.expect(&Token::Hops)?;
                self.expect(&Token::From)?;
                let node = self.expect_string()?;
                let time = self.parse_time_clause()?;
                Ok(Query::Hops {
                    hops: n as u8,
                    node,
                    time,
                })
            }
            other => Err(Error::Query(format!(
                "after FIND, expected NEIGHBORS or INTEGER HOPS, got {}",
                other.label()
            ))),
        }
    }

    fn parse_show(&mut self) -> Result<Query, Error> {
        // Already consumed SHOW. Expect PATH.
        self.expect(&Token::Path)?;
        self.expect(&Token::From)?;
        let from = self.expect_string()?;
        self.expect(&Token::To)?;
        let to = self.expect_string()?;
        let time = self.parse_time_clause()?;
        Ok(Query::Path { from, to, time })
    }

    fn parse_who(&mut self) -> Result<Query, Error> {
        self.expect(&Token::Was)?;
        self.expect(&Token::Connected)?;
        self.expect(&Token::To)?;
        let node = self.expect_string()?;
        self.expect(&Token::On)?;
        let on = self.expect_string()?;
        Ok(Query::WhoConnected { node, on })
    }

    fn parse_diff(&mut self) -> Result<Query, Error> {
        self.expect(&Token::Graph)?;
        self.expect(&Token::Between)?;
        let t1 = self.expect_string()?;
        self.expect(&Token::And)?;
        let t2 = self.expect_string()?;
        let node = self.parse_optional_for_node()?;
        Ok(Query::Diff { t1, t2, node })
    }

    fn parse_what(&mut self) -> Result<Query, Error> {
        self.expect(&Token::Changed)?;
        self.expect(&Token::Between)?;
        let t1 = self.expect_string()?;
        self.expect(&Token::And)?;
        let t2 = self.expect_string()?;
        let node = self.parse_optional_for_node()?;
        Ok(Query::Changed { t1, t2, node })
    }

    fn parse_query(&mut self) -> Result<Query, Error> {
        let first = self.advance();
        let q = match first {
            Token::Find => self.parse_find()?,
            Token::Show => self.parse_show()?,
            Token::Who => self.parse_who()?,
            Token::Diff => self.parse_diff()?,
            Token::What => self.parse_what()?,
            other => {
                return Err(Error::Query(format!(
                    "unexpected token {} at start of query; expected one of FIND, SHOW, WHO, DIFF, WHAT",
                    other.label()
                )));
            }
        };
        if !matches!(self.peek(), Token::Eof) {
            return Err(Error::Query(format!(
                "trailing tokens after query; next is {}",
                self.peek().label()
            )));
        }
        Ok(q)
    }
}

/// Parse a DSL query string into an AST.
pub fn parse(input: &str) -> Result<Query, Error> {
    let tokens = tokenize(input)?;
    Parser::new(tokens).parse_query()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_neighbors() {
        let q = parse("FIND NEIGHBORS OF \"alice\"").unwrap();
        assert_eq!(
            q,
            Query::Neighbors {
                node: "alice".into(),
                time: None,
            }
        );
    }

    #[test]
    fn parse_neighbors_with_at() {
        let q = parse("FIND NEIGHBORS OF \"alice\" AT \"2026-01-01\"").unwrap();
        assert_eq!(
            q,
            Query::Neighbors {
                node: "alice".into(),
                time: Some(TimeClause::At("2026-01-01".into())),
            }
        );
    }

    #[test]
    fn parse_hops() {
        let q = parse("FIND 2 HOPS FROM \"x\" AT \"2026-02-01\"").unwrap();
        assert!(matches!(
            q,
            Query::Hops { hops: 2, node, time: Some(TimeClause::At(_)) } if node == "x"
        ));
    }

    #[test]
    fn parse_path() {
        let q = parse("SHOW PATH FROM \"a\" TO \"b\" BEFORE \"2026-03-10\"").unwrap();
        assert!(matches!(
            q,
            Query::Path { from, to, time: Some(TimeClause::Before(_)) }
                if from == "a" && to == "b"
        ));
    }

    #[test]
    fn parse_who() {
        let q = parse("WHO WAS CONNECTED TO \"Acme\" ON \"2026-03-01\"").unwrap();
        assert!(matches!(
            q,
            Query::WhoConnected { node, on } if node == "Acme" && on == "2026-03-01"
        ));
    }

    #[test]
    fn parse_diff() {
        let q = parse("DIFF GRAPH BETWEEN \"2026-01-01\" AND \"2026-04-01\"").unwrap();
        assert!(matches!(q, Query::Diff { .. }));
    }

    #[test]
    fn parse_diff_for_node() {
        let q = parse("DIFF GRAPH BETWEEN \"a\" AND \"b\" FOR NODE \"x\"").unwrap();
        assert!(matches!(
            q,
            Query::Diff { node: Some(n), .. } if n == "x"
        ));
    }

    #[test]
    fn parse_changed() {
        let q = parse("WHAT CHANGED BETWEEN \"2026-03-01\" AND \"2026-04-01\"").unwrap();
        assert!(matches!(q, Query::Changed { .. }));
    }

    #[test]
    fn error_trailing_tokens() {
        assert!(parse("FIND NEIGHBORS OF \"x\" FOOBAR").is_err());
    }

    #[test]
    fn error_wrong_token() {
        assert!(parse("FIND NEIGHBORS \"x\"").is_err()); // missing OF
    }

    #[test]
    fn error_hops_too_large() {
        assert!(parse("FIND 999999 HOPS FROM \"x\"").is_err());
    }
}
