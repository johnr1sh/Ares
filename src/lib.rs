//! ARES Core reliability primitives.

#![forbid(unsafe_code)]
//!
//! The current crate provides a direction-aware state model, a bounded frame
//! codec, replay-window bookkeeping, and a monotonic probe/ack tracker.
//! It does not yet provide a socket transport, peer authentication, encryption,
//! discovery, media, or a production-ready session runtime.

mod direction;
mod receipt;
mod probe;
mod protocol;
mod replay;
mod session;

pub use direction::{Direction, DirectionHealth, DirectionState, TransitionError};
pub use receipt::{
    BidirectionalDelivery, DeliveryLedger, ReceiptError, ReceiptPolicy, ReceiptPolicyError,
    MAX_PENDING_RECEIPTS,
};
pub use probe::{
    ProbeError, ProbePolicy, ProbePolicyError, ProbeTracker, MAX_OUTSTANDING_PROBES,
};
pub use protocol::{
    decode_frame, encode_frame, Frame, FrameError, MessageKind, FRAME_MAGIC, HEADER_LEN,
    MAX_PAYLOAD_LEN, PROTOCOL_VERSION,
};
pub use replay::{ReplayError, ReplayWindow, REPLAY_WINDOW_BITS};
pub use session::{SessionHealth, SessionState};
