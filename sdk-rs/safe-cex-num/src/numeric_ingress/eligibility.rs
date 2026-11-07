//! Recursive eligibility, independent of Serde's serialization attributes.
use super::JsNumberInput;

/// Output whose complete field graph excludes lossy JSON visitor buffers.
///
/// Register records using `js_number_output!`, whose exhaustive destructuring
/// checks even skipped and renamed fields. Generic JSON payloads use
/// `JsNumberInput`, not `serde_json::Value`. Custom deserializers must also use
/// `JsNumberInput` for intermediate buffers. Handwritten implementations and
/// custom deserializers require an audit of their temporary buffers; use the
/// exhaustive macro for ordinary derived records. This contract cannot repair a loss
/// that happened before ingress or inside arbitrary user code.
pub trait JsNumberOutput {
    /// Recursively check the complete field graph, independent of serialization.
    fn check_number_output(&self);
}
impl JsNumberOutput for JsNumberInput {
    fn check_number_output(&self) {}
}
impl<A: JsNumberOutput, B: JsNumberOutput> JsNumberOutput for (A, B) {
    fn check_number_output(&self) {
        let (first, second) = self;
        first.check_number_output();
        second.check_number_output();
    }
}
impl JsNumberOutput for safe_cex_types::retained_json::RetainedJsonInput {
    fn check_number_output(&self) {}
}
impl JsNumberOutput for () {
    fn check_number_output(&self) {}
}
impl JsNumberOutput for bool {
    fn check_number_output(&self) {}
}
impl JsNumberOutput for char {
    fn check_number_output(&self) {}
}
impl JsNumberOutput for String {
    fn check_number_output(&self) {}
}
impl JsNumberOutput for str {
    fn check_number_output(&self) {}
}
impl<T: JsNumberOutput + ?Sized> JsNumberOutput for &T {
    fn check_number_output(&self) {
        (**self).check_number_output();
    }
}
impl JsNumberOutput for std::borrow::Cow<'_, str> {
    fn check_number_output(&self) {
        self.as_ref().check_number_output();
    }
}
impl JsNumberOutput for f64 {
    fn check_number_output(&self) {}
}
impl JsNumberOutput for f32 {
    fn check_number_output(&self) {}
}
impl JsNumberOutput for u8 {
    fn check_number_output(&self) {}
}
impl JsNumberOutput for u16 {
    fn check_number_output(&self) {}
}
impl JsNumberOutput for u32 {
    fn check_number_output(&self) {}
}
impl JsNumberOutput for u64 {
    fn check_number_output(&self) {}
}
impl JsNumberOutput for u128 {
    fn check_number_output(&self) {}
}
impl JsNumberOutput for usize {
    fn check_number_output(&self) {}
}
impl JsNumberOutput for i8 {
    fn check_number_output(&self) {}
}
impl JsNumberOutput for i16 {
    fn check_number_output(&self) {}
}
impl JsNumberOutput for i32 {
    fn check_number_output(&self) {}
}
impl JsNumberOutput for i64 {
    fn check_number_output(&self) {}
}
impl JsNumberOutput for i128 {
    fn check_number_output(&self) {}
}
impl JsNumberOutput for isize {
    fn check_number_output(&self) {}
}
impl<T: JsNumberOutput> JsNumberOutput for Option<T> {
    fn check_number_output(&self) {
        if let Some(value) = self {
            value.check_number_output();
        }
    }
}
impl<T: JsNumberOutput> JsNumberOutput for Vec<T> {
    fn check_number_output(&self) {
        for value in self {
            value.check_number_output();
        }
    }
}
impl<T: JsNumberOutput, const N: usize> JsNumberOutput for [T; N] {
    fn check_number_output(&self) {
        for value in self {
            value.check_number_output();
        }
    }
}
impl<T: JsNumberOutput + ?Sized> JsNumberOutput for Box<T> {
    fn check_number_output(&self) {
        (**self).check_number_output();
    }
}
impl<T: JsNumberOutput> JsNumberOutput for std::collections::BTreeMap<String, T> {
    fn check_number_output(&self) {
        for value in self.values() {
            value.check_number_output();
        }
    }
}
impl<T: JsNumberOutput, S> JsNumberOutput for std::collections::HashMap<String, T, S> {
    fn check_number_output(&self) {
        for value in self.values() {
            value.check_number_output();
        }
    }
}

