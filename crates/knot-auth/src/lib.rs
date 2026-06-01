//! Authentication primitives — password hashing, session tokens, login
//! throttle, CSRF tokens, OIDC client helpers.

pub mod password;

pub use password::{Hasher, PasswordError};
