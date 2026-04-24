//! Filter expression evaluator.
//!
//! Applies a parsed [`Filter`] to materialized [`EdgeView`] rows. v0.2
//! supports first-class edge fields: `type`, `source`, `confidence`,
//! `valid_from`, `valid_to`, `observed_at`. Property-level predicates are
//! deferred to v0.3.

use crate::ast::{CmpOp, Filter, FilterTerm, Literal};
use chrona_core::{EdgeView, Error, Ts};

/// Return `true` if the edge satisfies every term in the filter.
pub fn matches(filter: &Filter, edge: &EdgeView) -> Result<bool, Error> {
    for term in &filter.terms {
        if !matches_term(term, edge)? {
            return Ok(false);
        }
    }
    Ok(true)
}

fn matches_term(term: &FilterTerm, edge: &EdgeView) -> Result<bool, Error> {
    match term.field.as_str() {
        "type" | "edge_type" => match &term.value {
            Literal::Str(s) => Ok(apply_str_op(term.op, &edge.edge_type, s)),
            other => Err(Error::Query(format!(
                "type filter expects a string, got {:?}",
                other
            ))),
        },
        "source" => match &term.value {
            Literal::Str(s) => Ok(apply_str_op(term.op, &edge.source, s)),
            other => Err(Error::Query(format!(
                "source filter expects a string, got {:?}",
                other
            ))),
        },
        "confidence" => {
            // Compare in f32 to match the stored precision of edge.confidence.
            // Promoting 0.9f32 to f64 changes it to 0.899999..., so a naïve
            // `>= 0.9f64` comparison fails for edges stored at exactly 0.9.
            let threshold = lit_to_f64(&term.value)? as f32;
            Ok(apply_f32_op(term.op, edge.confidence, threshold))
        }
        "valid_from" => {
            let threshold = lit_to_ts(&term.value)?;
            Ok(apply_i64_op(
                term.op,
                edge.valid_from.raw(),
                threshold.raw(),
            ))
        }
        "valid_to" => {
            let threshold = lit_to_ts(&term.value)?;
            let vt = edge.valid_to.unwrap_or(Ts::MAX);
            Ok(apply_i64_op(term.op, vt.raw(), threshold.raw()))
        }
        "observed_at" => {
            let threshold = lit_to_ts(&term.value)?;
            Ok(apply_i64_op(
                term.op,
                edge.observed_at.raw(),
                threshold.raw(),
            ))
        }
        other => Err(Error::Query(format!(
            "unknown filter field {:?}; supported: type, source, confidence, \
             valid_from, valid_to, observed_at",
            other
        ))),
    }
}

fn apply_str_op(op: CmpOp, left: &str, right: &str) -> bool {
    match op {
        CmpOp::Eq => left == right,
        CmpOp::Neq => left != right,
        CmpOp::Gt => left > right,
        CmpOp::Gte => left >= right,
        CmpOp::Lt => left < right,
        CmpOp::Lte => left <= right,
    }
}

fn apply_f32_op(op: CmpOp, left: f32, right: f32) -> bool {
    match op {
        CmpOp::Eq => left == right,
        CmpOp::Neq => left != right,
        CmpOp::Gt => left > right,
        CmpOp::Gte => left >= right,
        CmpOp::Lt => left < right,
        CmpOp::Lte => left <= right,
    }
}

fn apply_i64_op(op: CmpOp, left: i64, right: i64) -> bool {
    match op {
        CmpOp::Eq => left == right,
        CmpOp::Neq => left != right,
        CmpOp::Gt => left > right,
        CmpOp::Gte => left >= right,
        CmpOp::Lt => left < right,
        CmpOp::Lte => left <= right,
    }
}

fn lit_to_f64(lit: &Literal) -> Result<f64, Error> {
    Ok(match lit {
        Literal::Float(f) => *f,
        Literal::Int(n) => *n as f64,
        Literal::Str(s) => {
            return Err(Error::Query(format!(
                "numeric comparison expects a number, got string {:?}",
                s
            )))
        }
    })
}

fn lit_to_ts(lit: &Literal) -> Result<Ts, Error> {
    match lit {
        Literal::Str(s) => Ts::parse(s),
        other => Err(Error::Query(format!(
            "time comparison expects an RFC 3339 string, got {:?}",
            other
        ))),
    }
}