// These read-only traversals never alter dictionary ordering or membership.
impl<T: JsNumberOutput> JsNumberOutput for safe_cex_types::object_map::JsObjectMap<T> {
    fn check_number_output(&self) {
        for value in self.values() {
            value.check_number_output();
        }
    }
}
impl<K: JsNumberOutput, T: JsNumberOutput, S> JsNumberOutput for indexmap::IndexMap<K, T, S> {
    fn check_number_output(&self) {
        for (key, value) in self {
            key.check_number_output();
            value.check_number_output();
        }
    }
}
impl<T: JsNumberOutput> JsNumberOutput for [T] {
    fn check_number_output(&self) {
        for value in self {
            value.check_number_output();
        }
    }
}
impl<A: JsNumberOutput> JsNumberOutput for (A,) {
    fn check_number_output(&self) {
        let (a,) = self;
        a.check_number_output();
    }
}
impl<A: JsNumberOutput, B: JsNumberOutput, C: JsNumberOutput> JsNumberOutput for (A, B, C) {
    fn check_number_output(&self) {
        let (a, b, c) = self;
        a.check_number_output();
        b.check_number_output();
        c.check_number_output();
    }
}
impl<A: JsNumberOutput, B: JsNumberOutput, C: JsNumberOutput, D: JsNumberOutput> JsNumberOutput
    for (A, B, C, D)
{
    fn check_number_output(&self) {
        let (a, b, c, d) = self;
        a.check_number_output();
        b.check_number_output();
        c.check_number_output();
        d.check_number_output();
    }
}
impl<A: JsNumberOutput, B: JsNumberOutput, C: JsNumberOutput, D: JsNumberOutput, E: JsNumberOutput>
    JsNumberOutput for (A, B, C, D, E)
{
    fn check_number_output(&self) {
        let (a, b, c, d, e) = self;
        a.check_number_output();
        b.check_number_output();
        c.check_number_output();
        d.check_number_output();
        e.check_number_output();
    }
}
impl<
    A: JsNumberOutput,
    B: JsNumberOutput,
    C: JsNumberOutput,
    D: JsNumberOutput,
    E: JsNumberOutput,
    F: JsNumberOutput,
> JsNumberOutput for (A, B, C, D, E, F)
{
    fn check_number_output(&self) {
        let (a, b, c, d, e, f) = self;
        a.check_number_output();
        b.check_number_output();
        c.check_number_output();
        d.check_number_output();
        e.check_number_output();
        f.check_number_output();
    }
}
impl<
    A: JsNumberOutput,
    B: JsNumberOutput,
    C: JsNumberOutput,
    D: JsNumberOutput,
    E: JsNumberOutput,
    F: JsNumberOutput,
    G: JsNumberOutput,
> JsNumberOutput for (A, B, C, D, E, F, G)
{
    fn check_number_output(&self) {
        let (a, b, c, d, e, f, g) = self;
        a.check_number_output();
        b.check_number_output();
        c.check_number_output();
        d.check_number_output();
        e.check_number_output();
        f.check_number_output();
        g.check_number_output();
    }
}
impl<
    A: JsNumberOutput,
    B: JsNumberOutput,
    C: JsNumberOutput,
    D: JsNumberOutput,
    E: JsNumberOutput,
    F: JsNumberOutput,
    G: JsNumberOutput,
    H: JsNumberOutput,
> JsNumberOutput for (A, B, C, D, E, F, G, H)
{
    fn check_number_output(&self) {
        let (a, b, c, d, e, f, g, h) = self;
        a.check_number_output();
        b.check_number_output();
        c.check_number_output();
        d.check_number_output();
        e.check_number_output();
        f.check_number_output();
        g.check_number_output();
        h.check_number_output();
    }
}
impl<
    A: JsNumberOutput,
    B: JsNumberOutput,
    C: JsNumberOutput,
    D: JsNumberOutput,
    E: JsNumberOutput,
    F: JsNumberOutput,
    G: JsNumberOutput,
    H: JsNumberOutput,
    I: JsNumberOutput,
