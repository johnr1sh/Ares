//! ARES Core primitives.
//!
//! This crate currently contains transport-independent session state logic.
//! It does not yet implement networking, cryptography, discovery, or media.

mod direction;
mod session;

pub use direction::{Direction, DirectionHealth, DirectionState, TransitionError};
pub use session::{SessionHealth, SessionState};
