//! Engine clock metadata observed only at a successful final signing release.
use crate::request_context::{Error, ReleaseTime};
use crate::signing::SignReceipt;

/// The stamp a resend must exceed: the genuine released receipt's stamp. Only
/// `SigningRelease::stamp_floor` makes one, so a venue cannot mint it from a
/// response, a header or a clock. It constrains a stamp and grants nothing.
/// ```
/// // Control: the exported type itself is nameable outside the engine.
/// let none: Option<safe_cex_engine::StampFloor> = None;
/// assert!(none.is_none());
/// ```
/// ```compile_fail
/// let forged = safe_cex_engine::StampFloor(5.);
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StampFloor(f64);
impl StampFloor {
    pub(crate) fn millis(self) -> f64 {
        self.0
    }
}

/// Metadata only: neither delivery evidence nor authority to send or retry.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SigningRelease {
    receipt: SignReceipt,
    time: ReleaseTime,
    raw_expiry_ms: Option<f64>,
}

impl SigningRelease {
    pub fn receipt(&self) -> SignReceipt {
        self.receipt
    }
    pub fn time(&self) -> ReleaseTime {
        self.time
    }
    /// The floor for a resend of this release, when its receipt carried a stamp.
    pub fn stamp_floor(&self) -> Option<StampFloor> {
        self.receipt.stamp_ms.map(StampFloor)
    }
    /// Uncapped signature expiry in raw time, independent of the HTTP deadline.
    pub fn raw_expiry_ms(&self) -> Option<f64> {
        self.raw_expiry_ms
    }

