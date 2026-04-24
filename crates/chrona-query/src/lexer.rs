//! Tokenizer for the Chrona DSL.
//!
//! Keywords are case-insensitive and mapped to a fixed set. String literals
//! are double-quoted; the lexer recognizes simple `\"`, `\\`, `\n`, `\t`
//! escapes. Integer and float literals are ASCII decimal.

use chrona_core::Error;

/// One token from the input.
#[derive(Clone, Debug, PartialEq)]
pub enum Token {
    // Structural keywords
    Find,
    Show,
    Who,
    Diff,
    What,
    Neighbors,
    Hops,
    Path,
    Graph,
    Was,
    Changed,
    Node,
    // Prepositions / connectives
    Of,
    From,
    To,
    Connected,
    On,
    Between,
    And,
    For,
    // Time keywords
    At,
    Before,
    After,
    // Filter / limit keywords
    Where,
    Limit,
    // Comparison operators
    Eq,
    Neq,
    Gt,
    Gte,
    Lt,
    Lte,
    // Literals
    String(String),
    Integer(u64),
    Float(f64),
    Ident(String),
    // End of input
    Eof,
}

impl Token {
    /// Human-readable name of the token.
    pub fn label(&self) -> &'static str {
        match self {
            Token::Find => "FIND",
            Token::Show => "SHOW",
            Token::Who => "WHO",
            Token::Diff => "DIFF",
            Token::What => "WHAT",
            Token::Neighbors => "NEIGHBORS",
            Token::Hops => "HOPS",
            Token::Path => "PATH",
            Token::Graph => "GRAPH",
            Token::Was => "WAS",
            Token::Changed => "CHANGED",
            Token::Node => "NODE",
            Token::Of => "OF",
            Token::From => "FROM",
            Token::To => "TO",
            Token::Connected => "CONNECTED",
            Token::On => "ON",
            Token::Between => "BETWEEN",
            Token::And => "AND",
            Token::For => "FOR",
            Token::At => "AT",
            Token::Before => "BEFORE",
            Token::After => "AFTER",
            Token::Where => "WHERE",
            Token::Limit => "LIMIT",
            Token::Eq => "=",
            Token::Neq => "!=",
            Token::Gt => ">",
            Token::Gte => ">=",
            Token::Lt => "<",
            Token::Lte => "<=",
            Token::String(_) => "STRING",
            Token::Integer(_) => "INTEGER",
            Token::Float(_) => "FLOAT",
            Token::Ident(_) => "IDENT",
            Token::Eof => "EOF",
        }
    }
}

fn keyword_to_token(s: &str) -> Option<Token> {
    let upper = s.to_ascii_uppercase();
    Some(match upper.as_str() {
        "FIND" => Token::Find,
        "SHOW" => Token::Show,
        "WHO" => Token::Who,
        "DIFF" => Token::Diff,
        "WHAT" => Token::What,
        "NEIGHBORS" => Token::Neighbors,
        "HOPS" => Token::Hops,
        "PATH" => Token::Path,
        "GRAPH" => Token::Graph,
        "WAS" => Token::Was,
        "CHANGED" => Token::Changed,
        "NODE" => Token::Node,
        "OF" => Token::Of,
        "FROM" => Token::From,
        "TO" => Token::To,
        "CONNECTED" => Token::Connected,
        "ON" => Token::On,
        "BETWEEN" => Token::Between,
        "AND" => Token::And,
        "FOR" => Token::For,
        "AT" => Token::At,
        "BEFORE" => Token::Before,
        "AFTER" => Token::After,
        "WHERE" => Token::Where,
        "LIMIT" => Token::Limit,
        _ => return None,
    })
}