> JsNumberOutput for (A, B, C, D, E, F, G, H, I)
{
    fn check_number_output(&self) {
        let (a, b, c, d, e, f, g, h, i) = self;
        a.check_number_output();
        b.check_number_output();
        c.check_number_output();
        d.check_number_output();
        e.check_number_output();
        f.check_number_output();
        g.check_number_output();
        h.check_number_output();
        i.check_number_output();
    }
}
impl<
    A: JsNumberOutput,
    B: JsNumberOutput,
    C: JsNumberOutput,
    D: JsNumberOutput,
    E: JsNumberOutput,
    F: JsNumberOutput,
    G: JsNumberOutput,
    H: JsNumberOutput,
    I: JsNumberOutput,
    J: JsNumberOutput,
> JsNumberOutput for (A, B, C, D, E, F, G, H, I, J)
{
    fn check_number_output(&self) {
        let (a, b, c, d, e, f, g, h, i, j) = self;
        a.check_number_output();
        b.check_number_output();
        c.check_number_output();
        d.check_number_output();
        e.check_number_output();
        f.check_number_output();
        g.check_number_output();
        h.check_number_output();
        i.check_number_output();
        j.check_number_output();
    }
}
impl<
    A: JsNumberOutput,
    B: JsNumberOutput,
    C: JsNumberOutput,
    D: JsNumberOutput,
    E: JsNumberOutput,
    F: JsNumberOutput,
    G: JsNumberOutput,
    H: JsNumberOutput,
    I: JsNumberOutput,
    J: JsNumberOutput,
    K: JsNumberOutput,
> JsNumberOutput for (A, B, C, D, E, F, G, H, I, J, K)
{
    fn check_number_output(&self) {
        let (a, b, c, d, e, f, g, h, i, j, k) = self;
        a.check_number_output();
        b.check_number_output();
        c.check_number_output();
        d.check_number_output();
        e.check_number_output();
        f.check_number_output();
        g.check_number_output();
        h.check_number_output();
        i.check_number_output();
        j.check_number_output();
        k.check_number_output();
    }
}
impl<
    A: JsNumberOutput,
    B: JsNumberOutput,
    C: JsNumberOutput,
    D: JsNumberOutput,
    E: JsNumberOutput,
    F: JsNumberOutput,
    G: JsNumberOutput,
    H: JsNumberOutput,
    I: JsNumberOutput,
    J: JsNumberOutput,
    K: JsNumberOutput,
    L: JsNumberOutput,
> JsNumberOutput for (A, B, C, D, E, F, G, H, I, J, K, L)
{
    fn check_number_output(&self) {
        let (a, b, c, d, e, f, g, h, i, j, k, l) = self;
        a.check_number_output();
        b.check_number_output();
        c.check_number_output();
        d.check_number_output();
        e.check_number_output();
        f.check_number_output();
        g.check_number_output();
        h.check_number_output();
        i.check_number_output();
        j.check_number_output();
        k.check_number_output();
        l.check_number_output();
    }
}
impl<
    A: JsNumberOutput,
    B: JsNumberOutput,
    C: JsNumberOutput,
    D: JsNumberOutput,
    E: JsNumberOutput,
    F: JsNumberOutput,
    G: JsNumberOutput,
    H: JsNumberOutput,
    I: JsNumberOutput,
    J: JsNumberOutput,
    K: JsNumberOutput,
    L: JsNumberOutput,
    M: JsNumberOutput,
> JsNumberOutput for (A, B, C, D, E, F, G, H, I, J, K, L, M)
{
    fn check_number_output(&self) {
        let (a, b, c, d, e, f, g, h, i, j, k, l, m) = self;
        a.check_number_output();
        b.check_number_output();
        c.check_number_output();
        d.check_number_output();
        e.check_number_output();
        f.check_number_output();
        g.check_number_output();
        h.check_number_output();
        i.check_number_output();
        j.check_number_output();
        k.check_number_output();
        l.check_number_output();
        m.check_number_output();
    }
}
impl<
    A: JsNumberOutput,
    B: JsNumberOutput,
    C: JsNumberOutput,
    D: JsNumberOutput,
    E: JsNumberOutput,
    F: JsNumberOutput,
    G: JsNumberOutput,
    H: JsNumberOutput,
    I: JsNumberOutput,
    J: JsNumberOutput,
    K: JsNumberOutput,
    L: JsNumberOutput,
    M: JsNumberOutput,
    N: JsNumberOutput,
