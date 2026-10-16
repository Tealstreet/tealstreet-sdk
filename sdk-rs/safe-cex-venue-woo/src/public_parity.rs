//! Transport-free public parity diagnostics. No transport or store authority.
use safe_cex_num::JsJsonValue;
use serde::Serialize;
use std::rc::Rc;

// ts: WooV3PublicParityRouteId
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
pub enum WooV3PublicParityRouteId {
    #[serde(rename = "markets")]
    Markets,
    #[serde(rename = "tickers")]
    Tickers,
    #[serde(rename = "orderbook")]
    Orderbook,
    #[serde(rename = "recent-klines")]
    RecentKlines,
    #[serde(rename = "historical-klines")]
    HistoricalKlines,
}
impl<'de> serde::Deserialize<'de> for WooV3PublicParityRouteId {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = <String as serde::Deserialize>::deserialize(deserializer)?;
        #[derive(serde::Deserialize)]
        #[serde(remote = "WooV3PublicParityRouteId")]
        enum Wire {
            #[serde(rename = "markets")]
            Markets,
            #[serde(rename = "tickers")]
            Tickers,
            #[serde(rename = "orderbook")]
            Orderbook,
            #[serde(rename = "recent-klines")]
            RecentKlines,
            #[serde(rename = "historical-klines")]
            HistoricalKlines,
        }
        Wire::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(value))
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ParityMismatchKind {
    Value,
    MissingInV3,
    UnexpectedInV3,
    DuplicateKey,
}
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ParityBlockerCode {
    NoLiveQuotes,
    InvalidRequestContext,
    RequestSymbolMismatch,
    RequestIntervalMismatch,
    UnsupportedKlineInterval,
    KlineRowSymbolMismatch,
    KlineRowIntervalMismatch,
    KlineRowEndTimestampMismatch,
}
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ParityStatus {
    Match,
    Mismatch,
    Blocked,
}

/// Native primitive data and immutable compound handles. Clones preserve object identity.
/// Executable/prototype/symbol values and live compound mutation are outside this adapter.
#[derive(Debug, Clone)]
pub struct ParityValue(Rc<JsJsonValue>);
impl ParityValue {
    pub fn new(value: JsJsonValue) -> Self {
        Self(Rc::new(value))
    }
    pub fn value(&self) -> &JsJsonValue {
        &self.0
    }
    pub fn undefined() -> Self {
        Self::new(JsJsonValue::Undefined)
    }
    pub fn string(value: impl Into<String>) -> Self {
        Self::new(JsJsonValue::String(value.into()))
    }
    pub fn object_is(&self, other: &Self) -> bool {
        use JsJsonValue::*;
        match (self.value(), other.value()) {
            (Undefined, Undefined) | (Null, Null) => true,
            (Bool(a), Bool(b)) => a == b,
            (Number(a), Number(b)) => {
                (a.is_nan() && b.is_nan())
                    || (a == b && (*a != 0.0 || a.is_sign_negative() == b.is_sign_negative()))
            }
            (String(a), String(b)) => a == b,
            (BigInt(a), BigInt(b)) => a == b,
            (Array(_), Array(_)) | (Object(_), Object(_)) => Rc::ptr_eq(&self.0, &other.0),
            _ => false,
        }
    }
}

#[derive(Debug)]
pub struct ParityProvenance {
    legacy: String,
    v3: String,
}
impl ParityProvenance {
    pub fn legacy(&self) -> &str {
        &self.legacy
    }
    pub fn v3(&self) -> &str {
        &self.v3
    }
}
// ts: WooV3PublicParityMismatch
#[derive(Debug)]
pub struct WooV3PublicParityMismatch {
    kind: ParityMismatchKind,
    path: String,
    legacy_value: ParityValue,
    v3_value: ParityValue,
    provenance: ParityProvenance,
}
impl WooV3PublicParityMismatch {
    pub fn kind(&self) -> ParityMismatchKind {
        self.kind
    }
    pub fn path(&self) -> &str {
        &self.path
    }
    pub fn legacy_value(&self) -> &ParityValue {
        &self.legacy_value
    }
    pub fn v3_value(&self) -> &ParityValue {
        &self.v3_value
    }
    pub fn provenance(&self) -> &ParityProvenance {
        &self.provenance
    }
}
// ts: mismatch
pub fn mismatch(
    kind: ParityMismatchKind,
    path: impl Into<String>,
    legacy_value: ParityValue,
    v3_value: ParityValue,
    legacy: &str,
    v3: &str,
) -> Rc<WooV3PublicParityMismatch> {
    Rc::new(WooV3PublicParityMismatch {
        kind,
        path: path.into(),
        legacy_value,
        v3_value,
        provenance: ParityProvenance {
            legacy: legacy.into(),
            v3: v3.into(),
        },
    })
}

