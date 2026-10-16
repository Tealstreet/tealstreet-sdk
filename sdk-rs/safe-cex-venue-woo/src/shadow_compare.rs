use crate::runtime_selector::WooV3MigrationRouteId;
use std::rc::Rc;

pub struct WooV3ObservedCanonical<V> {
    canonical: Vec<u16>,
    observed: V,
}
impl<V> WooV3ObservedCanonical<V> {
    pub fn new(canonical: Vec<u16>, observed: V) -> Self {
        Self {
            canonical,
            observed,
        }
    }
    pub fn canonical(&self) -> &[u16] {
        &self.canonical
    }
    pub fn observed(&self) -> &V {
        &self.observed
    }
}
pub struct WooV3ComparedRead<L, O> {
    matched: bool,
    route_id: WooV3MigrationRouteId,
    authoritative: L,
    observed_canonical: Vec<u16>,
    observed: O,
}
impl<L, O> WooV3ComparedRead<L, O> {
    pub fn new(
        matched: bool,
        route_id: WooV3MigrationRouteId,
        authoritative: L,
        observed_canonical: Vec<u16>,
        observed: O,
    ) -> Self {
        Self {
            matched,
            route_id,
            authoritative,
            observed_canonical,
            observed,
        }
    }
    pub fn status(&self) -> &'static str {
        if self.matched { "match" } else { "mismatch" }
    }
    pub fn route_id(&self) -> WooV3MigrationRouteId {
        self.route_id
    }
    pub fn authoritative(&self) -> &L {
        &self.authoritative
    }
    pub fn observed_canonical(&self) -> &[u16] {
        &self.observed_canonical
    }
    pub fn observed(&self) -> &O {
        &self.observed
    }
    pub fn store_action(&self) -> &'static str {
        "retain-legacy"
    }
}
pub trait ShadowErrorReference {
    type Value;
    type Failure;
    fn native(&self) -> &Self::Value;
    fn name(&self) -> Result<Vec<u16>, Self::Failure>;
    fn message(&self) -> Result<Vec<u16>, Self::Failure>;
    fn stack(&self) -> Result<Option<Vec<u16>>, Self::Failure>;
    fn cause(&self) -> Result<Option<Self::Value>, Self::Failure>;
}
pub struct WooV3FailedRead<L, E: ShadowErrorReference> {
    route_id: WooV3MigrationRouteId,
    authoritative: L,
    error: E,
}
impl<L, E: ShadowErrorReference> WooV3FailedRead<L, E> {
    pub fn new(route_id: WooV3MigrationRouteId, authoritative: L, error: E) -> Self {
        Self {
            route_id,
            authoritative,
            error,
        }
    }
    pub fn status(&self) -> &'static str {
        "decode-or-map-failed"
    }
    pub fn route_id(&self) -> WooV3MigrationRouteId {
        self.route_id
    }
    pub fn authoritative(&self) -> &L {
        &self.authoritative
    }
    pub fn error(&self) -> &E {
        &self.error
    }
    pub fn store_action(&self) -> &'static str {
        "retain-legacy"
    }
}
pub enum WooV3ShadowReadComparison<L, O, E: ShadowErrorReference> {
    Compared(WooV3ComparedRead<L, O>),
    Failed(WooV3FailedRead<L, E>),
}
pub struct ShadowOps<V> {
    pub get: Rc<dyn Fn(&V, &str) -> Result<V, V>>,
    pub global: Rc<dyn Fn(&str) -> Result<V, V>>,
    pub call: Rc<dyn Fn(V, V, Vec<V>) -> Result<V, V>>,
    pub construct: Rc<dyn Fn(V, Vec<V>) -> Result<V, V>>,
    pub object: Rc<dyn Fn(Vec<(&'static str, V)>) -> V>,
    pub string: Rc<dyn Fn(&str) -> V>,
    pub undefined: Rc<dyn Fn() -> V>,
    pub kind: Rc<dyn Fn(&V) -> &'static str>,
    pub null: Rc<dyn Fn(&V) -> bool>,
    pub truthy: Rc<dyn Fn(&V) -> bool>,
    pub strict_equal: Rc<dyn Fn(&V, &V) -> bool>,
    pub instance_of: Rc<dyn Fn(&V, &V) -> Result<bool, V>>,
    pub template_append: Rc<dyn Fn(V, V) -> Result<V, V>>,
}
fn method<V>(
    h: &ShadowOps<V>,
    receiver: V,
    key: &str,
    args: impl FnOnce() -> Result<Vec<V>, V>,
) -> Result<V, V> {
    let function = (h.get)(&receiver, key)?;
    (h.call)(function, receiver, args()?)
}
pub fn normalize_error<V: Clone>(h: &ShadowOps<V>, error: V) -> Result<V, V> {
    if (h.instance_of)(&error, &(h.global)("Error")?)? {
        return Ok(error);
    }
    let constructor = (h.global)("Error")?;
    let function = (h.global)("String")?;
    let text = (h.call)(function, (h.undefined)(), vec![error])?;
    let message = (h.template_append)((h.string)("Unknown WOO V3 shadow error: "), text)?;
    (h.construct)(constructor, vec![message])
}
fn has_own<V>(h: &ShadowOps<V>, value: V, key: &str) -> Result<bool, V> {
    let object = (h.global)("Object")?;
    let prototype = (h.get)(&object, "prototype")?;
    let member = (h.get)(&prototype, "hasOwnProperty")?;
    let result = method(h, member, "call", || Ok(vec![value, (h.string)(key)]))?;
    Ok((h.truthy)(&result))
}
fn freeze<V>(h: &ShadowOps<V>, value: impl FnOnce() -> Result<V, V>) -> Result<V, V> {
    method(h, (h.global)("Object")?, "freeze", || Ok(vec![value()?]))
}
fn invalid_decoder<V>(h: &ShadowOps<V>) -> Result<V, V> {
    (h.construct)(
        (h.global)("Error")?,
        vec![(h.string)(
            "WOO V3 shadow decoder must return a canonical string and its observed value",
        )],
    )
}
pub fn compare_shadow_read<V: Clone>(h: &ShadowOps<V>, input: &V) -> Result<V, V> {
    let decoded = (|| {
        let decoder = (h.get)(input, "decodeAndCanonicalizeV3")?;
        let response = (h.get)(input, "v3Response")?;
        let decoded = (h.call)(decoder, input.clone(), vec![response])?;
        if (h.kind)(&decoded) != "object"
            || (h.null)(&decoded)
            || !has_own(h, decoded.clone(), "canonical")?
            || !has_own(h, decoded.clone(), "observed")?
        {
            return Err(invalid_decoder(h)?);
        }
        let canonical = (h.get)(&decoded, "canonical")?;
        let observed = (h.get)(&decoded, "observed")?;
        if (h.kind)(&canonical) != "string" {
            return Err(invalid_decoder(h)?);
        }
        Ok((canonical, observed))
    })();
    match decoded {
        Err(error) => freeze(h, || {
            let selection = (h.get)(input, "selection")?;
            let route = (h.get)(&selection, "routeId")?;
            let authoritative = (h.get)(input, "legacySnapshot")?;
            let error = normalize_error(h, error)?;
            Ok((h.object)(vec![
                ("status", (h.string)("decode-or-map-failed")),
                ("routeId", route),
                ("authoritative", authoritative),
                ("error", error),
                ("storeAction", (h.string)("retain-legacy")),
            ]))
        }),
        Ok((canonical, observed)) => freeze(h, || {
            let legacy = (h.get)(input, "legacyCanonical")?;
            let status = (h.string)(if (h.strict_equal)(&legacy, &canonical) {
                "match"
            } else {
                "mismatch"
            });
            let selection = (h.get)(input, "selection")?;
            let route = (h.get)(&selection, "routeId")?;
            let authoritative = (h.get)(input, "legacySnapshot")?;
            Ok((h.object)(vec![
                ("status", status),
                ("routeId", route),
                ("authoritative", authoritative),
                ("observedCanonical", canonical),
                ("observed", observed),
                ("storeAction", (h.string)("retain-legacy")),
            ]))
        }),
    }
}
