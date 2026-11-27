use core::fmt;

/// A communication direction is always evaluated independently.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Direction {
    AToB,
    BToA,
}

/// Health state for one direction of a peer session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirectionState {
    Unknown,
    Starting,
    Healthy,
    Degraded,
    Failed,
    Recovering,
    Closed,
}

/// Counters are evidence, not a substitute for an end-to-end health policy.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DirectionHealth {
    pub state: DirectionState,
    pub probes_sent: u64,
    pub probes_received: u64,
    pub acknowledgements_received: u64,
}

impl Default for DirectionState {
    fn default() -> Self {
        Self::Unknown
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransitionError {
    pub from: DirectionState,
    pub to: DirectionState,
}

impl fmt::Display for TransitionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "invalid direction-state transition: {:?} -> {:?}",
            self.from, self.to
        )
    }
}

impl std::error::Error for TransitionError {}

impl DirectionHealth {
    pub fn new() -> Self {
        Self::default()
    }

    /// Apply an explicit state transition. Failed directions must pass through
    /// Recovering before they can become Healthy again.
    pub fn transition(&mut self, next: DirectionState) -> Result<(), TransitionError> {
        if self.state == next {
            return Ok(());
        }

        let allowed = matches!(
            (self.state, next),
            (DirectionState::Unknown, DirectionState::Starting)
                | (DirectionState::Unknown, DirectionState::Closed)
                | (DirectionState::Starting, DirectionState::Healthy)
                | (DirectionState::Starting, DirectionState::Degraded)
                | (DirectionState::Starting, DirectionState::Failed)
                | (DirectionState::Starting, DirectionState::Closed)
                | (DirectionState::Healthy, DirectionState::Degraded)
                | (DirectionState::Healthy, DirectionState::Failed)
                | (DirectionState::Healthy, DirectionState::Closed)
                | (DirectionState::Degraded, DirectionState::Healthy)
                | (DirectionState::Degraded, DirectionState::Failed)
                | (DirectionState::Degraded, DirectionState::Recovering)
                | (DirectionState::Degraded, DirectionState::Closed)
                | (DirectionState::Failed, DirectionState::Recovering)
                | (DirectionState::Failed, DirectionState::Closed)
                | (DirectionState::Recovering, DirectionState::Healthy)
                | (DirectionState::Recovering, DirectionState::Degraded)
                | (DirectionState::Recovering, DirectionState::Failed)
                | (DirectionState::Recovering, DirectionState::Closed)
        );

        if !allowed {
            return Err(TransitionError {
                from: self.state,
                to: next,
            });
        }

        self.state = next;
        Ok(())
    }

    pub fn record_probe_sent(&mut self) {
        self.probes_sent = self.probes_sent.saturating_add(1);
    }

    pub fn record_probe_received(&mut self) {
        self.probes_received = self.probes_received.saturating_add(1);
    }

    pub fn record_acknowledgement_received(&mut self) {
        self.acknowledgements_received = self.acknowledgements_received.saturating_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_direction_starts_unknown() {
        assert_eq!(DirectionHealth::new().state, DirectionState::Unknown);
    }

    #[test]
    fn failed_direction_must_recover_before_healthy() {
        let mut health = DirectionHealth::new();
        health.transition(DirectionState::Starting).unwrap();
        health.transition(DirectionState::Healthy).unwrap();
        health.transition(DirectionState::Failed).unwrap();

        assert!(health.transition(DirectionState::Healthy).is_err());
        health.transition(DirectionState::Recovering).unwrap();
        health.transition(DirectionState::Healthy).unwrap();
    }

    #[test]
    fn counters_saturate_instead_of_wrapping() {
        let mut health = DirectionHealth::new();
        health.probes_sent = u64::MAX;
        health.record_probe_sent();
        assert_eq!(health.probes_sent, u64::MAX);
    }

    #[test]
    fn closed_state_is_terminal() {
        let mut health = DirectionHealth::new();
        health.transition(DirectionState::Starting).unwrap();
        health.transition(DirectionState::Closed).unwrap();
        assert!(health.transition(DirectionState::Starting).is_err());
    }
}
