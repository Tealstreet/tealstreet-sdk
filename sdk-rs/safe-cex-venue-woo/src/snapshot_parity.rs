use crate::public_canonical::{self as canonical, CanonicalModule, CanonicalOps};
use std::rc::Rc;

pub struct SnapshotPrimitives<V, E> {
    pub template: Rc<dyn Fn(&[&str], Vec<V>) -> Result<V, E>>,
    pub construct: Rc<dyn Fn(V, Vec<V>) -> Result<V, E>>,
    pub throw_value: Rc<dyn Fn(V) -> E>,
    pub is_nullish: Rc<dyn Fn(&V) -> bool>,
    pub is_undefined: Rc<dyn Fn(&V) -> bool>,
    pub less_than: Rc<dyn Fn(V, V) -> Result<bool, E>>,
    pub add: Rc<dyn Fn(V, V) -> Result<V, E>>,
    pub divide: Rc<dyn Fn(V, V) -> Result<V, E>>,
    pub iterate: Rc<dyn Fn(V, &mut dyn FnMut(V) -> Result<(), E>) -> Result<(), E>>,
    pub binding: Rc<dyn Fn(V, usize) -> Result<Vec<V>, E>>,
}
pub struct SnapshotParityOps<V, E> {
    pub canonical: CanonicalModule<V, E>,
    language: SnapshotPrimitives<V, E>,
    fields: [V; 3],
}
impl<V, E> std::ops::Deref for SnapshotParityOps<V, E> {
    type Target = SnapshotPrimitives<V, E>;
    fn deref(&self) -> &Self::Target {
        &self.language
    }
}
impl<V: Clone + 'static, E: 'static> SnapshotParityOps<V, E> {
    pub fn new(
        value: Rc<CanonicalOps<V, E>>,
        language: SnapshotPrimitives<V, E>,
    ) -> Result<Rc<Self>, E> {
        let canonical = CanonicalModule::bindings(value);
        let undefined = (canonical.ops.undefined)();
        let mut h = Self {
            canonical,
            language,
            fields: [undefined.clone(), undefined.clone(), undefined],
        };
        h.fields[0] = market_fields(&h)?;
        h.fields[1] = ticker_fields(&h)?;
        h.canonical.freeze_intervals()?;
        h.fields[2] = candle_fields(&h)?;
        Ok(Rc::new(h))
    }
}

