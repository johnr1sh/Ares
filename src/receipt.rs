//! End-to-end delivery evidence for one direction.
//!
//! A local socket/QUIC send result is not proof of remote delivery. This ledger
//! becomes Healthy only after an authenticated remote receipt report. A
//! missing-delivery report must also come from the authenticated receiver;
//! silence alone never proves which direction failed.

use crate::{Direction, DirectionHealth, DirectionState, SessionHealth, SessionState};
use core::fmt;
use std::collections::VecDeque;

/// Hard ceiling for unconfirmed delivery records in one direction.
pub const MAX_PENDING_RECEIPTS: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReceiptPolicy {
    pub max_pending: usize,
    pub degraded_after_missing: u32,
    pub failed_after_missing: u32,
    pub recovery_successes: u32,
}

impl Default for ReceiptPolicy {
    fn default() -> Self {
        Self {
            max_pending: 256,
            degraded_after_missing: 1,
            failed_after_missing: 3,
            recovery_successes: 2,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReceiptPolicyError {
    ZeroPendingLimit,
    PendingLimitTooLarge { requested: usize, maximum: usize },
    InvalidMissingThresholds,
    ZeroRecoveryThreshold,
}

impl fmt::Display for ReceiptPolicyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroPendingLimit => write!(f, "pending receipt limit must be non-zero"),
            Self::PendingLimitTooLarge { requested, maximum } => write!(
                f,
                "pending receipt limit {requested} exceeds maximum {maximum}"
            ),
            Self::InvalidMissingThresholds => write!(
                f,
                "failure threshold must be greater than degradation threshold"
            ),
            Self::ZeroRecoveryThreshold => write!(f, "recovery threshold must be non-zero"),
        }
    }
}

impl std::error::Error for ReceiptPolicyError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReceiptError {
    Closed,
    SequenceNotIncreasing { previous: u64, next: u64 },
    ReportSequenceNotIncreasing { previous: u64, next: u64 },
    PendingLimitReached,
    UnknownOrAlreadyReported(u64),
}

impl fmt::Display for ReceiptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Closed => write!(f, "receipt ledger is closed"),
            Self::SequenceNotIncreasing { previous, next } => write!(
                f,
                "sequence must increase: previous {previous}, next {next}"
            ),
            Self::ReportSequenceNotIncreasing { previous, next } => write!(
                f,
                "receipt reports must be ordered: previous {previous}, next {next}"
            ),
            Self::PendingLimitReached => write!(f, "pending receipt limit reached"),
            Self::UnknownOrAlreadyReported(sequence) => {
                write!(f, "unknown or already reported sequence: {sequence}")
            }
        }
    }
}

impl std::error::Error for ReceiptError {}

/// Evidence ledger for one sender-to-receiver direction.
///
/// The caller must authenticate the remote peer and bind each report to the
/// correct session and direction before calling record_remote_received or
/// record_remote_missing. A receiver should report missing probes only after
/// its own observation deadline.
#[derive(Debug, Clone)]
pub struct DeliveryLedger {
    policy: ReceiptPolicy,
    pending: VecDeque<u64>,
    last_sequence: Option<u64>,
    last_reported_sequence: Option<u64>,
    last_observed_sequence: u64,
    state: DirectionState,
    confirmed_received: u64,
    confirmed_missing: u64,
    consecutive_missing: u32,
    recovery_successes: u32,
}

