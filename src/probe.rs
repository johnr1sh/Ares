//! Bounded acknowledgement tracking for periodic health probes.
//!
//! A probe/ack exchange measures a request/response path. A future two-peer
//! supervisor must combine both endpoints' observations and both probe
//! directions; an ACK timeout alone cannot prove which leg of a round trip
//! dropped the packet.

use crate::DirectionState;
use core::fmt;
use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Hard ceiling so callers cannot configure unbounded probe bookkeeping.
pub const MAX_OUTSTANDING_PROBES: usize = 1024;

/// Time and memory limits for one probe tracker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProbePolicy {
    pub acknowledgement_timeout: Duration,
    pub degraded_after_misses: u32,
    pub failed_after_misses: u32,
    pub max_outstanding: usize,
    pub recovery_successes: u32,
}

impl Default for ProbePolicy {
    fn default() -> Self {
        Self {
            acknowledgement_timeout: Duration::from_secs(1),
            degraded_after_misses: 2,
            failed_after_misses: 4,
            max_outstanding: 32,
            recovery_successes: 2,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbePolicyError {
    ZeroAcknowledgementTimeout,
    InvalidMissThresholds,
    ZeroOutstandingLimit,
    OutstandingLimitTooLarge { requested: usize, maximum: usize },
    ZeroRecoveryThreshold,
}

impl fmt::Display for ProbePolicyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroAcknowledgementTimeout => write!(f, "acknowledgement timeout must be non-zero"),
            Self::InvalidMissThresholds => {
                write!(f, "failure threshold must be greater than degradation threshold")
            }
            Self::ZeroOutstandingLimit => write!(f, "outstanding probe limit must be non-zero"),
            Self::OutstandingLimitTooLarge { requested, maximum } => write!(
                f,
                "outstanding probe limit {requested} exceeds maximum {maximum}"
            ),
            Self::ZeroRecoveryThreshold => write!(f, "recovery threshold must be non-zero"),
        }
    }
}

impl std::error::Error for ProbePolicyError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeError {
    Closed,
    SequenceNotIncreasing { previous: u64, next: u64 },
    OutstandingLimitReached,
    UnknownOrExpiredSequence(u64),
}

impl fmt::Display for ProbeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Closed => write!(f, "probe tracker is closed"),
            Self::SequenceNotIncreasing { previous, next } => {
                write!(f, "probe sequence must increase: previous {previous}, next {next}")
            }
            Self::OutstandingLimitReached => write!(f, "outstanding probe limit reached"),
            Self::UnknownOrExpiredSequence(sequence) => {
                write!(f, "unknown or expired probe sequence: {sequence}")
            }
        }
    }
}

impl std::error::Error for ProbeError {}

#[derive(Debug, Clone, Copy)]
struct OutstandingProbe {
    sequence: u64,
    sent_at: Instant,
}

/// A bounded tracker for one probe stream.
#[derive(Debug, Clone)]
pub struct ProbeTracker {
    policy: ProbePolicy,
    outstanding: VecDeque<OutstandingProbe>,
    last_sequence: Option<u64>,
    state: DirectionState,
    probes_sent: u64,
    acknowledgements_received: u64,
    missed_probes: u64,
    consecutive_misses: u32,
    recovery_successes: u32,
}

impl ProbeTracker {
    pub fn new(policy: ProbePolicy) -> Result<Self, ProbePolicyError> {
        if policy.acknowledgement_timeout.is_zero() {
            return Err(ProbePolicyError::ZeroAcknowledgementTimeout);
        }
        if policy.degraded_after_misses == 0
            || policy.failed_after_misses <= policy.degraded_after_misses
        {
            return Err(ProbePolicyError::InvalidMissThresholds);
        }
        if policy.max_outstanding == 0 {
            return Err(ProbePolicyError::ZeroOutstandingLimit);
        }
        if policy.max_outstanding > MAX_OUTSTANDING_PROBES {
            return Err(ProbePolicyError::OutstandingLimitTooLarge {
                requested: policy.max_outstanding,
                maximum: MAX_OUTSTANDING_PROBES,
            });
        }
        if policy.recovery_successes == 0 {
            return Err(ProbePolicyError::ZeroRecoveryThreshold);
        }

        Ok(Self {
            policy,
            outstanding: VecDeque::with_capacity(policy.max_outstanding),
            last_sequence: None,
            state: DirectionState::Unknown,
            probes_sent: 0,
            acknowledgements_received: 0,
            missed_probes: 0,
            consecutive_misses: 0,
            recovery_successes: 0,
        })
    }