fn get<V, E>(h: &SnapshotParityOps<V, E>, value: &V, key: &str) -> Result<V, E> {
    (h.canonical.ops.get)(value, key)
}
fn path<V: Clone, E>(h: &SnapshotParityOps<V, E>, value: &V, keys: &[&str]) -> Result<V, E> {
    let mut value = value.clone();
    for key in keys {
        value = get(h, &value, key)?;
    }
    Ok(value)
}
fn method<V, E>(
    h: &SnapshotParityOps<V, E>,
    receiver: V,
    key: &str,
    args: impl FnOnce() -> Result<Vec<V>, E>,
) -> Result<V, E> {
    let callee = get(h, &receiver, key)?;
    (h.canonical.ops.invoke)(callee, receiver, args()?)
}
fn global<V, E>(h: &SnapshotParityOps<V, E>, name: &str) -> Result<V, E> {
    (h.canonical.ops.global)(name)
}
fn freeze<V, E>(h: &SnapshotParityOps<V, E>, value: impl FnOnce() -> Result<V, E>) -> Result<V, E> {
    method(h, global(h, "Object")?, "freeze", || Ok(vec![value()?]))
}
fn error<V, E>(
    h: &SnapshotParityOps<V, E>,
    message: impl FnOnce() -> Result<V, E>,
) -> Result<V, E> {
    let ctor = global(h, "Error")?;
    let value = (h.construct)(ctor, vec![message()?])?;
    Err((h.throw_value)(value))
}
fn argument<V: Clone, E>(h: &SnapshotParityOps<V, E>, args: &[V], index: usize) -> V {
    args.get(index)
        .cloned()
        .unwrap_or_else(|| (h.canonical.ops.undefined)())
}
fn push<V, E>(
    h: &SnapshotParityOps<V, E>,
    array: V,
    value: impl FnOnce() -> Result<V, E>,
) -> Result<(), E> {
    method(h, array, "push", || Ok(vec![value()?]))?;
    Ok(())
}
fn template<V, E>(h: &SnapshotParityOps<V, E>, parts: &[&str], values: Vec<V>) -> Result<V, E> {
    (h.template)(parts, values)
}
fn blocker<V, E>(
    h: &SnapshotParityOps<V, E>,
    code: &str,
    detail: V,
    context: Vec<(&'static str, V)>,
) -> Result<V, E> {
    freeze(h, || {
        let o = &h.canonical.ops;
        let mut fields = vec![("code", (o.string)(code)), ("detail", detail)];
        fields.extend(context);
        Ok((o.object)(fields))
    })
}
fn mismatch<V, E>(
    h: &SnapshotParityOps<V, E>,
    kind: &str,
    path: V,
    legacy: V,
    v3: V,
    legacy_provenance: V,
    v3_provenance: V,
) -> Result<V, E> {
    freeze(h, || {
        let o = &h.canonical.ops;
        Ok((o.object)(vec![
            ("kind", (o.string)(kind)),
            ("path", path),
            ("legacyValue", legacy),
            ("v3Value", v3),
            (
                "provenance",
                freeze(h, || {
                    Ok((o.object)(vec![
                        ("legacy", legacy_provenance),
                        ("v3", v3_provenance),
                    ]))
                })?,
            ),
        ]))
    })
}
pub fn unsupported_interval_blocker<V: Clone + 'static, E: 'static>(
    h: &SnapshotParityOps<V, E>,
    side: V,
    interval: V,
    normalized: &V,
) -> Result<V, E> {
    let o = &h.canonical.ops;
    let reason = get(h, normalized, "reason")?;
    let detail = if (o.strict_equal)(&reason, &(o.string)("variable-calendar-duration")) {
        template(
            h,
            &[
                "WOO ",
                " uses variable calendar boundaries; fixed endTimestamp parity is intentionally blocked",
            ],
            vec![get(h, normalized, "normalized")?],
        )?
    } else {
        template(
            h,
            &["Unsupported WOO kline interval ", ""],
            vec![interval.clone()],
        )?
    };
    let path = template(h, &["kline.request.", ".interval"], vec![side])?;
    let expected = method(h, global(h, "Object")?, "keys", || {
        Ok(vec![h.canonical.fixed_intervals()])
    })?;
    blocker(
        h,
        "unsupported-kline-interval",
        detail,
        vec![("path", path), ("expected", expected), ("actual", interval)],
    )
}
pub fn validate_kline_context<V: Clone + 'static, E: 'static>(
    h: &SnapshotParityOps<V, E>,
    input: &V,
) -> Result<V, E> {
    let o = &h.canonical.ops;
    let blockers = (o.array)(vec![]);
    let legacy = canonical::normalize_kline_interval(
        &h.canonical,
        path(h, input, &["legacyRequest", "interval"])?,
    )?;
    let v3 = canonical::normalize_kline_interval(
        &h.canonical,
        path(h, input, &["v3Request", "interval"])?,
    )?;
    if !(o.truthy)(&path(h, input, &["legacyRequest", "symbol"])?)
        || !(o.truthy)(&path(h, input, &["v3Request", "symbol"])?)
    {
        push(h, blockers.clone(), || {
            blocker(
                h,
                "invalid-request-context",
                (o.string)("Kline request symbols must be non-empty"),
                vec![
                    ("path", (o.string)("kline.request.symbol")),
                    ("expected", (o.string)("non-empty exact WOO symbol")),
                    (
                        "actual",
                        (o.object)(vec![
                            ("legacy", path(h, input, &["legacyRequest", "symbol"])?),
                            ("v3", path(h, input, &["v3Request", "symbol"])?),
                        ]),
                    ),
                ],
            )
        })?;
    } else if !(o.strict_equal)(
        &path(h, input, &["legacyRequest", "symbol"])?,
        &path(h, input, &["v3Request", "symbol"])?,
    ) {
        push(h, blockers.clone(), || {
            blocker(
                h,
                "request-symbol-mismatch",
                (o.string)("Legacy and V3 kline requests target different symbols"),
                vec![
                    ("path", (o.string)("kline.request.symbol")),
                    ("expected", path(h, input, &["legacyRequest", "symbol"])?),
                    ("actual", path(h, input, &["v3Request", "symbol"])?),
                ],
            )
        })?;
    }
    for (side, value) in [("legacy", &legacy), ("v3", &v3)] {
        if (o.strict_equal)(&get(h, value, "status")?, &(o.string)("blocked")) {
            push(h, blockers.clone(), || {
                unsupported_interval_blocker(
                    h,
                    (o.string)(side),
                    path(
                        h,
                        input,
                        &[
                            if side == "legacy" {
                                "legacyRequest"
                            } else {
                                "v3Request"
                            },
                            "interval",
                        ],
                    )?,
                    value,
                )
            })?;
        }
    }
    if (o.strict_equal)(&get(h, &legacy, "status")?, &(o.string)("ready"))
        && (o.strict_equal)(&get(h, &v3, "status")?, &(o.string)("ready"))
        && !(o.strict_equal)(&get(h, &legacy, "interval")?, &get(h, &v3, "interval")?)
    {
        push(h, blockers.clone(), || {
            blocker(
                h,
                "request-interval-mismatch",
                (o.string)("Legacy and V3 kline requests target different intervals"),
                vec![
                    ("path", (o.string)("kline.request.interval")),
                    ("expected", get(h, &legacy, "interval")?),
                    ("actual", get(h, &v3, "interval")?),
                ],
            )
        })?;
    }
    let entries = method(h, get(h, input, "v3Rows")?, "entries", || Ok(vec![]))?;
    (h.iterate)(entries, &mut |entry| {
        let pair = (h.binding)(entry, 2)?;
        let index = pair[0].clone();
        let row = pair[1].clone();
        if !(o.strict_equal)(
            &get(h, &row, "symbol")?,
            &path(h, input, &["v3Request", "symbol"])?,
        ) {
            push(h, blockers.clone(), || {
                blocker(
                    h,
                    "v3-kline-row-symbol-mismatch",
                    (o.string)("V3 kline row does not match its request symbol"),
                    vec![
                        (
                            "path",
                            template(h, &["kline.v3Rows[", "].symbol"], vec![index.clone()])?,
                        ),
                        ("expected", path(h, input, &["v3Request", "symbol"])?),
                        ("actual", get(h, &row, "symbol")?),
                    ],
                )
            })?;
        }
        if (o.strict_equal)(&get(h, &v3, "status")?, &(o.string)("ready"))
            && !(o.strict_equal)(&get(h, &row, "type")?, &get(h, &v3, "interval")?)
        {
            push(h, blockers.clone(), || {
                blocker(
                    h,
                    "v3-kline-row-interval-mismatch",
                    (o.string)("V3 kline row does not match its normalized request interval"),
                    vec![
                        (
                            "path",
                            template(h, &["kline.v3Rows[", "].type"], vec![index.clone()])?,
                        ),
                        ("expected", get(h, &v3, "interval")?),
                        ("actual", get(h, &row, "type")?),
                    ],
                )
            })?;
        }
        if (o.strict_equal)(&get(h, &v3, "status")?, &(o.string)("ready")) {
            let expected = (h.add)(get(h, &row, "startTimestamp")?, get(h, &v3, "durationMs")?)?;
            let safe = method(h, global(h, "Number")?, "isSafeInteger", || {
                Ok(vec![expected.clone()])
            })?;
            if !(o.truthy)(&safe) || !(o.strict_equal)(&get(h, &row, "endTimestamp")?, &expected) {
                push(h, blockers.clone(), || {
                    blocker(
                        h,
                        "v3-kline-row-end-timestamp-mismatch",
                        (o.string)(
                            "V3 kline endTimestamp must equal startTimestamp plus the fixed requested interval",
                        ),
                        vec![
                            (
                                "path",
                                template(h, &["kline.v3Rows[", "].endTimestamp"], vec![index])?,
                            ),
                            ("expected", expected),
                            ("actual", get(h, &row, "endTimestamp")?),
                        ],
                    )
                })?;
            }
        }
        Ok(())
    })?;
    freeze(h, || {
        Ok((o.object)(vec![
            ("blockers", freeze(h, || Ok(blockers))?),
            ("legacyInterval", legacy),
            ("v3Interval", v3),
        ]))
    })
}
fn require_finite_number<V: Clone, E>(
    h: &SnapshotParityOps<V, E>,
    value: V,
    path: V,
) -> Result<V, E> {
    let o = &h.canonical.ops;
    let function = global(h, "Number")?;
    let result = (o.invoke)(function, (o.undefined)(), vec![value])?;
    let finite = method(h, global(h, "Number")?, "isFinite", || {
        Ok(vec![result.clone()])
    })?;
    if !(o.truthy)(&finite) || (h.less_than)(result.clone(), (o.number)(0.))? {
        return error(h, || {
            template(
                h,
                &["", " must be at least ", ""],
                vec![path, (o.number)(0.)],
            )
        });
    }
    Ok(result)
}
pub fn map_v3_candles<V: Clone + 'static, E: 'static>(
    h: Rc<SnapshotParityOps<V, E>>,
    rows: V,
) -> Result<V, E> {
    let o = &h.canonical.ops;
    let ops = h.clone();
    let mapped = method(&h, rows, "map", || {
        Ok(vec![(o.function)(
            "",
            2,
            Rc::new(move |args| {
                let row = argument(&ops, &args, 0);
                let index = argument(&ops, &args, 1);
                let o = &ops.canonical.ops;
                let path = template(&ops, &["WOO V3 kline[", "]"], vec![index])?;
                let timestamp =
                    (ops.divide)(get(&ops, &row, "startTimestamp")?, (o.number)(1000.))?;
                let mut fields = vec![("timestamp", timestamp)];
                for field in ["open", "high", "low", "close", "volume"] {
                    let value = get(&ops, &row, field)?;
                    let field_path =
                        template(&ops, &["", ".", ""], vec![path.clone(), (o.string)(field)])?;
                    fields.push((field, require_finite_number(&ops, value, field_path)?));
                }
                Ok((o.object)(fields))
            }),
        )])
    })?;
    let ops = h.clone();
    method(&h, mapped, "sort", || {
        Ok(vec![(o.function)(
            "",
            2,
            Rc::new(move |args| {
                let a = get(&ops, &argument(&ops, &args, 0), "timestamp")?;
                let b = get(&ops, &argument(&ops, &args, 1), "timestamp")?;
                (ops.canonical.ops.subtract)(a, b)
            }),
        )])
    })
}
pub fn canonicalize_v3_kline_rows<V: Clone + 'static, E: 'static>(
    h: Rc<SnapshotParityOps<V, E>>,
    request: &V,
    rows: V,
) -> Result<V, E> {
    let o = &h.canonical.ops;
    let validation = validate_kline_context(
        &h,
        &(o.object)(vec![
            ("legacyRequest", request.clone()),
            ("v3Request", request.clone()),
            ("v3Rows", rows.clone()),
        ]),
    )?;
    if (h.less_than)(
        (o.number)(0.),
        path(&h, &validation, &["blockers", "length"])?,
    )? || (o.strict_equal)(
        &path(&h, &validation, &["v3Interval", "status"])?,
        &(o.string)("blocked"),
    ) {
        return error(&h, || {
            let blockers = get(&h, &validation, "blockers")?;
            let ops = h.clone();
            let codes = method(&h, blockers, "map", || {
                Ok(vec![(o.function)(
                    "",
                    1,
                    Rc::new(move |args| get(&ops, &argument(&ops, &args, 0), "code")),
                )])
            })?;
            let joined = method(&h, codes, "join", || Ok(vec![(o.string)(",")]))?;
            template(
                &h,
                &["WOO V3 kline rows cannot form an exact canonical: ", ""],
                vec![joined],
            )
        });
    }
    let request = (o.object)(vec![
        ("symbol", get(&h, request, "symbol")?),
        (
            "interval",
            path(&h, &validation, &["v3Interval", "interval"])?,
        ),
    ]);
    let candles = map_v3_candles(h.clone(), rows)?;
    canonical::canonicalize_candles_for_request(&h.canonical, &request, candles)
}
pub fn compare_order_book_side<V: Clone + 'static, E: 'static>(
    h: &SnapshotParityOps<V, E>,
    side: V,
    legacy: V,
    v3: V,
) -> Result<V, E> {
    let o = &h.canonical.ops;
    let fields = (o.array)(vec![
        (o.string)("price"),
        (o.string)("amount"),
        (o.string)("total"),
    ]);
    let mismatches = (o.array)(vec![]);
    let length = method(h, global(h, "Math")?, "max", || {
        Ok(vec![get(h, &legacy, "length")?, get(h, &v3, "length")?])
    })?;
    let mut index = 0.;
    while (h.less_than)((o.number)(index), length.clone())? {
        let left = (o.get_key)(&legacy, &(o.number)(index))?;
        let right = (o.get_key)(&v3, &(o.number)(index))?;
        if (h.is_undefined)(&left) || (h.is_undefined)(&right) {
            push(h, mismatches.clone(), || {
                let kind = if (h.is_undefined)(&left) {
                    "unexpected-in-v3"
                } else {
                    "missing-in-v3"
                };
                let path = template(
                    h,
                    &["orderbook.", "[", "]"],
                    vec![side.clone(), (o.number)(index)],
                )?;
                let left = if (h.is_undefined)(&left) {
                    (o.undefined)()
                } else {
                    canonical::canonical_order_book_level(o, &left)?
                };
                let right = if (h.is_undefined)(&right) {
                    (o.undefined)()
                } else {
                    canonical::canonical_order_book_level(o, &right)?
                };
                mismatch(
                    h,
                    kind,
                    path,
                    left,
                    right,
                    (o.string)("v1 price/quantity normalized with cumulative total"),
                    (o.string)("V3 price/quantity normalized with cumulative total"),
                )
            })?;
        } else {
            (h.iterate)(fields.clone(), &mut |field| {
                let equal = method(h, global(h, "Object")?, "is", || {
                    Ok(vec![
                        (o.get_key)(&left, &field)?,
                        (o.get_key)(&right, &field)?,
                    ])
                })?;
                if !(o.truthy)(&equal) {
                    push(h, mismatches.clone(), || {
                        let path = template(
                            h,
                            &["orderbook.", "[", "].", ""],
                            vec![side.clone(), (o.number)(index), field.clone()],
                        )?;
                        let a = (o.get_key)(&left, &field)?;
                        let b = (o.get_key)(&right, &field)?;
                        let name = if (o.strict_equal)(&field, &(o.string)("amount")) {
                            (o.string)("quantity")
                        } else {
                            field.clone()
                        };
                        let lp = template(h, &["v1 ", "[].", ""], vec![side.clone(), name])?;
                        let name = if (o.strict_equal)(&field, &(o.string)("amount")) {
                            (o.string)("quantity")
                        } else {
                            field.clone()
                        };
                        let rp = template(h, &["V3 ", "[].", ""], vec![side.clone(), name])?;
                        mismatch(h, "value", path, a, b, lp, rp)
                    })?;
                }
                Ok(())
            })?;
        }
        index += 1.;
    }
    Ok(mismatches)
}
fn result<V: Clone, E>(h: &SnapshotParityOps<V, E>, input: &V) -> Result<V, E> {
    let o = &h.canonical.ops;
    let blockers = freeze(h, || {
        let value = get(h, input, "blockers")?;
        let value = if (h.is_nullish)(&value) {
            (o.array)(vec![])
        } else {
            value
        };
        Ok((o.array)((o.spread)(&value)?))
    })?;
    let mismatches = freeze(h, || {
        Ok((o.array)((o.spread)(&get(h, input, "mismatches")?)?))
    })?;
    freeze(h, || {
        let status = if (h.less_than)((o.number)(0.), get(h, &blockers, "length")?)? {
            "blocked"
        } else if (h.less_than)((o.number)(0.), get(h, &mismatches, "length")?)? {
            "mismatch"
        } else {
            "match"
        };
        Ok((o.object)(vec![
            ("status", (o.string)(status)),
            ("routeId", get(h, input, "routeId")?),
            ("authoritative", get(h, input, "authoritative")?),
            ("observed", get(h, input, "observed")?),
            ("legacyCanonical", get(h, input, "legacyCanonical")?),
            ("v3Canonical", get(h, input, "v3Canonical")?),
            ("mismatches", mismatches),
            ("blockers", blockers),
            ("storeAction", (o.string)("retain-legacy")),
        ]))
    })
}
pub fn compare_order_book<V: Clone + 'static, E: 'static>(
    h: &SnapshotParityOps<V, E>,
    input: &V,
) -> Result<V, E> {
    let o = &h.canonical.ops;
    let blockers = (o.array)(vec![]);
    if !(o.truthy)(&path(h, input, &["legacyRequest", "symbol"])?)
        || !(o.truthy)(&path(h, input, &["v3Request", "symbol"])?)
    {
        push(h, blockers.clone(), || {
            blocker(
                h,
                "invalid-request-context",
                (o.string)("Orderbook request symbols must be non-empty"),
                vec![
                    ("path", (o.string)("orderbook.request.symbol")),
                    ("expected", (o.string)("non-empty exact WOO symbol")),
                    (
                        "actual",
                        (o.object)(vec![
                            ("legacy", path(h, input, &["legacyRequest", "symbol"])?),
                            ("v3", path(h, input, &["v3Request", "symbol"])?),
                        ]),
                    ),
                ],
            )
        })?;
    } else if !(o.strict_equal)(
        &path(h, input, &["legacyRequest", "symbol"])?,
        &path(h, input, &["v3Request", "symbol"])?,
    ) {
        push(h, blockers.clone(), || {
            blocker(
                h,
                "request-symbol-mismatch",
                (o.string)("Legacy and V3 orderbook requests target different symbols"),
                vec![
                    ("path", (o.string)("orderbook.request.symbol")),
                    ("expected", path(h, input, &["legacyRequest", "symbol"])?),
                    ("actual", path(h, input, &["v3Request", "symbol"])?),
                ],
            )
        })?;
    }
    let authoritative = get(h, input, "legacyOrderBook")?;
    let observed = get(h, input, "v3OrderBook")?;
    let legacy_canonical = if (o.truthy)(&path(h, input, &["legacyRequest", "symbol"])?) {
        canonical::canonicalize_order_book_values(
            &h.canonical,
            &get(h, input, "legacyRequest")?,
            &get(h, input, "legacyOrderBook")?,
        )?
    } else {
        canonical::canonical_string(
            o,
            (o.object)(vec![("request", get(h, input, "legacyRequest")?)]),
        )?
    };
    let v3_canonical = if (o.truthy)(&path(h, input, &["v3Request", "symbol"])?) {
        canonical::canonicalize_v3_order_book(
            &h.canonical,
            &get(h, input, "v3Request")?,
            &get(h, input, "v3OrderBook")?,
        )?
    } else {
        canonical::canonical_string(
            o,
            (o.object)(vec![("request", get(h, input, "v3Request")?)]),
        )?
    };
    let left = compare_order_book_side(
        h,
        (o.string)("bids"),
        path(h, input, &["legacyOrderBook", "bids"])?,
        path(h, input, &["v3OrderBook", "bids"])?,
    )?;
    let mut mismatches = (o.spread)(&left)?;
    let right = compare_order_book_side(
        h,
        (o.string)("asks"),
        path(h, input, &["legacyOrderBook", "asks"])?,
        path(h, input, &["v3OrderBook", "asks"])?,
    )?;
    mismatches.extend((o.spread)(&right)?);
    result(
        h,
        &(o.object)(vec![
            ("routeId", (o.string)("orderbook")),
            ("authoritative", authoritative),
            ("observed", observed),
            ("legacyCanonical", legacy_canonical),
            ("v3Canonical", v3_canonical),
            ("mismatches", (o.array)(mismatches)),
            ("blockers", blockers),
        ]),
    )
}
fn field_arrow<V: Clone + 'static, E: 'static>(
    h: &SnapshotParityOps<V, E>,
    name: &str,
    keys: &[&'static str],
) -> V {
    let o = h.canonical.ops.clone();
    let keys = keys.to_vec();
    (h.canonical.ops.function)(
        name,
        1,
        Rc::new(move |args| {
            let mut value = args.first().cloned().unwrap_or_else(|| (o.undefined)());
            for key in &keys {
                value = (o.get)(&value, key)?;
            }
            Ok(value)
        }),
    )
}
fn market_fields<V: Clone + 'static, E: 'static>(h: &SnapshotParityOps<V, E>) -> Result<V, E> {
    let o = &h.canonical.ops;
    let mut fields = vec![];
    for (name, keys, lp, rp) in [
        (
            "id",
            vec!["id"],
            "v1 symbol split + USDT contract ID",
            "V3 baseAsset/quoteAsset",
        ),
        (
            "symbol",
            vec!["symbol"],
            "v1 symbol",
            "V3 symbol/baseAsset/quoteAsset",
        ),
        ("base", vec!["base"], "v1 symbol base", "V3 baseAsset"),
        ("quote", vec!["quote"], "v1 symbol quote", "V3 quoteAsset"),
        (
            "active",
            vec!["active"],
            "v1 supported PERP row",
            "V3 status when present",
        ),
        (
            "precision.amount",
            vec!["precision", "amount"],
            "v1 base_tick",
            "V3 baseTick",
        ),
        (
            "precision.price",
            vec!["precision", "price"],
            "v1 quote_tick",
            "V3 quoteTick",
        ),
        (
            "limits.amount.min",
            vec!["limits", "amount", "min"],
            "v1 base_min",
            "V3 baseMin",
        ),
        (
            "limits.amount.max",
            vec!["limits", "amount", "max"],
            "v1 base_max",
            "V3 baseMax",
        ),
        (
            "limits.leverage.min",
            vec!["limits", "leverage", "min"],
            "safe-cex legacy constant",
            "safe-cex V3 constant",
        ),
        (
            "limits.leverage.max",
            vec!["limits", "leverage", "max"],
            "floor(1 / v1 base_imr)",
            "floor(1 / V3 baseIMR)",
        ),
    ] {
        fields.push((o.object)(vec![
            ("path", (o.string)(name)),
            ("legacy", field_arrow(h, "legacy", &keys)),
            ("v3", field_arrow(h, "v3", &keys)),
            ("legacyProvenance", (o.string)(lp)),
            ("v3Provenance", (o.string)(rp)),
        ]));
    }
    Ok((o.array)(fields))
}
fn ticker_fields<V: Clone + 'static, E: 'static>(h: &SnapshotParityOps<V, E>) -> Result<V, E> {
    let o = &h.canonical.ops;
    let mut rows = vec![];
    for (name, key, lp, rp) in [
        ("id", "id", "legacy mapped market", "V3 mapped market"),
        ("symbol", "symbol", "v1 symbol", "V3 symbol"),
        ("index", "index", "v1 index_price", "V3 indexPrice"),
        ("mark", "mark", "v1 mark_price", "V3 markPrice"),
        (
            "percentage",
            "percentage24h",
            "v1 24h_open/24h_close",
            "V3 24hOpen/24hClose",
        ),
        (
            "openInterest",
            "openInterest",
            "v1 open_interest",
            "V3 openInterest",
        ),
        (
            "fundingRate",
            "estimatedFundingRate",
            "legacy safe-cex fundingRate from estfundingrate stream",
            "V3 estFundingRate",
        ),
        (
            "fundingTime",
            "fundingTime",
            "v1 next_funding_time",
            "V3 nextFundingTime",
        ),
        ("volume", "volume24h", "v1 24h_volume", "V3 24hVolume"),
        (
            "quoteVolume",
            "quoteVolume24h",
            "v1 24h_amount",
            "V3 24hAmount",
        ),
    ] {
        rows.push((o.array)(vec![
            (o.string)(name),
            field_arrow(h, "", &[name]),
            field_arrow(h, "", &[key]),
            (o.string)(lp),
            (o.string)(rp),
        ]));
    }
    let binding = h.binding.clone();
    let o = h.canonical.ops.clone();
    method(h, (h.canonical.ops.array)(rows), "map", || {
        Ok(vec![(h.canonical.ops.function)(
            "",
            1,
            Rc::new(move |args| {
                let row = args.first().cloned().unwrap_or_else(|| (o.undefined)());
                let values = binding(row, 5)?;
                Ok((o.object)(vec![
                    ("path", values[0].clone()),
                    ("legacy", values[1].clone()),
                    ("v3", values[2].clone()),
                    ("legacyProvenance", values[3].clone()),
                    ("v3Provenance", values[4].clone()),
                ]))
            }),
        )])
    })
}
fn candle_fields<V: Clone + 'static, E: 'static>(h: &SnapshotParityOps<V, E>) -> Result<V, E> {
    let o = &h.canonical.ops;
    let values = (o.array)(
        ["open", "high", "low", "close", "volume"]
            .into_iter()
            .map(|v| (o.string)(v))
            .collect(),
    );
    let o = h.canonical.ops.clone();
    let template = h.template.clone();
    method(h, values, "map", || {
        Ok(vec![(h.canonical.ops.function)(
            "",
            1,
            Rc::new(move |args| {
                let field = args.first().cloned().unwrap_or_else(|| (o.undefined)());
                let left = o.clone();
                let key = field.clone();
                let legacy = (o.function)(
                    "legacy",
                    1,
                    Rc::new(move |args| {
                        (left.get_key)(
                            &args.first().cloned().unwrap_or_else(|| (left.undefined)()),
                            &key,
                        )
                    }),
                );
                let right = o.clone();
                let key = field.clone();
                let v3 = (o.function)(
                    "v3",
                    1,
                    Rc::new(move |args| {
                        (right.get_key)(
                            &args.first().cloned().unwrap_or_else(|| (right.undefined)()),
                            &key,
                        )
                    }),
                );
                let lp = template(&["v1 ", ""], vec![field.clone()])?;
                let rp = template(&["V3 ", ""], vec![field.clone()])?;
                Ok((o.object)(vec![
                    ("path", field),
                    ("legacy", legacy),
                    ("v3", v3),
                    ("legacyProvenance", lp),
                    ("v3Provenance", rp),
                ]))
            }),
        )])
    })
}
fn compare_fields<V: Clone + 'static, E: 'static>(
    h: Rc<SnapshotParityOps<V, E>>,
    legacy: V,
    v3: V,
    prefix: V,
    fields: V,
) -> Result<V, E> {
    let o = &h.canonical.ops;
    let ops = h.clone();
    method(&h, fields, "flatMap", || {
        Ok(vec![(o.function)(
            "",
            1,
            Rc::new(move |args| {
                let field = argument(&ops, &args, 0);
                let o = &ops.canonical.ops;
                let left = method(&ops, field.clone(), "legacy", || Ok(vec![legacy.clone()]))?;
                let right = method(&ops, field.clone(), "v3", || Ok(vec![v3.clone()]))?;
                let equal = method(&ops, global(&ops, "Object")?, "is", || {
                    Ok(vec![left.clone(), right.clone()])
                })?;
                if (o.truthy)(&equal) {
                    Ok((o.array)(vec![]))
                } else {
                    let path = template(
                        &ops,
                        &["", ".", ""],
                        vec![prefix.clone(), get(&ops, &field, "path")?],
                    )?;
                    let lp = get(&ops, &field, "legacyProvenance")?;
                    let rp = get(&ops, &field, "v3Provenance")?;
                    Ok((o.array)(vec![mismatch(
                        &ops, "value", path, left, right, lp, rp,
                    )?]))
                }
            }),
        )])
    })
}
fn push_many<V, E>(
    h: &SnapshotParityOps<V, E>,
    array: V,
    values: impl FnOnce() -> Result<V, E>,
) -> Result<(), E> {
    method(h, array, "push", || (h.canonical.ops.spread)(&values()?))?;
    Ok(())
}
fn compare_keyed<V: Clone + 'static, E: 'static>(
    h: Rc<SnapshotParityOps<V, E>>,
    input: &V,
) -> Result<V, E> {
    let o = &h.canonical.ops;
    let mismatches = (o.array)(vec![]);
    let legacy = (h.construct)(global(&h, "Map")?, vec![])?;
    let v3 = (h.construct)(global(&h, "Map")?, vec![])?;
    let values = get(&h, input, "legacy")?;
    (h.iterate)(values, &mut |value| {
        let key = method(&h, input.clone(), "legacyKey", || Ok(vec![value.clone()]))?;
        let has = method(&h, legacy.clone(), "has", || Ok(vec![key.clone()]))?;
        if (o.truthy)(&has) {
            push(&h, mismatches.clone(), || {
                let path = template(
                    &h,
                    &["", "[", "]"],
                    vec![get(&h, input, "path")?, key.clone()],
                )?;
                mismatch(
                    &h,
                    "duplicate-key",
                    path,
                    key,
                    (o.undefined)(),
                    get(&h, input, "legacyProvenance")?,
                    get(&h, input, "v3Provenance")?,
                )
            })?;
        } else {
            method(&h, legacy.clone(), "set", || Ok(vec![key, value]))?;
        }
        Ok(())
    })?;
    let values = get(&h, input, "v3")?;
    (h.iterate)(values, &mut |value| {
        let key = method(&h, input.clone(), "v3Key", || Ok(vec![value.clone()]))?;
        let has = method(&h, v3.clone(), "has", || Ok(vec![key.clone()]))?;
        if (o.truthy)(&has) {
            push(&h, mismatches.clone(), || {
                let path = template(
                    &h,
                    &["", "[", "]"],
                    vec![get(&h, input, "path")?, key.clone()],
                )?;
                mismatch(
                    &h,
                    "duplicate-key",
                    path,
                    (o.undefined)(),
                    key,
                    get(&h, input, "legacyProvenance")?,
                    get(&h, input, "v3Provenance")?,
                )
            })?;
        } else {
            method(&h, v3.clone(), "set", || Ok(vec![key, value]))?;
        }
        Ok(())
    })?;
    (h.iterate)(legacy.clone(), &mut |entry| {
        let pair = (h.binding)(entry, 2)?;
        let key = pair[0].clone();
        let left = pair[1].clone();
        let right = method(&h, v3.clone(), "get", || Ok(vec![key.clone()]))?;
        if (h.is_undefined)(&right) {
            push(&h, mismatches.clone(), || {
                let path = template(
                    &h,
                    &["", "[", "]"],
                    vec![get(&h, input, "path")?, key.clone()],
                )?;
                mismatch(
                    &h,
                    "missing-in-v3",
                    path,
                    key,
                    (o.undefined)(),
                    get(&h, input, "legacyProvenance")?,
                    get(&h, input, "v3Provenance")?,
                )
            })?;
        } else {
            push_many(&h, mismatches.clone(), || {
                let prefix = template(&h, &["", "[", "]"], vec![get(&h, input, "path")?, key])?;
                let fields = get(&h, input, "fields")?;
                compare_fields(h.clone(), left, right, prefix, fields)
            })?;
        }
        Ok(())
    })?;
    (h.iterate)(v3, &mut |entry| {
        let key = (h.binding)(entry, 1)?[0].clone();
        let has = method(&h, legacy.clone(), "has", || Ok(vec![key.clone()]))?;
        if !(o.truthy)(&has) {
            push(&h, mismatches.clone(), || {
                let path = template(
                    &h,
                    &["", "[", "]"],
                    vec![get(&h, input, "path")?, key.clone()],
                )?;
                mismatch(
                    &h,
                    "unexpected-in-v3",
                    path,
                    (o.undefined)(),
                    key,
                    get(&h, input, "legacyProvenance")?,
                    get(&h, input, "v3Provenance")?,
                )
            })?;
        }
        Ok(())
    })?;
    Ok(mismatches)
}
pub fn compare_markets<V: Clone + 'static, E: 'static>(
    h: Rc<SnapshotParityOps<V, E>>,
    legacy: V,
    v3: V,
) -> Result<V, E> {
    let o = &h.canonical.ops;
    let left = canonical::canonicalize_markets(&h.canonical, legacy.clone())?;
    let right = canonical::canonicalize_markets(&h.canonical, v3.clone())?;
    let mismatches = compare_keyed(
        h.clone(),
        &(o.object)(vec![
            ("path", (o.string)("markets")),
            ("legacy", legacy.clone()),
            ("v3", v3.clone()),
            ("legacyKey", field_arrow(&h, "legacyKey", &["symbol"])),
            ("v3Key", field_arrow(&h, "v3Key", &["symbol"])),
            ("fields", h.fields[0].clone()),
            (
                "legacyProvenance",
                (o.string)("GET /v1/public/info mapped perpetual rows"),
            ),
            (
                "v3Provenance",
                (o.string)("GET /v3/public/instruments mapped perpetual rows"),
            ),
        ]),
    )?;
    result(
        &h,
        &(o.object)(vec![
            ("routeId", (o.string)("markets")),
            ("authoritative", legacy),
            ("observed", v3),
            ("legacyCanonical", left),
            ("v3Canonical", right),
            ("mismatches", mismatches),
        ]),
    )
}
pub fn compare_ticker_stats<V: Clone + 'static, E: 'static>(
    h: Rc<SnapshotParityOps<V, E>>,
    legacy: V,
    v3: V,
) -> Result<V, E> {
    let o = &h.canonical.ops;
    let left = canonical::canonicalize_legacy_tickers(&h.canonical, legacy.clone())?;
    let right = canonical::canonicalize_v3_tickers(&h.canonical, v3.clone())?;
    let mismatches = compare_keyed(
        h.clone(),
        &(o.object)(vec![
            ("path", (o.string)("tickers")),
            ("legacy", legacy.clone()),
            ("v3", v3.clone()),
            ("legacyKey", field_arrow(&h, "legacyKey", &["symbol"])),
            ("v3Key", field_arrow(&h, "v3Key", &["symbol"])),
            ("fields", h.fields[1].clone()),
            (
                "legacyProvenance",
                (o.string)("GET /v1/public/futures mapped ticker"),
            ),
            (
                "v3Provenance",
                (o.string)("GET /v3/public/futures mapped statistics"),
            ),
        ]),
    )?;
    let blocker = freeze(&h, || {
        Ok((o.object)(vec![
            ("code", (o.string)("v3-futures-rest-has-no-live-quotes")),
            (
                "detail",
                (o.string)(
                    "Legacy REST fills bid/ask/last from 24h_close, while V3 REST has no best-bid, best-ask, or last-trade fields; V3 ticker authority requires coherent BBO and Trade streams.",
                ),
            ),
        ]))
    })?;
    result(
        &h,
        &(o.object)(vec![
            ("routeId", (o.string)("tickers")),
            ("authoritative", legacy),
            ("observed", v3),
            ("legacyCanonical", left),
            ("v3Canonical", right),
            ("mismatches", mismatches),
            ("blockers", (o.array)(vec![blocker])),
        ]),
    )
}
fn timestamp_key<V: Clone + 'static, E: 'static>(h: &SnapshotParityOps<V, E>, name: &str) -> V {
    let o = h.canonical.ops.clone();
    (h.canonical.ops.function)(
        name,
        1,
        Rc::new(move |args| {
            let function = (o.global)("String")?;
            let candle = args.first().cloned().unwrap_or_else(|| (o.undefined)());
            let timestamp = (o.get)(&candle, "timestamp")?;
            (o.invoke)(function, (o.undefined)(), vec![timestamp])
        }),
    )
}
pub fn compare_candles<V: Clone + 'static, E: 'static>(
    h: Rc<SnapshotParityOps<V, E>>,
    input: &V,
) -> Result<V, E> {
    let o = &h.canonical.ops;
    let validation = validate_kline_context(&h, input)?;
    let v3_candles = map_v3_candles(h.clone(), get(&h, input, "v3Rows")?)?;
    let legacy_canonical = if (o.strict_equal)(
        &path(&h, &validation, &["legacyInterval", "status"])?,
        &(o.string)("ready"),
    ) {
        let request = (o.object)(vec![
            ("symbol", path(&h, input, &["legacyRequest", "symbol"])?),
            (
                "interval",
                path(&h, &validation, &["legacyInterval", "interval"])?,
            ),
        ]);
        canonical::canonicalize_candles_for_request(
            &h.canonical,
            &request,
            get(&h, input, "legacyCandles")?,
        )?
    } else {
        let request = get(&h, input, "legacyRequest")?;
        let candles = method(&h, get(&h, input, "legacyCandles")?, "map", || {
            Ok(vec![h.canonical.candle_mapper()])
        })?;
        canonical::canonical_string(
            o,
            (o.object)(vec![("request", request), ("candles", candles)]),
        )?
    };
    let v3_canonical = if (o.strict_equal)(
        &path(&h, &validation, &["v3Interval", "status"])?,
        &(o.string)("ready"),
    ) && (o.strict_equal)(
        &path(&h, &validation, &["blockers", "length"])?,
        &(o.number)(0.),
    ) {
        let request = (o.object)(vec![
            ("symbol", path(&h, input, &["v3Request", "symbol"])?),
            (
                "interval",
                path(&h, &validation, &["v3Interval", "interval"])?,
            ),
        ]);
        canonical::canonicalize_candles_for_request(&h.canonical, &request, v3_candles.clone())?
    } else {
        let request = get(&h, input, "v3Request")?;
        let ops = h.clone();
        let rows = method(&h, get(&h, input, "v3Rows")?, "map", || {
            Ok(vec![(o.function)(
                "",
                1,
                Rc::new(move |args| {
                    let row = argument(&ops, &args, 0);
                    let o = &ops.canonical.ops;
                    Ok((o.object)(vec![
                        ("symbol", get(&ops, &row, "symbol")?),
                        ("type", get(&ops, &row, "type")?),
                        ("startTimestamp", get(&ops, &row, "startTimestamp")?),
                        ("endTimestamp", get(&ops, &row, "endTimestamp")?),
                    ]))
                }),
            )])
        })?;
        canonical::canonical_string(o, (o.object)(vec![("request", request), ("rows", rows)]))?
    };
    let route = get(&h, input, "routeId")?;
    let authoritative = get(&h, input, "legacyCandles")?;
    let observed = get(&h, input, "v3Rows")?;
    let legacy = get(&h, input, "legacyCandles")?;
    let legacy_key = timestamp_key(&h, "legacyKey");
    let v3_key = timestamp_key(&h, "v3Key");
    let lp = (o.string)(
        if (o.strict_equal)(&get(&h, input, "routeId")?, &(o.string)("recent-klines")) {
            "GET /v1/kline rows"
        } else {
            "GET legacy historical kline rows"
        },
    );
    let rp = (o.string)(
        if (o.strict_equal)(&get(&h, input, "routeId")?, &(o.string)("recent-klines")) {
            "GET /v3/public/kline rows"
        } else {
            "GET /v3/public/klineHistory rows"
        },
    );
    let mismatches = compare_keyed(
        h.clone(),
        &(o.object)(vec![
            ("path", (o.string)("candles")),
            ("legacy", legacy),
            ("v3", v3_candles),
            ("legacyKey", legacy_key),
            ("v3Key", v3_key),
            ("fields", h.fields[2].clone()),
            ("legacyProvenance", lp),
            ("v3Provenance", rp),
        ]),
    )?;
    let blockers = get(&h, &validation, "blockers")?;
    result(
        &h,
        &(o.object)(vec![
            ("routeId", route),
            ("authoritative", authoritative),
            ("observed", observed),
            ("legacyCanonical", legacy_canonical),
            ("v3Canonical", v3_canonical),
            ("mismatches", mismatches),
            ("blockers", blockers),
        ]),
    )
}
