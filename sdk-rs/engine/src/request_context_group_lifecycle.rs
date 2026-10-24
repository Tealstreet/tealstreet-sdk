use super::{Admission, Error, HttpRelease, Lifecycle, Permit, ReleaseTime, SendPacing, State};

pub(crate) enum ClaimPreparation {
    WaitUntil(f64),
    Ready(ClaimDraft),
}

// These tokens stay inside one engine turn; neither is Clone or serializable.
pub(crate) struct ClaimDraft {
    permit: Permit,
    release: HttpRelease,
}
pub(crate) struct PreparedClaim {
    permit: Permit,
    release: HttpRelease,
}
impl ClaimDraft {
    pub(crate) fn release(&self) -> &HttpRelease {
        &self.release
    }
    pub(crate) fn admit(self, admission: Admission) -> Result<PreparedClaim, Error> {
        let mut release = self.release;
        match admission {
            Admission::Unscheduled => {}
            Admission::Scheduled(lease) => {
                release.response_lease = Some(release.deadline.min(lease.0));
            }
            Admission::Denied => return Err(Error::Revoked),
        }
        Ok(PreparedClaim {
            permit: self.permit,
            release,
        })
    }
}
impl Lifecycle {
    #[cfg(feature = "http-plans")]
    pub(crate) fn group_needs_activation(&self) -> bool {
        matches!(self.state, State::Prepared(_))
    }

    #[cfg(feature = "http-plans")]
    pub(crate) fn validate_group_activation(&self, now: f64) -> Result<(), Error> {
        let State::Prepared(route) = self.state else {
            return Err(Error::State);
        };
        if route.expires_at.is_some_and(|expiry| now >= expiry) {
            return Err(Error::Stale);
        }
        if self.permits == u64::MAX {
            return Err(Error::Capacity);
        }
        Ok(())
    }

    #[cfg(feature = "http-plans")]
    pub(crate) fn group_send_before(&self, release: &HttpRelease) -> Result<f64, Error> {
        let State::Permitted(route, _) = self.state else {
            return Err(Error::State);
        };
        let before = self
            .deadline
            .min(self.fresh_until)
            .min(route.expires_at.unwrap_or(f64::MAX))
            .min(release.signature_valid_until.unwrap_or(f64::MAX));
        if !before.is_finite() || before <= release.time.raw_ms {
            return Err(Error::Stale);
        }
        Ok(before)
    }

    pub(crate) fn prepare_claim(
        &self,
        permit: Permit,
        time: ReleaseTime,
        pacing: &SendPacing,
    ) -> Result<ClaimPreparation, Error> {
        let State::Permitted(route, expected) = self.state else {
            return Err(Error::State);
        };
        if permit != expected {
            return Err(Error::Revoked);
        }
        let release = HttpRelease::new(self.attempt, time, self.signature, self.deadline)?;
        if route.expires_at.is_some_and(|at| time.raw_ms >= at)
            || self.signed_revision.is_some_and(|rev| rev != time.revision)
            || time.raw_ms >= self.fresh_until
        {
            return Err(Error::Stale);
        }
        if let Some(at) = pacing.check(self.context.pacing, time.raw_ms)? {
            return if at >= self.deadline {
                Err(Error::TimedOut)
            } else {
                Ok(ClaimPreparation::WaitUntil(at))
            };
        }
        Ok(ClaimPreparation::Ready(ClaimDraft { permit, release }))
    }

    // Engine orchestration finalizes every member before invoking any commit.
    pub(crate) fn commit_claim(
        &mut self,
        ready: PreparedClaim,
        pacing: &mut SendPacing,
    ) -> HttpRelease {
        debug_assert!(self.is_current_permit(ready.permit));
        pacing.released(self.context.pacing, ready.release.time.raw_ms);
        self.state = State::Released(ready.release);
        ready.release
    }
}