> JsNumberOutput for (A, B, C, D, E, F, G, H, I, J, K, L, M, N)
{
    fn check_number_output(&self) {
        let (a, b, c, d, e, f, g, h, i, j, k, l, m, n) = self;
        a.check_number_output();
        b.check_number_output();
        c.check_number_output();
        d.check_number_output();
        e.check_number_output();
        f.check_number_output();
        g.check_number_output();
        h.check_number_output();
        i.check_number_output();
        j.check_number_output();
        k.check_number_output();
        l.check_number_output();
        m.check_number_output();
        n.check_number_output();
    }
}
impl<
    A: JsNumberOutput,
    B: JsNumberOutput,
    C: JsNumberOutput,
    D: JsNumberOutput,
    E: JsNumberOutput,
    F: JsNumberOutput,
    G: JsNumberOutput,
    H: JsNumberOutput,
    I: JsNumberOutput,
    J: JsNumberOutput,
    K: JsNumberOutput,
    L: JsNumberOutput,
    M: JsNumberOutput,
    N: JsNumberOutput,
    O: JsNumberOutput,
> JsNumberOutput for (A, B, C, D, E, F, G, H, I, J, K, L, M, N, O)
{
    fn check_number_output(&self) {
        let (a, b, c, d, e, f, g, h, i, j, k, l, m, n, o) = self;
        a.check_number_output();
        b.check_number_output();
        c.check_number_output();
        d.check_number_output();
        e.check_number_output();
        f.check_number_output();
        g.check_number_output();
        h.check_number_output();
        i.check_number_output();
        j.check_number_output();
        k.check_number_output();
        l.check_number_output();
        m.check_number_output();
        n.check_number_output();
        o.check_number_output();
    }
}
impl<
    A: JsNumberOutput,
    B: JsNumberOutput,
    C: JsNumberOutput,
    D: JsNumberOutput,
    E: JsNumberOutput,
    F: JsNumberOutput,
    G: JsNumberOutput,
    H: JsNumberOutput,
    I: JsNumberOutput,
    J: JsNumberOutput,
    K: JsNumberOutput,
    L: JsNumberOutput,
    M: JsNumberOutput,
    N: JsNumberOutput,
    O: JsNumberOutput,
    P: JsNumberOutput,
> JsNumberOutput for (A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P)
{
    fn check_number_output(&self) {
        let (a, b, c, d, e, f, g, h, i, j, k, l, m, n, o, p) = self;
        a.check_number_output();
        b.check_number_output();
        c.check_number_output();
        d.check_number_output();
        e.check_number_output();
        f.check_number_output();
        g.check_number_output();
        h.check_number_output();
        i.check_number_output();
        j.check_number_output();
        k.check_number_output();
        l.check_number_output();
        m.check_number_output();
        n.check_number_output();
        o.check_number_output();
        p.check_number_output();
    }
}

/// Register every field inside JsNumberOutput::check_number_output. This is an
/// expression macro, so production declaration indexing needs no expansion. Adding a field without
/// registering it fails exhaustive destructuring; Value fields fail the bound,
/// including fields hidden by skip_serializing or asymmetric Serde renames.
#[macro_export]
macro_rules! js_number_output {
    ($value:expr, struct { $($field:ident),* $(,)? } passthrough { $($opaque:ident),+ $(,)? }) => {{
        let Self { $($field,)* $($opaque),+ } = $value;
        $($crate::numeric_ingress::JsNumberOutput::check_number_output($field);)*
        $($crate::numeric_ingress::JsNumberPassthrough::check_number_passthrough($opaque);)+
    }};
    ($value:expr, struct { $($field:ident),* $(,)? }) => {{
        let Self { $($field),* } = $value;
        $($crate::numeric_ingress::JsNumberOutput::check_number_output($field);)*
    }};
    ($value:expr, tuple($($field:ident),* $(,)?)) => {{
        let Self($($field),*) = $value;
        $($crate::numeric_ingress::JsNumberOutput::check_number_output($field);)*
    }};
    ($value:expr, enum { $($variant:ident $(($($tuple:ident),*))? $({$($field:ident),*})?),* $(,)? }) => {{
        match $value { $(Self::$variant $(($($tuple),*))? $({$($field),*})? => {
            $($($crate::numeric_ingress::JsNumberOutput::check_number_output($tuple);)*)?
            $($($crate::numeric_ingress::JsNumberOutput::check_number_output($field);)*)?
        }),* }
    }};
}