    pub(crate) fn new(receipt: SignReceipt, time: ReleaseTime) -> Result<Self, Error> {
        if !time.raw_ms.is_finite() || !time.protocol_ms.is_finite() {
            return Err(Error::Invalid);
        }
        receipt.validate().map_err(|_| Error::Invalid)?;
        let raw_expiry_ms = if let Some(expiry) = receipt.expiry_ms {
            let remaining = expiry - time.protocol_ms;
            let projected = time.raw_ms + remaining;
            if !remaining.is_finite() || !projected.is_finite() {
                return Err(Error::Invalid);
            }
            if remaining <= 0. {
                return Err(Error::Stale);
            }
            if projected <= time.raw_ms {
                return Err(Error::Invalid);
            }
            Some(projected)
        } else {
            None
        };
        Ok(Self {
            receipt,
            time,
            raw_expiry_ms,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(raw_ms: f64, protocol_ms: f64) -> ReleaseTime {
        ReleaseTime {
            raw_ms,
            protocol_ms,
            revision: 7,
        }
    }
    fn receipt(expiry_ms: f64) -> SignReceipt {
        SignReceipt {
            nonce: None,
            stamp_ms: None,
            expiry_ms: Some(expiry_ms),
        }
    }
    #[test]
    fn projection_refuses_invalid_clocks_arithmetic_and_expired_signatures() {
        for time in [at(f64::NAN, 1.), at(1., f64::INFINITY)] {
            assert_eq!(
                SigningRelease::new(SignReceipt::default(), time),
                Err(Error::Invalid)
            );
        }
        assert_eq!(
            SigningRelease::new(receipt(f64::INFINITY), at(1., 0.)),
            Err(Error::Invalid)
        );
        assert_eq!(
            SigningRelease::new(receipt(f64::MAX), at(f64::MAX, 0.)),
            Err(Error::Invalid)
        );
        // Valid receipt, finite remaining time, overflow only in raw addition.
        assert_eq!(
            SigningRelease::new(receipt(1.), at(f64::MAX, -f64::MAX)),
            Err(Error::Invalid)
        );
        assert_eq!(
            SigningRelease::new(receipt(1.), at(f64::MAX, 0.)),
            Err(Error::Invalid)
        );
        for expiry in [9., 10.] {
            assert_eq!(
                SigningRelease::new(receipt(expiry), at(1., 10.)),
                Err(Error::Stale)
            );
        }
        let valid = SigningRelease::new(receipt(60_000.), at(1_000., 5_000.)).unwrap();
        assert_eq!(valid.raw_expiry_ms(), Some(56_000.));
        assert_eq!(valid.time().revision, 7);
        assert_eq!(
            SigningRelease::new(SignReceipt::default(), at(1., 2.))
                .unwrap()
                .raw_expiry_ms(),
            None
        );
    }

    // Receipt-bound stamp floor (RUST_STAMP_FLOOR.md). REDs: StampFloor,
    // SigningRelease::stamp_floor and DispatchContext::stamp_after are new.
    fn stamped(stamp_ms: Option<f64>) -> SigningRelease {
        SigningRelease::new(
            SignReceipt {
                nonce: None,
                stamp_ms,
                expiry_ms: None,
            },
            at(0., 0.),
        )
        .unwrap()
    }
    fn floor(stamp_ms: f64) -> Option<StampFloor> {
        stamped(Some(stamp_ms)).stamp_floor()
    }
    fn dispatch(now_ms: f64) -> crate::DispatchContext<'static> {
        crate::DispatchContext::new(now_ms, None)
    }

    /// R1: without a floor, the current stamp, never negative.
    #[test]
    fn stamp_after_without_a_floor_is_the_nonnegative_current_stamp() {
        assert_eq!(dispatch(1_000.75).stamp_after(None), Ok(1_000.));
        assert_eq!(dispatch(-1.5).stamp_after(None), Ok(0.));
    }

    /// R2: a floor holds against fixed and backward clocks, and the clock
    /// still wins once it passes the floor.
    #[test]
    fn stamp_after_a_floor_is_strictly_later_on_fixed_and_backward_clocks() {
        let floor = floor(1_000.);
        assert_eq!(dispatch(1_000.4).stamp_after(floor), Ok(1_001.));
        assert_eq!(dispatch(900.).stamp_after(floor), Ok(1_001.));
        assert_eq!(dispatch(1_500.).stamp_after(floor), Ok(1_500.));
        assert_eq!(dispatch(-1.5).stamp_after(self::floor(5.)), Ok(6.));
    }

    /// R3: floor + 1 must stay a safe integer; it is refused, never rounded.
    #[test]
    fn stamp_after_refuses_past_the_safe_integer_edge() {
        const MAX: f64 = 9_007_199_254_740_991.;
        assert_eq!(
            dispatch(0.).stamp_after(floor(MAX)),
            Err(crate::signing::SignError::InvalidRequest)
        );
        assert_eq!(dispatch(0.).stamp_after(floor(MAX - 1.)), Ok(MAX));
    }

    /// R4: a floor exists exactly when the released receipt carried a stamp.
    #[test]
    fn stamp_floor_comes_only_from_a_released_stamp() {
        assert!(stamped(None).stamp_floor().is_none());
        assert_eq!(dispatch(0.).stamp_after(floor(42.)), Ok(43.));
    }

    #[test]
    fn stamp_after_refuses_an_unsafe_current_clock_with_or_without_floor() {
        const MAX: f64 = 9_007_199_254_740_991.;
        for now in [MAX + 1., MAX + 2., f64::MAX] {
            for selected_floor in [None, floor(42.)] {
                assert_eq!(
                    dispatch(now).stamp_after(selected_floor),
                    Err(crate::signing::SignError::InvalidRequest)
                );
            }
        }
    }

    #[test]
    fn stamp_after_refuses_nonfinite_current_clocks_before_clamping() {
        for now in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            for selected_floor in [None, floor(42.)] {
                assert_eq!(
                    dispatch(now).stamp_after(selected_floor),
                    Err(crate::signing::SignError::InvalidRequest)
                );
            }
        }
    }

    #[test]
    fn stamp_after_keeps_the_maximum_safe_current_clock() {
        const MAX: f64 = 9_007_199_254_740_991.;
        for selected_floor in [None, floor(42.), floor(MAX - 1.)] {
            assert_eq!(dispatch(MAX).stamp_after(selected_floor), Ok(MAX));
        }
    }

    #[test]
    fn stamp_ms_is_unchanged_by_the_opt_in_floor_helper() {
        let negative = dispatch(-1.5);
        assert_eq!(negative.stamp_ms(), -2.);
        assert_eq!(negative.stamp_after(None), Ok(0.));
        assert_eq!(negative.stamp_ms(), -2.);
        let current = dispatch(1_000.75);
        assert_eq!(current.stamp_ms(), 1_000.);
        assert_eq!(current.stamp_after(floor(1_000.)), Ok(1_001.));
        assert_eq!(current.stamp_ms(), 1_000.);
    }
}