    pub fn state(&self) -> DirectionState {
        self.state
    }

    pub fn probes_sent(&self) -> u64 {
        self.probes_sent
    }

    pub fn acknowledgements_received(&self) -> u64 {
        self.acknowledgements_received
    }

    pub fn missed_probes(&self) -> u64 {
        self.missed_probes
    }

    pub fn outstanding_count(&self) -> usize {
        self.outstanding.len()
    }

    /// Register a probe only after the caller has successfully submitted it
    /// to its transport. Sequence numbers must be strictly increasing.
    pub fn record_sent(&mut self, sequence: u64, now: Instant) -> Result<(), ProbeError> {
        if self.state == DirectionState::Closed {
            return Err(ProbeError::Closed);
        }
        if let Some(previous) = self.last_sequence {
            if sequence <= previous {
                return Err(ProbeError::SequenceNotIncreasing {
                    previous,
                    next: sequence,
                });
            }
        }
        if self.outstanding.len() >= self.policy.max_outstanding {
            return Err(ProbeError::OutstandingLimitReached);
        }

        self.last_sequence = Some(sequence);
        self.probes_sent = self.probes_sent.saturating_add(1);
        self.outstanding.push_back(OutstandingProbe {
            sequence,
            sent_at: now,
        });
        if self.state == DirectionState::Unknown {
            self.state = DirectionState::Starting;
        }
        Ok(())
    }

    /// Record a matching acknowledgement. Late or unsolicited ACKs are ignored
    /// by returning an error instead of falsely restoring health.
    pub fn record_acknowledgement(&mut self, sequence: u64) -> Result<(), ProbeError> {
        if self.state == DirectionState::Closed {
            return Err(ProbeError::Closed);
        }

        let position = self
            .outstanding
            .iter()
            .position(|probe| probe.sequence == sequence)
            .ok_or(ProbeError::UnknownOrExpiredSequence(sequence))?;
        let _ = self.outstanding.remove(position);

        self.acknowledgements_received = self.acknowledgements_received.saturating_add(1);
        self.consecutive_misses = 0;

        match self.state {
            DirectionState::Failed | DirectionState::Recovering => {
                self.state = DirectionState::Recovering;
                self.recovery_successes = self.recovery_successes.saturating_add(1);
                if self.recovery_successes >= self.policy.recovery_successes {
                    self.state = DirectionState::Healthy;
                    self.recovery_successes = 0;
                }
            }
            DirectionState::Closed => return Err(ProbeError::Closed),
            _ => {
                self.state = DirectionState::Healthy;
                self.recovery_successes = 0;
            }
        }

        Ok(())
    }

    /// Expire timed-out probes. Call this from a monotonic timer loop.
    pub fn evaluate(&mut self, now: Instant) -> DirectionState {
        if self.state == DirectionState::Closed {
            return self.state;
        }

        let mut index = 0;
        while index < self.outstanding.len() {
            let overdue = now.saturating_duration_since(self.outstanding[index].sent_at)
                >= self.policy.acknowledgement_timeout;
            if overdue {
                let _ = self.outstanding.remove(index);
                self.missed_probes = self.missed_probes.saturating_add(1);
                self.consecutive_misses = self.consecutive_misses.saturating_add(1);
            } else {
                index += 1;
            }
        }

        if self.consecutive_misses >= self.policy.failed_after_misses {
            self.state = DirectionState::Failed;
            self.outstanding.clear();
            self.recovery_successes = 0;
        } else if self.consecutive_misses >= self.policy.degraded_after_misses
            && self.state != DirectionState::Failed
        {
            self.state = DirectionState::Degraded;
        }

        self.state
    }