/// Truncate a vector to `limit` entries, returning the original when `None`.
pub fn apply_limit<T>(mut v: Vec<T>, limit: Option<u32>) -> Vec<T> {
    if let Some(n) = limit {
        v.truncate(n as usize);
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrona_core::{EdgeId, NodeId, Props};

    fn sample() -> EdgeView {
        EdgeView {
            id: EdgeId::from_raw(1),
            from: NodeId::from_raw(1),
            from_ext_id: "a".into(),
            to: NodeId::from_raw(2),
            to_ext_id: "b".into(),
            edge_type: "KNOWS".into(),
            valid_from: Ts::parse("2026-01-01").unwrap(),
            valid_to: None,
            observed_at: Ts::parse("2026-01-01").unwrap(),
            source: "slack".into(),
            confidence: 0.9,
            supersedes: None,
            properties: Props::new(),
        }
    }

    #[test]
    fn filter_empty_matches() {
        let f = Filter::default();
        assert!(matches(&f, &sample()).unwrap());
    }

    #[test]
    fn filter_type_eq() {
        let f = Filter {
            terms: vec![FilterTerm {
                field: "type".into(),
                op: CmpOp::Eq,
                value: Literal::Str("KNOWS".into()),
            }],
        };
        assert!(matches(&f, &sample()).unwrap());
    }

    #[test]
    fn filter_type_neq() {
        let f = Filter {
            terms: vec![FilterTerm {
                field: "type".into(),
                op: CmpOp::Neq,
                value: Literal::Str("KNOWS".into()),
            }],
        };
        assert!(!matches(&f, &sample()).unwrap());
    }

    #[test]
    fn filter_confidence_gte() {
        let f = Filter {
            terms: vec![FilterTerm {
                field: "confidence".into(),
                op: CmpOp::Gte,
                value: Literal::Float(0.5),
            }],
        };
        assert!(matches(&f, &sample()).unwrap());
    }

    #[test]
    fn filter_confidence_lt() {
        let f = Filter {
            terms: vec![FilterTerm {
                field: "confidence".into(),
                op: CmpOp::Lt,
                value: Literal::Float(0.5),
            }],
        };
        assert!(!matches(&f, &sample()).unwrap());
    }

    #[test]
    fn filter_multiple_terms_all_match() {
        let f = Filter {
            terms: vec![
                FilterTerm {
                    field: "type".into(),
                    op: CmpOp::Eq,
                    value: Literal::Str("KNOWS".into()),
                },
                FilterTerm {
                    field: "source".into(),
                    op: CmpOp::Eq,
                    value: Literal::Str("slack".into()),
                },
            ],
        };
        assert!(matches(&f, &sample()).unwrap());
    }

    #[test]
    fn filter_multiple_terms_one_fails() {
        let f = Filter {
            terms: vec![
                FilterTerm {
                    field: "type".into(),
                    op: CmpOp::Eq,
                    value: Literal::Str("KNOWS".into()),
                },
                FilterTerm {
                    field: "source".into(),
                    op: CmpOp::Eq,
                    value: Literal::Str("email".into()),
                },
            ],
        };
        assert!(!matches(&f, &sample()).unwrap());
    }

    #[test]
    fn filter_unknown_field_errors() {
        let f = Filter {
            terms: vec![FilterTerm {
                field: "bogus".into(),
                op: CmpOp::Eq,
                value: Literal::Str("x".into()),
            }],
        };
        assert!(matches(&f, &sample()).is_err());
    }

    #[test]
    fn filter_type_wrong_literal_errors() {
        let f = Filter {
            terms: vec![FilterTerm {
                field: "type".into(),
                op: CmpOp::Eq,
                value: Literal::Int(42),
            }],
        };
        assert!(matches(&f, &sample()).is_err());
    }

    #[test]
    fn apply_limit_trims() {
        let v = vec![1, 2, 3, 4, 5];
        assert_eq!(apply_limit(v.clone(), None).len(), 5);
        assert_eq!(apply_limit(v.clone(), Some(3)).len(), 3);
        assert_eq!(apply_limit(v, Some(100)).len(), 5);
    }
}