impl DeliveryLedger {
    pub fn new(policy: ReceiptPolicy) -> Result<Self, ReceiptPolicyError> {
        if policy.max_pending == 0 {
            return Err(ReceiptPolicyError::ZeroPendingLimit);
        }
        if policy.max_pending > MAX_PENDING_RECEIPTS {
            return Err(ReceiptPolicyError::PendingLimitTooLarge {
                requested: policy.max_pending,
                maximum: MAX_PENDING_RECEIPTS,
            });
        }
        if policy.degraded_after_missing == 0
            || policy.failed_after_missing <= policy.degraded_after_missing
        {
            return Err(ReceiptPolicyError::InvalidMissingThresholds);
        }
        if policy.recovery_successes == 0 {
            return Err(ReceiptPolicyError::ZeroRecoveryThreshold);
        }

        Ok(Self {
            policy,
            pending: VecDeque::with_capacity(policy.max_pending),
            last_sequence: None,
            last_reported_sequence: None,
            last_observed_sequence: 0,
            state: DirectionState::Unknown,
            confirmed_received: 0,
            confirmed_missing: 0,
            consecutive_missing: 0,
            recovery_successes: 0,
        })
    }

    pub fn state(&self) -> DirectionState {
        self.state
    }

    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    pub fn confirmed_received(&self) -> u64 {
        self.confirmed_received
    }

    pub fn confirmed_missing(&self) -> u64 {
        self.confirmed_missing
    }

    /// Register a sequence after the transport accepts the outbound send.
    /// This records local submission, not proof of remote delivery.
    pub fn record_sent(&mut self, sequence: u64) -> Result<(), ReceiptError> {
        if self.state == DirectionState::Closed {
            return Err(ReceiptError::Closed);
        }
        if let Some(previous) = self.last_sequence {
            if sequence <= previous {
                return Err(ReceiptError::SequenceNotIncreasing {
                    previous,
                    next: sequence,
                });
            }
        }
        if self.pending.len() >= self.policy.max_pending {
            return Err(ReceiptError::PendingLimitReached);
        }

        self.last_sequence = Some(sequence);
        self.pending.push_back(sequence);
        if self.state == DirectionState::Unknown {
            self.state = DirectionState::Starting;
        }
        Ok(())
    }

    /// Apply an authenticated receiver report confirming delivery.
    pub fn record_remote_received(&mut self, sequence: u64) -> Result<(), ReceiptError> {
        self.remove_pending(sequence)?;
        self.confirmed_received = self.confirmed_received.saturating_add(1);
        self.consecutive_missing = 0;

        match self.state {
            DirectionState::Failed | DirectionState::Recovering => {
                self.state = DirectionState::Recovering;
                self.recovery_successes = self.recovery_successes.saturating_add(1);
                if self.recovery_successes >= self.policy.recovery_successes {
                    self.state = DirectionState::Healthy;
                    self.recovery_successes = 0;
                }
            }
            DirectionState::Closed => return Err(ReceiptError::Closed),
            _ => {
                self.state = DirectionState::Healthy;
                self.recovery_successes = 0;
            }
        }
        Ok(())
    }

    /// Apply an authenticated receiver report that this sequence was not
    /// received before the receiver's declared observation deadline.
    ///
    /// Do not call this because an ACK timed out locally: only the receiver
    /// can attest that it did not observe the sequence.
    pub fn record_remote_missing(&mut self, sequence: u64) -> Result<(), ReceiptError> {
        self.remove_pending(sequence)?;
        let recovery_was_in_progress = self.state == DirectionState::Recovering;
        self.confirmed_missing = self.confirmed_missing.saturating_add(1);
        self.consecutive_missing = self.consecutive_missing.saturating_add(1);
        self.recovery_successes = 0;

        if recovery_was_in_progress || self.consecutive_missing >= self.policy.failed_after_missing
        {
            self.state = DirectionState::Failed;
        } else if self.consecutive_missing >= self.policy.degraded_after_missing {
            self.state = DirectionState::Degraded;
        }
        Ok(())
    }

    /// Permanently close the ledger and release pending state.
    pub fn close(&mut self) {
        self.pending.clear();
        self.state = DirectionState::Closed;
        self.recovery_successes = 0;
    }

