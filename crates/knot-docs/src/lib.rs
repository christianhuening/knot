//! ACL resolver + cache + invalidation listener for knot.

pub mod acl;

pub use acl::{EffectiveRole, resolve};