/// None = absent property; Some(None) = present undefined path.
#[derive(Debug, Default)]
pub struct ParityBlockerContext {
    pub path: Option<Option<String>>,
    pub expected: Option<ParityValue>,
    pub actual: Option<ParityValue>,
}
// ts: WooV3PublicParityBlocker
#[derive(Debug)]
pub struct WooV3PublicParityBlocker {
    code: ParityBlockerCode,
    detail: String,
    context: ParityBlockerContext,
}
impl WooV3PublicParityBlocker {
    pub fn code(&self) -> ParityBlockerCode {
        self.code
    }
    pub fn detail(&self) -> &str {
        &self.detail
    }
    pub fn path(&self) -> Option<Option<&str>> {
        self.context.path.as_ref().map(|v| v.as_deref())
    }
    pub fn expected(&self) -> Option<&ParityValue> {
        self.context.expected.as_ref()
    }
    pub fn actual(&self) -> Option<&ParityValue> {
        self.context.actual.as_ref()
    }
}
// ts: blocker
pub fn blocker(
    code: ParityBlockerCode,
    detail: impl Into<String>,
    context: ParityBlockerContext,
) -> Rc<WooV3PublicParityBlocker> {
    Rc::new(WooV3PublicParityBlocker {
        code,
        detail: detail.into(),
        context,
    })
}

/// Sparse field-array holes are represented by None in the outer slice.
/// Fallible host accessors preserve the caller's error E unchanged.
pub struct FieldComparison<'a, L, V, E> {
    pub path: &'a str,
    pub legacy: &'a dyn Fn(&L) -> Result<ParityValue, E>,
    pub v3: &'a dyn Fn(&V) -> Result<ParityValue, E>,
    pub legacy_provenance: &'a str,
    pub v3_provenance: &'a str,
}
// ts: compareFields
pub fn compare_fields<L, V, E>(
    legacy: &L,
    v3: &V,
    prefix: &str,
    fields: &[Option<FieldComparison<'_, L, V, E>>],
) -> Result<Vec<Rc<WooV3PublicParityMismatch>>, E> {
    let mut mismatches = Vec::new();
    for field in fields.iter().flatten() {
        let legacy_value = (field.legacy)(legacy)?;
        let v3_value = (field.v3)(v3)?;
        if !legacy_value.object_is(&v3_value) {
            mismatches.push(mismatch(
                ParityMismatchKind::Value,
                format!("{prefix}.{}", field.path),
                legacy_value,
                v3_value,
                field.legacy_provenance,
                field.v3_provenance,
            ));
        }
    }
    Ok(mismatches)
}

/// Option<T> distinguishes JS undefined elements from present T (including null).
/// Slices are fixed iteration inputs, not the source's live for-of arrays.
pub struct KeyedComparison<'a, L, V, E> {
    pub path: &'a str,
    pub legacy: &'a [Option<L>],
    pub v3: &'a [Option<V>],
    pub legacy_key: &'a dyn Fn(&Option<L>) -> Result<String, E>,
    pub v3_key: &'a dyn Fn(&Option<V>) -> Result<String, E>,
    pub fields: &'a [Option<FieldComparison<'a, Option<L>, Option<V>, E>>],
    pub legacy_provenance: &'a str,
    pub v3_provenance: &'a str,
}
// ts: compareKeyed
pub fn compare_keyed<L, V, E>(
    input: KeyedComparison<'_, L, V, E>,
) -> Result<Vec<Rc<WooV3PublicParityMismatch>>, E> {
    let mut mismatches = Vec::new();
    // Vec is intentional: JS Map insertion order includes integer-looking keys.
    let mut legacy_by_key: Vec<(String, &Option<L>)> = Vec::new();
    let mut v3_by_key: Vec<(String, &Option<V>)> = Vec::new();
    for value in input.legacy {
        let key = (input.legacy_key)(value)?;
        if legacy_by_key.iter().any(|(k, _)| *k == key) {
            mismatches.push(mismatch(
                ParityMismatchKind::DuplicateKey,
                format!("{}[{key}]", input.path),
                ParityValue::string(key),
                ParityValue::undefined(),
                input.legacy_provenance,
                input.v3_provenance,
            ));
        } else {
            legacy_by_key.push((key, value));
        }
    }
    for value in input.v3 {
        let key = (input.v3_key)(value)?;
        if v3_by_key.iter().any(|(k, _)| *k == key) {
            mismatches.push(mismatch(
                ParityMismatchKind::DuplicateKey,
                format!("{}[{key}]", input.path),
                ParityValue::undefined(),
                ParityValue::string(key),
                input.legacy_provenance,
                input.v3_provenance,
            ));
        } else {
            v3_by_key.push((key, value));
        }
    }
    for (key, legacy_value) in &legacy_by_key {
        // JS Map.get returns undefined both for missing keys and stored undefined.
        match v3_by_key.iter().find(|(k, _)| k == key) {
            Some((_, value)) if value.is_some() => mismatches.extend(compare_fields(
                *legacy_value,
                *value,
                &format!("{}[{key}]", input.path),
                input.fields,
            )?),
            _ => mismatches.push(mismatch(
                ParityMismatchKind::MissingInV3,
                format!("{}[{key}]", input.path),
                ParityValue::string(key),
                ParityValue::undefined(),
                input.legacy_provenance,
                input.v3_provenance,
            )),
        }
    }
    for (key, _) in &v3_by_key {
        if !legacy_by_key.iter().any(|(k, _)| k == key) {
            mismatches.push(mismatch(
                ParityMismatchKind::UnexpectedInV3,
                format!("{}[{key}]", input.path),
                ParityValue::undefined(),
                ParityValue::string(key),
                input.legacy_provenance,
                input.v3_provenance,
            ));
        }
    }
    Ok(mismatches)
}

