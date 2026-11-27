use crate::{Direction, DirectionHealth, DirectionState};

/// Derived session health. It never replaces the per-direction records.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    Unknown,
    Starting,
    BidirectionalHealthy,
    Degraded,
    OneWay,
    Unavailable,
    Closed,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SessionHealth {
    pub a_to_b: DirectionHealth,
    pub b_to_a: DirectionHealth,
}

impl SessionHealth {
    pub fn direction(&self, direction: Direction) -> &DirectionHealth {
        match direction {
            Direction::AToB => &self.a_to_b,
            Direction::BToA => &self.b_to_a,
        }
    }

    pub fn direction_mut(&mut self, direction: Direction) -> &mut DirectionHealth {
        match direction {
            Direction::AToB => &mut self.a_to_b,
            Direction::BToA => &mut self.b_to_a,
        }
    }

    /// Derive aggregate health without assuming direction independence.
    ///
    /// One healthy direction is not enough for a healthy session. A confirmed
    /// failed/closed opposite direction is reported as OneWay; incomplete or
    /// recovering evidence remains Degraded rather than being called healthy.
    pub fn state(&self) -> SessionState {
        use DirectionState as D;
        use SessionState as S;

        let a = self.a_to_b.state;
        let b = self.b_to_a.state;

        if a == D::Closed && b == D::Closed {
            return S::Closed;
        }
        if a == D::Unknown && b == D::Unknown {
            return S::Unknown;
        }
        if a == D::Healthy && b == D::Healthy {
            return S::BidirectionalHealthy;
        }
        if (a == D::Healthy && matches!(b, D::Failed | D::Closed))
            || (b == D::Healthy && matches!(a, D::Failed | D::Closed))
        {
            return S::OneWay;
        }
        if matches!(a, D::Failed | D::Closed) && matches!(b, D::Failed | D::Closed) {
            return S::Unavailable;
        }
        if matches!(a, D::Failed | D::Closed) || matches!(b, D::Failed | D::Closed) {
            return S::Degraded;
        }
        if a == D::Healthy || b == D::Healthy {
            return S::Degraded;
        }
        if a == D::Starting || b == D::Starting {
            return S::Starting;
        }
        S::Degraded
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(health: &mut SessionHealth, direction: Direction, state: DirectionState) {
        health.direction_mut(direction).state = state;
    }

    #[test]
    fn both_directions_must_be_healthy() {
        let mut health = SessionHealth::default();
        set(&mut health, Direction::AToB, DirectionState::Healthy);
        assert_eq!(health.state(), SessionState::Degraded);

        set(&mut health, Direction::BToA, DirectionState::Healthy);
        assert_eq!(health.state(), SessionState::BidirectionalHealthy);
    }

    #[test]
    fn one_way_failure_is_not_reported_as_connected() {
        let mut health = SessionHealth::default();
        set(&mut health, Direction::AToB, DirectionState::Healthy);
        set(&mut health, Direction::BToA, DirectionState::Failed);
        assert_eq!(health.state(), SessionState::OneWay);
    }

    #[test]
    fn two_failed_directions_are_unavailable() {
        let mut health = SessionHealth::default();
        set(&mut health, Direction::AToB, DirectionState::Failed);
        set(&mut health, Direction::BToA, DirectionState::Failed);
        assert_eq!(health.state(), SessionState::Unavailable);
    }

    #[test]
    fn both_closed_directions_close_the_session() {
        let mut health = SessionHealth::default();
        set(&mut health, Direction::AToB, DirectionState::Closed);
        set(&mut health, Direction::BToA, DirectionState::Closed);
        assert_eq!(health.state(), SessionState::Closed);
    }

    #[test]
    fn healthy_and_starting_is_degraded_not_healthy_or_starting() {
        let mut health = SessionHealth::default();
        set(&mut health, Direction::AToB, DirectionState::Healthy);
        set(&mut health, Direction::BToA, DirectionState::Starting);
        assert_eq!(health.state(), SessionState::Degraded);
    }

    #[test]
    fn healthy_and_unknown_is_degraded_until_the_other_direction_is_proven() {
        let mut health = SessionHealth::default();
        set(&mut health, Direction::AToB, DirectionState::Healthy);
        assert_eq!(health.state(), SessionState::Degraded);
    }

    #[test]
    fn direction_counters_remain_independent() {
        let mut health = SessionHealth::default();
        health.direction_mut(Direction::AToB).record_probe_sent();
        assert_eq!(health.a_to_b.probes_sent, 1);
        assert_eq!(health.b_to_a.probes_sent, 0);
    }
}
