//! Graph data model: nodes, edges, and input structs for writes.

pub mod edge;
pub mod node;

pub use edge::Edge;
pub use node::Node;

use crate::id::NodeId;
use crate::props::Props;
use crate::time::Ts;

/// Input passed to `WriteTxn::add_edge`.
///
/// References endpoints by external (user-facing) id. The write layer resolves
/// these to internal `NodeId`s via the `ext_ids` table, auto-upserting if the
/// node does not yet exist.
#[derive(Clone, Debug)]
pub struct EdgeInput {
    pub from: String,
    pub to: String,
    pub edge_type: String,
    pub valid_from: Ts,
    pub valid_to: Option<Ts>,
    pub observed_at: Ts,
    pub source: String,
    pub confidence: f32,
    pub properties: Props,
}

impl EdgeInput {
    /// Minimal constructor with sane defaults: open-ended validity,
    /// observed-at = now, confidence = 1.0, no properties.
    pub fn new(
        from: impl Into<String>,
        to: impl Into<String>,
        edge_type: impl Into<String>,
    ) -> Self {
        Self {
            from: from.into(),
            to: to.into(),
            edge_type: edge_type.into(),
            valid_from: Ts::now(),
            valid_to: None,
            observed_at: Ts::now(),
            source: String::new(),
            confidence: 1.0,
            properties: Props::new(),
        }
    }
}

/// Input passed to `WriteTxn::upsert_node`.
#[derive(Clone, Debug)]
pub struct NodeInput {
    pub ext_id: String,
    pub node_type: Option<String>,
    pub properties: Props,
}

impl NodeInput {
    pub fn new(ext_id: impl Into<String>) -> Self {
        Self {
            ext_id: ext_id.into(),
            node_type: None,
            properties: Props::new(),
        }
    }

    pub fn with_type(mut self, ty: impl Into<String>) -> Self {
        self.node_type = Some(ty.into());
        self
    }
}

/// A resolved edge plus the string-typed fields spelled out (type name, source
/// name). This is what queries return.
#[derive(Clone, Debug)]
pub struct EdgeView {
    pub id: crate::id::EdgeId,
    pub from: NodeId,
    pub from_ext_id: String,
    pub to: NodeId,
    pub to_ext_id: String,
    pub edge_type: String,
    pub valid_from: Ts,
    pub valid_to: Option<Ts>,
    pub observed_at: Ts,
    pub source: String,
    pub confidence: f32,
    pub supersedes: Option<crate::id::EdgeId>,
    pub properties: Props,
}