/// Lex an entire input string into a vector of tokens plus a trailing `Eof`.
pub fn tokenize(input: &str) -> Result<Vec<Token>, Error> {
    let mut out = Vec::new();
    let bytes = input.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        let c = bytes[i];
        if c.is_ascii_whitespace() {
            i += 1;
            continue;
        }

        // Comparison operators.
        if c == b'=' {
            out.push(Token::Eq);
            i += 1;
            continue;
        }
        if c == b'!' && i + 1 < bytes.len() && bytes[i + 1] == b'=' {
            out.push(Token::Neq);
            i += 2;
            continue;
        }
        if c == b'>' {
            if i + 1 < bytes.len() && bytes[i + 1] == b'=' {
                out.push(Token::Gte);
                i += 2;
            } else {
                out.push(Token::Gt);
                i += 1;
            }
            continue;
        }
        if c == b'<' {
            if i + 1 < bytes.len() && bytes[i + 1] == b'=' {
                out.push(Token::Lte);
                i += 2;
            } else {
                out.push(Token::Lt);
                i += 1;
            }
            continue;
        }

        if c == b'"' {
            // String literal.
            let start = i + 1;
            let mut buf = String::new();
            i += 1;
            while i < bytes.len() && bytes[i] != b'"' {
                if bytes[i] == b'\\' && i + 1 < bytes.len() {
                    match bytes[i + 1] {
                        b'"' => buf.push('"'),
                        b'\\' => buf.push('\\'),
                        b'n' => buf.push('\n'),
                        b't' => buf.push('\t'),
                        other => {
                            return Err(Error::Query(format!(
                                "invalid escape \\{} at offset {}",
                                other as char, i
                            )));
                        }
                    }
                    i += 2;
                } else {
                    buf.push(bytes[i] as char);
                    i += 1;
                }
            }
            if i >= bytes.len() {
                return Err(Error::Query(format!(
                    "unterminated string literal starting at offset {}",
                    start
                )));
            }
            i += 1; // closing quote
            out.push(Token::String(buf));
            continue;
        }
        if c.is_ascii_digit() || (c == b'-' && i + 1 < bytes.len() && bytes[i + 1].is_ascii_digit())
        {
            // Number literal — may be integer or float.
            let start = i;
            if c == b'-' {
                i += 1;
            }
            let mut saw_dot = false;
            while i < bytes.len() && (bytes[i].is_ascii_digit() || (bytes[i] == b'.' && !saw_dot)) {
                if bytes[i] == b'.' {
                    saw_dot = true;
                }
                i += 1;
            }
            let s = std::str::from_utf8(&bytes[start..i])
                .map_err(|_| Error::Query("non-UTF8 number".into()))?;
            if saw_dot {
                let f: f64 = s
                    .parse()
                    .map_err(|_| Error::Query(format!("cannot parse float {:?}", s)))?;
                out.push(Token::Float(f));
            } else {
                // For unsigned, reject negatives (LIMIT/HOPS must be positive).
                if s.starts_with('-') {
                    return Err(Error::Query(format!(
                        "negative integer not allowed here: {}",
                        s
                    )));
                }
                let n: u64 = s
                    .parse()
                    .map_err(|_| Error::Query(format!("cannot parse integer {:?}", s)))?;
                out.push(Token::Integer(n));
            }
            continue;
        }
        if c.is_ascii_alphabetic() || c == b'_' {
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            let word = std::str::from_utf8(&bytes[start..i])
                .map_err(|_| Error::Query("non-UTF8 identifier".into()))?;
            match keyword_to_token(word) {
                Some(t) => out.push(t),
                None => {
                    // Unknown bare identifier is kept as an Ident token. The
                    // parser decides whether it's legal in context (e.g.
                    // inside WHERE, yes; as a node id, no).
                    out.push(Token::Ident(word.to_string()));
                }
            }
            continue;
        }

        return Err(Error::Query(format!(
            "unexpected character {:?} at offset {}",
            c as char, i
        )));
    }

    out.push(Token::Eof);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenize_simple_neighbor_query() {
        let toks = tokenize("FIND NEIGHBORS OF \"alice\"").unwrap();
        assert_eq!(
            toks,
            vec![
                Token::Find,
                Token::Neighbors,
                Token::Of,
                Token::String("alice".into()),
                Token::Eof,
            ]
        );
    }

    #[test]
    fn keywords_case_insensitive() {
        let toks = tokenize("find neighbors of \"x\"").unwrap();
        assert_eq!(toks[0], Token::Find);
        assert_eq!(toks[1], Token::Neighbors);
    }

    #[test]
    fn integer_and_time_clause() {
        let toks = tokenize("FIND 3 HOPS FROM \"x\" AT \"2026-01-01\"").unwrap();
        assert_eq!(toks[1], Token::Integer(3));
        assert_eq!(toks[5], Token::At);
    }

    #[test]
    fn string_escape() {
        let toks = tokenize(r#"FIND NEIGHBORS OF "a\"b""#).unwrap();
        assert_eq!(toks[3], Token::String("a\"b".into()));
    }

    #[test]
    fn unterminated_string_errors() {
        assert!(tokenize(r#"FIND NEIGHBORS OF "foo"#).is_err());
    }

    #[test]
    fn bare_identifier_is_ident() {
        let toks = tokenize("FIND NEIGHBORS OF alice").unwrap();
        assert_eq!(toks[3], Token::Ident("alice".into()));
    }

    #[test]
    fn where_clause_tokens() {
        let toks =
            tokenize(r#"FIND NEIGHBORS OF "alice" WHERE confidence >= 0.8 AND type = "KNOWS""#)
                .unwrap();
        assert!(toks.contains(&Token::Where));
        assert!(toks.contains(&Token::Gte));
        assert!(toks.contains(&Token::Float(0.8)));
        assert!(toks.contains(&Token::And));
        assert!(toks.contains(&Token::Eq));
    }

    #[test]
    fn limit_clause() {
        let toks = tokenize(r#"FIND NEIGHBORS OF "alice" LIMIT 5"#).unwrap();
        assert!(toks.contains(&Token::Limit));
        assert!(toks.contains(&Token::Integer(5)));
    }

    #[test]
    fn comparison_operators() {
        let toks = tokenize("= != > >= < <=").unwrap();
        assert_eq!(
            toks,
            vec![
                Token::Eq,
                Token::Neq,
                Token::Gt,
                Token::Gte,
                Token::Lt,
                Token::Lte,
                Token::Eof,
            ]
        );
    }

    #[test]
    fn float_parsing() {
        let toks = tokenize("0.5 1.0 0.123").unwrap();
        assert_eq!(toks[0], Token::Float(0.5));
        assert_eq!(toks[1], Token::Float(1.0));
        assert_eq!(toks[2], Token::Float(0.123));
    }
}
