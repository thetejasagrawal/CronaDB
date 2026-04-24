//! Tokenizer for the Chrona DSL.
//!
//! Keywords are case-insensitive and mapped to a fixed set. String literals
//! are double-quoted; the lexer recognizes simple `\"` and `\\` escapes.
//! Integer literals are ASCII decimal.

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
    // Literals
    String(String),
    Integer(u64),
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
            Token::String(_) => "STRING",
            Token::Integer(_) => "INTEGER",
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
        if c.is_ascii_digit() {
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            let s = std::str::from_utf8(&bytes[start..i])
                .map_err(|_| Error::Query("non-UTF8 integer".into()))?;
            let n: u64 = s
                .parse()
                .map_err(|_| Error::Query(format!("cannot parse integer {:?}", s)))?;
            out.push(Token::Integer(n));
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
                    return Err(Error::Query(format!(
                        "unexpected identifier {:?}; only keywords and quoted strings \
                         are allowed",
                        word
                    )));
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
    fn unknown_identifier_errors() {
        assert!(tokenize("FIND NEIGHBORS OF alice").is_err());
    }
}
