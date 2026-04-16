//! VST3 guest plugin hosting.
//!
//! Stub — real integration lands under issue "VST3 guest hosting".
//! Target dependency: `vst3` crate (coupler-rs/vst3-rs), MIT-licensed.

#![forbid(unsafe_op_in_unsafe_fn)]

pub struct Vst3Guest;

impl Vst3Guest {
    pub fn placeholder() -> Self {
        Self
    }
}
