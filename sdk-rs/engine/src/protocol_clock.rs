//! Per-engine calibration for outbound protocol timestamps only.
//! Host scheduling time never passes through this service.

#[derive(Default)]
pub(crate) struct ProtocolClock {
    offset_ms: f64,
    revision: u64,
    venue_calibrated: bool,
}

impl ProtocolClock {
    pub fn at(&self, host_ms: f64) -> f64 {
        host_ms + self.offset_ms
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Replace, rather than accumulate, the correction. Failed observations
    /// leave both the last valid correction and pending-signing revision intact.
    pub fn observe(&mut self, host_ms: f64, server_ms: f64) -> bool {
        if !self.replace(host_ms, server_ms) {
            return false;
        }
        self.venue_calibrated = true;
        true
    }

    /// Host wall-time synchronization is only the fallback. A venue's own
    /// observation remains authoritative on the stable scheduling clock.
    pub fn observe_host(&mut self, host_ms: f64, server_ms: f64) -> bool {
        if self.venue_calibrated {
            return host_ms.is_finite()
                && server_ms.is_finite()
                && (server_ms - host_ms).is_finite();
        }
        self.replace(host_ms, server_ms)
    }

    fn replace(&mut self, host_ms: f64, server_ms: f64) -> bool {
        if !host_ms.is_finite() || !server_ms.is_finite() {
            return false;
        }
        let offset_ms = server_ms - host_ms;
        if !offset_ms.is_finite() || !(host_ms + offset_ms).is_finite() {
            return false;
        }
        if offset_ms == self.offset_ms {
            return true;
        }
        let Some(revision) = self.revision.checked_add(1) else {
            return false;
        };
        self.offset_ms = offset_ms;
        self.revision = revision;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corrections_replace_are_idempotent_and_engine_local() {
        let mut first = ProtocolClock::default();
        let second = ProtocolClock::default();
        assert_eq!(first.at(1000.), 1000.);
        assert!(first.observe(1000., 1400.));
        assert_eq!((first.at(2000.), first.revision()), (2400., 1));
        assert_eq!((second.at(2000.), second.revision()), (2000., 0));
        assert!(first.observe(2000., 2400.));
        assert_eq!(first.revision(), 1);
        assert!(first.observe(2000., 1700.));
        assert_eq!((first.at(3000.), first.revision()), (2700., 2));
        assert!(first.observe(3000., 3000.));
        assert_eq!((first.at(4000.), first.revision()), (4000., 3));
    }

    #[test]
    fn host_sync_is_fallback_only_and_does_not_move_scheduling_time() {
        let mut clock = ProtocolClock::default();
        assert!(clock.observe_host(1_000.0, 1_800_000_000_000.0));
        assert_eq!(clock.at(13_000.0), 1_800_000_012_000.0);
        let revision = clock.revision();
        assert!(clock.observe_host(13_000.0, 1_800_000_012_000.0));
        assert_eq!(clock.revision(), revision);
        assert!(clock.observe(13_000.0, 1_800_000_011_900.0));
        let venue_revision = clock.revision();
        for utc in [1_786_003_200_000.0, 1_813_996_800_000.0] {
            assert!(clock.observe_host(13_000.0, utc));
            assert_eq!(clock.at(13_000.0), 1_800_000_011_900.0);
            assert_eq!(clock.revision(), venue_revision);
        }
        assert!(!clock.observe_host(f64::NAN, 1.0));
        assert!(!clock.observe_host(1.0, f64::INFINITY));
        assert_eq!(clock.at(13_000.0), 1_800_000_011_900.0);
    }

    #[test]
    fn invalid_observations_never_poison_calibration_or_revision() {
        let mut clock = ProtocolClock::default();
        assert!(clock.observe(1000., 1500.));
        for (host, server) in [
            (f64::NAN, 1000.),
            (1000., f64::NAN),
            (f64::INFINITY, 1000.),
            (1000., f64::NEG_INFINITY),
            (-f64::MAX, f64::MAX),
            (f64::MAX, -f64::MAX),
        ] {
            assert!(!clock.observe(host, server));
            assert_eq!((clock.at(1000.), clock.revision()), (1500., 1));
        }
    }

    #[test]
    fn exhausted_revision_rejects_changes_but_accepts_identical_observation() {
        let mut clock = ProtocolClock {
            offset_ms: 500.,
            revision: u64::MAX,
            venue_calibrated: false,
        };
        assert!(!clock.observe(1000., 1600.));
        assert_eq!((clock.at(1000.), clock.revision()), (1500., u64::MAX));
        assert!(clock.observe(1000., 1500.));
        assert_eq!((clock.at(1000.), clock.revision()), (1500., u64::MAX));
    }
}
