//! CLAP guest plugin hosting via `clack-host`.
//!
//! Stub — real integration lands under issue "CLAP guest hosting".
//! Target dependency: `clack-host` (prokopyl/clack).

#![forbid(unsafe_op_in_unsafe_fn)]

pub struct ClapGuest;

impl ClapGuest {
    pub fn placeholder() -> Self {
        Self
    }
}
