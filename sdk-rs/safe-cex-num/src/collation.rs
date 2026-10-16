use crate::JsCoercionError;
use icu_collator::{Collator, options::CollatorOptions};
use std::cmp::Ordering;

/// Root-locale, nonnumeric, case-sensitive collation for signed query keys.
/// CHOSEN profile: ICU4X 2.3.1 with generated Unihan root data, measured against Node ICU 78.
/// Inputs are Unicode strings; lone UTF-16 surrogates are outside this API.
pub struct JsRootCollator(Collator);

impl JsRootCollator {
    /// Load the pinned root data. Failure is returned, never replaced by byte order.
    pub fn try_new() -> Result<Self, JsCoercionError> {
        Collator::try_new_unstable(
            &RootData,
            icu_locale_core::Locale::UNKNOWN.into(),
            CollatorOptions::default(),
        )
        .map(Self)
        .map_err(|_| JsCoercionError {
            name: "Error",
            message: "Pinned root collation data unavailable",
        })
    }

    /// Compare using root collation. Use a stable sort to retain equal-key order.
    pub fn compare(&self, left: &str, right: &str) -> Ordering {
        self.0.as_borrowed().compare(left, right)
    }
}

// Root and auxiliary collation tables share indices: never mix Han profiles.
#[allow(dead_code, clippy::redundant_static_lifetimes)]
#[rustfmt::skip]
#[path = "collation_data/provider.rs"]
mod data;
use data::RootData;
impl icu_provider::DataProvider<icu_normalizer::provider::NormalizerNfdDataV1> for RootData {
    fn load(
        &self,
        request: icu_provider::DataRequest,
    ) -> Result<
        icu_provider::DataResponse<icu_normalizer::provider::NormalizerNfdDataV1>,
        icu_provider::DataError,
    > {
        icu_provider::DataProvider::<icu_normalizer::provider::NormalizerNfdDataV1>::load(
            &icu_normalizer::provider::Baked,
            request,
        )
    }
}
impl icu_provider::DataProvider<icu_normalizer::provider::NormalizerNfdTablesV1> for RootData {
    fn load(
        &self,
        request: icu_provider::DataRequest,
    ) -> Result<
        icu_provider::DataResponse<icu_normalizer::provider::NormalizerNfdTablesV1>,
        icu_provider::DataError,
    > {
        icu_provider::DataProvider::<icu_normalizer::provider::NormalizerNfdTablesV1>::load(
            &icu_normalizer::provider::Baked,
            request,
        )
    }
}
