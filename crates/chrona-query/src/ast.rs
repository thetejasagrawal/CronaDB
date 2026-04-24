//! Abstract syntax tree for the Chrona DSL.

/// A time constraint on a traversal query.
#[derive(Clone, Debug, PartialEq)]
pub enum TimeClause {
    /// Evaluate the query as of the given time.
    At(String),
    /// Evaluate as of the given time, treating it as an upper bound.
    Before(String),
    /// Evaluate as of the given time (treated the same as `At` in v0.2).
    After(String),
}

/// A comparison operator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CmpOp {
    Eq,
    Neq,
    Gt,
    Gte,
    Lt,
    Lte,
}

impl CmpOp {
    /// Human-readable name.
    pub fn symbol(&self) -> &'static str {
        match self {
            CmpOp::Eq => "=",
            CmpOp::Neq => "!=",
            CmpOp::Gt => ">",
            CmpOp::Gte => ">=",
            CmpOp::Lt => "<",
            CmpOp::Lte => "<=",
        }
    }
}

/// A literal value in a filter expression.
#[derive(Clone, Debug, PartialEq)]
pub enum Literal {
    Str(String),
    Int(u64),
    Float(f64),
}

/// A single comparison inside a `WHERE` clause.
#[derive(Clone, Debug, PartialEq)]
pub struct FilterTerm {
    pub field: String,
    pub op: CmpOp,
    pub value: Literal,
}

/// A conjunction of filter terms. `AND`-joined. `OR` is not yet supported.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Filter {
    pub terms: Vec<FilterTerm>,
}

impl Filter {
    /// True if the filter has no terms.
    pub fn is_empty(&self) -> bool {
        self.terms.is_empty()
    }
}

/// Every parseable top-level query.
#[derive(Clone, Debug, PartialEq)]
pub enum Query {
    /// `FIND NEIGHBORS OF "x" [AT "..."]`
    Neighbors {
        node: String,
        time: Option<TimeClause>,
        filter: Filter,
        limit: Option<u32>,
    },
    /// `FIND n HOPS FROM "x" [AT "..."]`
    Hops {
        hops: u8,
        node: String,
        time: Option<TimeClause>,
        filter: Filter,
        limit: Option<u32>,
    },
    /// `SHOW PATH FROM "a" TO "b" [AT|BEFORE|AFTER "..."]`
    Path {
        from: String,
        to: String,
        time: Option<TimeClause>,
        filter: Filter,
        limit: Option<u32>,
    },
    /// `WHO WAS CONNECTED TO "x" ON "..."`
    WhoConnected {
        node: String,
        on: String,
        filter: Filter,
        limit: Option<u32>,
    },
    /// `DIFF GRAPH BETWEEN "..." AND "..." [FOR NODE "x"]`
    Diff {
        t1: String,
        t2: String,
        node: Option<String>,
    },
    /// `WHAT CHANGED BETWEEN "..." AND "..." [FOR NODE "x"]`
    Changed {
        t1: String,
        t2: String,
        node: Option<String>,
    },
}