// ts: WooV3PublicParityResult
#[derive(Debug)]
pub struct WooV3PublicParityResult<'a, L, V> {
    status: ParityStatus,
    route_id: WooV3PublicParityRouteId,
    authoritative: &'a L,
    observed: &'a V,
    legacy_canonical: String,
    v3_canonical: String,
    mismatches: Vec<Option<Rc<WooV3PublicParityMismatch>>>,
    blockers: Vec<Option<Rc<WooV3PublicParityBlocker>>>,
}
impl<L, V> WooV3PublicParityResult<'_, L, V> {
    pub fn status(&self) -> ParityStatus {
        self.status
    }
    pub fn route_id(&self) -> WooV3PublicParityRouteId {
        self.route_id
    }
    pub fn authoritative(&self) -> &L {
        self.authoritative
    }
    pub fn observed(&self) -> &V {
        self.observed
    }
    pub fn legacy_canonical(&self) -> &str {
        &self.legacy_canonical
    }
    pub fn v3_canonical(&self) -> &str {
        &self.v3_canonical
    }
    pub fn mismatches(&self) -> &[Option<Rc<WooV3PublicParityMismatch>>] {
        &self.mismatches
    }
    pub fn blockers(&self) -> &[Option<Rc<WooV3PublicParityBlocker>>] {
        &self.blockers
    }
    pub fn store_action(&self) -> &'static str {
        "retain-legacy"
    }
}
// ts: result
#[allow(clippy::too_many_arguments)]
pub fn result<'a, L, V>(
    route_id: WooV3PublicParityRouteId,
    authoritative: &'a L,
    observed: &'a V,
    legacy_canonical: &str,
    v3_canonical: &str,
    mismatches: &[Option<Rc<WooV3PublicParityMismatch>>],
    blockers: Option<&[Option<Rc<WooV3PublicParityBlocker>>]>,
) -> WooV3PublicParityResult<'a, L, V> {
    let blockers = blockers.unwrap_or_default().to_vec();
    let mismatches = mismatches.to_vec();
    let status = if !blockers.is_empty() {
        ParityStatus::Blocked
    } else if !mismatches.is_empty() {
        ParityStatus::Mismatch
    } else {
        ParityStatus::Match
    };
    WooV3PublicParityResult {
        status,
        route_id,
        authoritative,
        observed,
        legacy_canonical: legacy_canonical.into(),
        v3_canonical: v3_canonical.into(),
        mismatches,
        blockers,
    }
}
