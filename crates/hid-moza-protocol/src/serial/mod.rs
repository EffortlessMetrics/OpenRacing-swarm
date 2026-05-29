//! Semantic-only Moza serial protocol scaffolding.
//!
//! This module does not expose wire codecs or hardware send paths.

#[doc(hidden)]
pub mod fake_transport;
#[doc(hidden)]
pub mod frame;
#[doc(hidden)]
pub mod response_semantics;
#[doc(hidden)]
pub mod status_probe;
#[doc(hidden)]
pub mod vendor_authority;
