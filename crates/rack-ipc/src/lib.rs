//! Inter-instance linking for plugin-rack.
//!
//! Stub — real implementation under issue "IPC: shared-memory link registry".
//! Design: SPSC ring via `rtrb` in a shared-memory segment keyed by a
//! user-persisted `link_tag`. Sibling discovery via PID registry with
//! heartbeat TTL. See research/ipc.md for the full spec.

#![forbid(unsafe_op_in_unsafe_fn)]

/// User-visible label that identifies a group of linked rack instances.
///
/// Instances on the same `LinkTag` in the same DAW session will discover
/// each other and render a shared console view. Empty tag = not linked.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LinkTag(pub String);

impl LinkTag {
    pub fn is_unlinked(&self) -> bool {
        self.0.is_empty()
    }
}