    /// Close permanently and release all outstanding probe state.
    pub fn close(&mut self) {
        self.outstanding.clear();
        self.state = DirectionState::Closed;
        self.recovery_successes = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy() -> ProbePolicy {
        ProbePolicy {
            acknowledgement_timeout: Duration::from_secs(1),
            degraded_after_misses: 1,
            failed_after_misses: 2,
            max_outstanding: 2,
            recovery_successes: 2,
        }
    }

    #[test]
    fn policy_rejects_unbounded_or_inconsistent_settings() {
        let mut invalid = policy();
        invalid.max_outstanding = 0;
        assert_eq!(
            ProbeTracker::new(invalid).err(),
            Some(ProbePolicyError::ZeroOutstandingLimit)
        );

        invalid = policy();
        invalid.max_outstanding = MAX_OUTSTANDING_PROBES + 1;
        assert_eq!(
            ProbeTracker::new(invalid).err(),
            Some(ProbePolicyError::OutstandingLimitTooLarge {
                requested: MAX_OUTSTANDING_PROBES + 1,
                maximum: MAX_OUTSTANDING_PROBES
            })
        );

        invalid = policy();
        invalid.failed_after_misses = invalid.degraded_after_misses;
        assert_eq!(
            ProbeTracker::new(invalid).err(),
            Some(ProbePolicyError::InvalidMissThresholds)
        );
    }

    #[test]
    fn matching_acknowledgement_marks_probe_stream_healthy() {
        let now = Instant::now();
        let mut tracker = ProbeTracker::new(policy()).unwrap();
        tracker.record_sent(1, now).unwrap();
        assert_eq!(tracker.state(), DirectionState::Starting);
        tracker.record_acknowledgement(1).unwrap();
        assert_eq!(tracker.state(), DirectionState::Healthy);
        assert_eq!(tracker.acknowledgements_received(), 1);
    }

    #[test]
    fn one_missed_probe_degrades_and_a_fresh_ack_restores_health() {
        let now = Instant::now();
        let mut tracker = ProbeTracker::new(policy()).unwrap();
        tracker.record_sent(1, now).unwrap();
        assert_eq!(
            tracker.evaluate(now + Duration::from_secs(1)),
            DirectionState::Degraded
        );

        tracker.record_sent(2, now + Duration::from_secs(2)).unwrap();
        tracker.record_acknowledgement(2).unwrap();
        assert_eq!(tracker.state(), DirectionState::Healthy);
    }

    #[test]
    fn missing_acknowledgements_degrade_then_fail() {
        let now = Instant::now();
        let mut tracker = ProbeTracker::new(policy()).unwrap();
        tracker.record_sent(1, now).unwrap();
        tracker.record_sent(2, now).unwrap();

        assert_eq!(
            tracker.evaluate(now + Duration::from_secs(1)),
            DirectionState::Failed
        );
        assert_eq!(tracker.missed_probes(), 2);
        assert_eq!(tracker.outstanding_count(), 0);
    }

    #[test]
    fn failed_tracker_requires_consecutive_successes_to_recover() {
        let now = Instant::now();
        let mut tracker = ProbeTracker::new(policy()).unwrap();
        tracker.record_sent(1, now).unwrap();
        tracker.record_sent(2, now).unwrap();
        tracker.evaluate(now + Duration::from_secs(1));
        assert_eq!(tracker.state(), DirectionState::Failed);

        let recovery_time = now + Duration::from_secs(2);
        tracker.record_sent(3, recovery_time).unwrap();
        tracker.record_acknowledgement(3).unwrap();
        assert_eq!(tracker.state(), DirectionState::Recovering);

        tracker.record_sent(4, recovery_time).unwrap();
        tracker.record_acknowledgement(4).unwrap();
        assert_eq!(tracker.state(), DirectionState::Healthy);
    }

    #[test]
    fn expired_acknowledgement_cannot_restore_health() {
        let now = Instant::now();
        let mut tracker = ProbeTracker::new(policy()).unwrap();
        tracker.record_sent(1, now).unwrap();
        tracker.evaluate(now + Duration::from_secs(1));
        assert_eq!(
            tracker.record_acknowledgement(1),
            Err(ProbeError::UnknownOrExpiredSequence(1))
        );
    }

    #[test]
    fn tracker_bounds_outstanding_work_and_closes_terminally() {
        let now = Instant::now();
        let mut tracker = ProbeTracker::new(policy()).unwrap();
        tracker.record_sent(1, now).unwrap();
        tracker.record_sent(2, now).unwrap();
        assert_eq!(
            tracker.record_sent(3, now),
            Err(ProbeError::OutstandingLimitReached)
        );
        tracker.close();
        assert_eq!(tracker.state(), DirectionState::Closed);
        assert_eq!(tracker.outstanding_count(), 0);
        assert_eq!(tracker.record_sent(3, now), Err(ProbeError::Closed));
    }

    #[test]
    fn sequences_must_increase_strictly() {
        let now = Instant::now();
        let mut tracker = ProbeTracker::new(policy()).unwrap();
        tracker.record_sent(8, now).unwrap();
        assert_eq!(
            tracker.record_sent(8, now),
            Err(ProbeError::SequenceNotIncreasing {
                previous: 8,
                next: 8
            })
        );
    }
}
