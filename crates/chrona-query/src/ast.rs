//! Abstract syntax tree for the Chrona DSL.

/// A time constraint on a traversal query.
#[derive(Clone, Debug, PartialEq)]
pub enum TimeClause {
    At(String),
    Before(String),
    After(String),
}

/// Every parseable top-level query.
#[derive(Clone, Debug, PartialEq)]
pub enum Query {
    /// `FIND NEIGHBORS OF "x" [AT "..."]`
    Neighbors {
        node: String,
        time: Option<TimeClause>,
    },
    /// `FIND n HOPS FROM "x" [AT "..."]`
    Hops {
        hops: u8,
        node: String,
        time: Option<TimeClause>,
    },
    /// `SHOW PATH FROM "a" TO "b" [AT|BEFORE|AFTER "..."]`
    Path {
        from: String,
        to: String,
        time: Option<TimeClause>,
    },
    /// `WHO WAS CONNECTED TO "x" ON "..."`
    WhoConnected { node: String, on: String },
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