    fn remove_pending(&mut self, sequence: u64) -> Result<(), ReceiptError> {
        if self.state == DirectionState::Closed {
            return Err(ReceiptError::Closed);
        }
        if let Some(previous) = self.last_reported_sequence {
            if sequence <= previous {
                return Err(ReceiptError::ReportSequenceNotIncreasing {
                    previous,
                    next: sequence,
                });
            }
        }
        let position = self
            .pending
            .iter()
            .position(|pending| *pending == sequence)
            .ok_or(ReceiptError::UnknownOrAlreadyReported(sequence))?;
        let _ = self.pending.remove(position);
        self.last_reported_sequence = Some(sequence);
        Ok(())
    }
}


/// Two independent delivery ledgers for one A/B session.
///
/// A direction becomes healthy only from a receiver-originated receipt report.
/// A one-way result requires explicit receiver evidence for one direction and
/// successful receipt evidence for the opposite direction.
#[derive(Debug, Clone)]
pub struct BidirectionalDelivery {
    a_to_b: DeliveryLedger,
    b_to_a: DeliveryLedger,
}

impl BidirectionalDelivery {
    pub fn new(policy: ReceiptPolicy) -> Result<Self, ReceiptPolicyError> {
        Ok(Self {
            a_to_b: DeliveryLedger::new(policy)?,
            b_to_a: DeliveryLedger::new(policy)?,
        })
    }

    pub fn direction(&self, direction: Direction) -> &DeliveryLedger {
        match direction {
            Direction::AToB => &self.a_to_b,
            Direction::BToA => &self.b_to_a,
        }
    }

    pub fn direction_mut(&mut self, direction: Direction) -> &mut DeliveryLedger {
        match direction {
            Direction::AToB => &mut self.a_to_b,
            Direction::BToA => &mut self.b_to_a,
        }
    }

