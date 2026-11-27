//! Fixed-memory replay protection for authenticated frame sequences.

use core::fmt;

pub const REPLAY_WINDOW_BITS: u32 = 128;

/// A rejected sequence number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplayError {
    Duplicate { sequence: u64 },
    TooOld { sequence: u64, highest_seen: u64 },
}

impl fmt::Display for ReplayError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Duplicate { sequence } => write!(f, "duplicate sequence number: {sequence}"),
            Self::TooOld {
                sequence,
                highest_seen,
            } => write!(
                f,
                "sequence {sequence} is outside the replay window (highest {highest_seen})"
            ),
        }
    }
}

impl std::error::Error for ReplayError {}

/// A 128-message sliding replay window with constant memory use.
///
/// Apply this after authenticating a frame. Recording unauthenticated sequence
/// numbers would let an attacker advance the window and deny valid traffic.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReplayWindow {
    highest_seen: Option<u64>,
    bitmap: u128,
}

impl ReplayWindow {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn highest_seen(&self) -> Option<u64> {
        self.highest_seen
    }

    /// Accept a fresh sequence number or reject a duplicate/stale one.
    pub fn accept(&mut self, sequence: u64) -> Result<(), ReplayError> {
        let Some(highest) = self.highest_seen else {
            self.highest_seen = Some(sequence);
            self.bitmap = 1;
            return Ok(());
        };

        if sequence > highest {
            let shift = sequence - highest;
            self.bitmap = if shift >= REPLAY_WINDOW_BITS as u64 {
                1
            } else {
                (self.bitmap << shift) | 1
            };
            self.highest_seen = Some(sequence);
            return Ok(());
        }

        let distance = highest - sequence;
        if distance >= REPLAY_WINDOW_BITS as u64 {
            return Err(ReplayError::TooOld {
                sequence,
                highest_seen: highest,
            });
        }

        let mask = 1_u128 << distance;
        if self.bitmap & mask != 0 {
            return Err(ReplayError::Duplicate { sequence });
        }

        self.bitmap |= mask;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_sequence_is_accepted_and_duplicate_rejected() {
        let mut window = ReplayWindow::new();
        assert_eq!(window.accept(10), Ok(()));
        assert_eq!(
            window.accept(10),
            Err(ReplayError::Duplicate { sequence: 10 })
        );
        assert_eq!(window.highest_seen(), Some(10));
    }

    #[test]
    fn reordered_messages_inside_window_are_accepted_once() {
        let mut window = ReplayWindow::new();
        assert_eq!(window.accept(100), Ok(()));
        assert_eq!(window.accept(98), Ok(()));
        assert_eq!(window.accept(99), Ok(()));
        assert_eq!(
            window.accept(98),
            Err(ReplayError::Duplicate { sequence: 98 })
        );
    }

    #[test]
    fn messages_outside_window_are_rejected() {
        let mut window = ReplayWindow::new();
        window.accept(200).unwrap();
        window.accept(329).unwrap();
        assert_eq!(
            window.accept(200),
            Err(ReplayError::TooOld {
                sequence: 200,
                highest_seen: 329
            })
        );
    }

    #[test]
    fn exact_replay_window_boundary_is_inclusive_at_127() {
        let mut window = ReplayWindow::new();
        window.accept(999).unwrap();
        window.accept(1_128).unwrap();
        assert_eq!(window.accept(1_001), Ok(()));
        assert_eq!(
            window.accept(1_001),
            Err(ReplayError::Duplicate { sequence: 1_001 })
        );
    }

    #[test]
    fn sequence_128_positions_behind_highest_is_too_old() {
        let mut window = ReplayWindow::new();
        window.accept(1_000).unwrap();
        window.accept(1_128).unwrap();
        assert_eq!(
            window.accept(1_000),
            Err(ReplayError::TooOld {
                sequence: 1_000,
                highest_seen: 1_128
            })
        );
    }

    #[test]
    fn large_jump_and_u64_boundary_do_not_overflow() {
        let mut window = ReplayWindow::new();
        window.accept(1).unwrap();
        window.accept(u64::MAX).unwrap();
        assert_eq!(window.highest_seen(), Some(u64::MAX));
        assert_eq!(
            window.accept(1),
            Err(ReplayError::TooOld {
                sequence: 1,
                highest_seen: u64::MAX
            })
        );
    }
}
