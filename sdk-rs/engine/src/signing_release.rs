//! Engine clock metadata observed only at a successful final signing release.
use crate::request_context::{Error, ReleaseTime};
use crate::signing::SignReceipt;

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
}