    pub fn state(&self) -> SessionState {
        SessionHealth {
            a_to_b: DirectionHealth {
                state: self.a_to_b.state(),
                ..DirectionHealth::default()
            },
            b_to_a: DirectionHealth {
                state: self.b_to_a.state(),
                ..DirectionHealth::default()
            },
        }
        .state()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy() -> ReceiptPolicy {
        ReceiptPolicy {
            max_pending: 2,
            degraded_after_missing: 1,
            failed_after_missing: 2,
            recovery_successes: 2,
        }
    }

    #[test]
    fn send_submission_alone_never_proves_delivery() {
        let mut ledger = DeliveryLedger::new(policy()).unwrap();
        ledger.record_sent(1).unwrap();
        assert_eq!(ledger.state(), DirectionState::Starting);
        assert_eq!(ledger.confirmed_received(), 0);
    }

    #[test]
    fn only_matching_remote_receipt_confirms_delivery() {
        let mut ledger = DeliveryLedger::new(policy()).unwrap();
        ledger.record_sent(1).unwrap();
        assert_eq!(
            ledger.record_remote_received(2),
            Err(ReceiptError::UnknownOrAlreadyReported(2))
        );
        ledger.record_remote_received(1).unwrap();
        assert_eq!(ledger.state(), DirectionState::Healthy);
        assert_eq!(ledger.confirmed_received(), 1);
        assert_eq!(ledger.pending_count(), 0);
    }

    #[test]
    fn explicit_receiver_reports_degrade_then_fail_one_direction() {
        let mut ledger = DeliveryLedger::new(policy()).unwrap();
        ledger.record_sent(1).unwrap();
        ledger.record_sent(2).unwrap();
        ledger.record_remote_missing(1).unwrap();
        assert_eq!(ledger.state(), DirectionState::Degraded);
        ledger.record_remote_missing(2).unwrap();
        assert_eq!(ledger.state(), DirectionState::Failed);
        assert_eq!(ledger.confirmed_missing(), 2);
    }

    #[test]
    fn recovery_requires_consecutive_remote_receipts() {
        let mut ledger = DeliveryLedger::new(policy()).unwrap();
        ledger.record_sent(1).unwrap();
        ledger.record_sent(2).unwrap();
        ledger.record_remote_missing(1).unwrap();
        ledger.record_remote_missing(2).unwrap();
        assert_eq!(ledger.state(), DirectionState::Failed);

        ledger.record_sent(3).unwrap();
        ledger.record_remote_received(3).unwrap();
        assert_eq!(ledger.state(), DirectionState::Recovering);

        ledger.record_sent(4).unwrap();
        ledger.record_remote_received(4).unwrap();
        assert_eq!(ledger.state(), DirectionState::Healthy);
    }

    #[test]
    fn a_missing_report_breaks_recovery_streak() {
        let mut ledger = DeliveryLedger::new(policy()).unwrap();
        for sequence in 1..=2 {
            ledger.record_sent(sequence).unwrap();
            ledger.record_remote_missing(sequence).unwrap();
        }
        ledger.record_sent(3).unwrap();
        ledger.record_remote_received(3).unwrap();
        ledger.record_sent(4).unwrap();
        ledger.record_remote_missing(4).unwrap();
        assert_eq!(ledger.state(), DirectionState::Failed);
        assert_eq!(ledger.confirmed_received(), 1);
    }

    #[test]
    fn pending_work_and_sequence_order_are_bounded() {
        let mut ledger = DeliveryLedger::new(policy()).unwrap();
        ledger.record_sent(5).unwrap();
        assert_eq!(
            ledger.record_sent(5),
            Err(ReceiptError::SequenceNotIncreasing {
                previous: 5,
                next: 5
            })
        );
        ledger.record_sent(6).unwrap();
        assert_eq!(
            ledger.record_sent(7),
            Err(ReceiptError::PendingLimitReached)
        );
        ledger.record_remote_received(5).unwrap();
        ledger.record_sent(7).unwrap();
    }

    #[test]
    fn receiver_reports_must_be_applied_in_sequence_order() {
        let mut ledger = DeliveryLedger::new(policy()).unwrap();
        ledger.record_sent(1).unwrap();
        ledger.record_sent(2).unwrap();
        ledger.record_remote_received(2).unwrap();
        assert_eq!(
            ledger.record_remote_missing(1),
            Err(ReceiptError::ReportSequenceNotIncreasing {
                previous: 2,
                next: 1
            })
        );
    }

    #[test]
    fn closed_ledger_is_terminal() {
        let mut ledger = DeliveryLedger::new(policy()).unwrap();
        ledger.record_sent(1).unwrap();
        ledger.close();
        assert_eq!(ledger.state(), DirectionState::Closed);
        assert_eq!(ledger.pending_count(), 0);
        assert_eq!(ledger.record_sent(2), Err(ReceiptError::Closed));
        assert_eq!(ledger.record_remote_received(1), Err(ReceiptError::Closed));
    }

    #[test]
    fn bidirectional_delivery_detects_confirmed_one_way_failure() {
        let mut session = BidirectionalDelivery::new(policy()).unwrap();

        session
            .direction_mut(Direction::AToB)
            .record_sent(1)
            .unwrap();
        session
            .direction_mut(Direction::AToB)
            .record_remote_missing(1)
            .unwrap();
        session
            .direction_mut(Direction::AToB)
            .record_sent(2)
            .unwrap();
        session
            .direction_mut(Direction::AToB)
            .record_remote_missing(2)
            .unwrap();

        session
            .direction_mut(Direction::BToA)
            .record_sent(1)
            .unwrap();
        session
            .direction_mut(Direction::BToA)
            .record_remote_received(1)
            .unwrap();

        assert_eq!(session.state(), SessionState::OneWay);
        assert_eq!(
            session.direction(Direction::AToB).state(),
            DirectionState::Failed
        );
        assert_eq!(
            session.direction(Direction::BToA).state(),
            DirectionState::Healthy
        );
    }

    #[test]
    fn missing_reverse_direction_evidence_is_not_misreported_as_one_way() {
        let mut session = BidirectionalDelivery::new(policy()).unwrap();
        session
            .direction_mut(Direction::AToB)
            .record_sent(1)
            .unwrap();
        session
            .direction_mut(Direction::AToB)
            .record_remote_missing(1)
            .unwrap();
        session
            .direction_mut(Direction::AToB)
            .record_sent(2)
            .unwrap();
        session
            .direction_mut(Direction::AToB)
            .record_remote_missing(2)
            .unwrap();

        session
            .direction_mut(Direction::BToA)
            .record_sent(1)
            .unwrap();
        assert_eq!(session.state(), SessionState::Degraded);
    }

    #[test]
    fn two_explicitly_failed_directions_are_unavailable() {
        let mut session = BidirectionalDelivery::new(policy()).unwrap();
        for direction in [Direction::AToB, Direction::BToA] {
            session.direction_mut(direction).record_sent(1).unwrap();
            session
                .direction_mut(direction)
                .record_remote_missing(1)
                .unwrap();
            session.direction_mut(direction).record_sent(2).unwrap();
            session
                .direction_mut(direction)
                .record_remote_missing(2)
                .unwrap();
        }
        assert_eq!(session.state(), SessionState::Unavailable);
    }

    #[test]
    fn one_direction_receipt_does_not_make_session_healthy() {
        let mut session = BidirectionalDelivery::new(policy()).unwrap();
        session.direction_mut(Direction::AToB).record_sent(1).unwrap();
        session
            .direction_mut(Direction::AToB)
            .record_remote_received(1)
            .unwrap();
        assert_eq!(session.state(), SessionState::Degraded);
    }

    #[test]
    fn policy_rejects_unbounded_and_inconsistent_limits() {
        let mut invalid = policy();
        invalid.max_pending = MAX_PENDING_RECEIPTS + 1;
        assert_eq!(
            DeliveryLedger::new(invalid).err(),
            Some(ReceiptPolicyError::PendingLimitTooLarge {
                requested: MAX_PENDING_RECEIPTS + 1,
                maximum: MAX_PENDING_RECEIPTS
            })
        );
        invalid = policy();
        invalid.max_pending = 0;
        assert_eq!(
            DeliveryLedger::new(invalid).err(),
            Some(ReceiptPolicyError::ZeroPendingLimit)
        );

        invalid = policy();
        invalid.recovery_successes = 0;
        assert_eq!(
            DeliveryLedger::new(invalid).err(),
            Some(ReceiptPolicyError::ZeroRecoveryThreshold)
        );

        invalid = policy();
        invalid.failed_after_missing = invalid.degraded_after_missing;
        assert_eq!(
            DeliveryLedger::new(invalid).err(),
            Some(ReceiptPolicyError::InvalidMissingThresholds)
        );
    }
    #[test]
    fn receiver_observation_does_not_require_a_local_send() {
        let mut ledger = DeliveryLedger::new(policy()).unwrap();
        ledger.record_observed_received(1).unwrap();
        assert_eq!(ledger.state(), DirectionState::Healthy);
        assert_eq!(ledger.confirmed_received(), 1);
        assert_eq!(ledger.pending_count(), 0);
    }

    #[test]
    fn receiver_observed_deadlines_fail_only_the_observed_direction() {
        let mut session = BidirectionalDelivery::new(policy()).unwrap();
        let inbound = session.direction_mut(Direction::BToA);
        inbound.record_observed_missing(1).unwrap();
        assert_eq!(inbound.state(), DirectionState::Degraded);
        inbound.record_observed_missing(2).unwrap();
        assert_eq!(inbound.state(), DirectionState::Failed);
        session
            .direction_mut(Direction::AToB)
            .record_observed_received(1)
            .unwrap();
        assert_eq!(session.state(), SessionState::OneWay);
    }

    #[test]
    fn late_or_duplicate_receiver_observations_are_rejected() {
        let mut ledger = DeliveryLedger::new(policy()).unwrap();
        ledger.record_observed_missing(2).unwrap();
        assert_eq!(
            ledger.record_observed_received(1),
            Err(ReceiptError::ReportSequenceNotIncreasing {
                previous: 2,
                next: 1
            })
        );
    }
}
