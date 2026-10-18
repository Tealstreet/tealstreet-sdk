//! The named JS expressions the ported OKX bodies delegate to their host
//! (`intrinsic("name", args)`). Each arm is the TS expression it stands for.
use safe_cex_exchange::js_runtime::{
    ops,
    value::{Js, Thrown, arg},
};

/// What an intrinsic needs from the running receiver; implemented by the scope.
pub(crate) trait IntrinsicHost {
    fn this(&self) -> Js;
    fn initialize_okx_fields(&self, receiver: &Js) -> Result<(), Thrown>;
}

fn text(v: &Js) -> Result<String, Thrown> {
    Ok(String::from_utf16_lossy(&v.to_js_string()?))
}

fn items(v: &Js) -> Result<Vec<Js>, Thrown> {
    v.array_values()
        .ok_or_else(|| Thrown::type_error(&format!("{} is not iterable", v.to_display())))
}

fn store_member(host: &dyn IntrinsicHost, table: &str, key: &Js) -> Result<Js, Thrown> {
    let store = host.this().get("store")?;
    store.get(table)?.get_key(&key.to_js_string()?)
}

/// Abstract equality (`==`) over the value kinds these bodies compare.
fn loose_equal(a: &Js, b: &Js) -> Result<bool, Thrown> {
    Ok(match (a, b) {
        (Js::Undefined | Js::Null, Js::Undefined | Js::Null) => true,
        (Js::Undefined | Js::Null, _) | (_, Js::Undefined | Js::Null) => false,
        (
            Js::Number(_) | Js::String(_) | Js::Bool(_),
            Js::Number(_) | Js::String(_) | Js::Bool(_),
        ) if std::mem::discriminant(a) != std::mem::discriminant(b) => {
            a.to_number()? == b.to_number()?
        }
        _ => a.strict_equals(b),
    })
}

/// `array.sort((a, b) => b[key] - a[key])`: stable, NaN differences compare equal.
fn sort_desc(array: &Js, key: &str) -> Result<Js, Thrown> {
    let values = items(array)?;
    let mut keyed = Vec::with_capacity(values.len());
    for value in values {
        let k = value.get(key)?.to_number()?;
        keyed.push((k, value));
    }
    keyed.sort_by(|(a, _), (b, _)| {
        let d = b - a;
        if d > 0.0 {
            std::cmp::Ordering::Greater
        } else if d < 0.0 {
            std::cmp::Ordering::Less
        } else {
            std::cmp::Ordering::Equal
        }
    });
    let sorted: Vec<Js> = keyed.into_iter().map(|(_, v)| v).collect();
    if let Js::Array(cells) = array {
        *cells.borrow_mut() = sorted.iter().cloned().map(Some).collect();
    }
    Ok(array.clone())
}

fn message(parts: &[&str]) -> Js {
    Js::str(&parts.concat())
}

pub(crate) fn intrinsic(host: &dyn IntrinsicHost, name: &str, a: Vec<Js>) -> Result<Js, Thrown> {
    let x = arg(&a, 0);
    let y = arg(&a, 1);
    Ok(match name {
        "null" => Js::Null,
        // `+x`
        "unary-plus" => Js::Number(x.to_number()?),
        "add" => ops::add(&x, &y)?,
        "add-one" => Js::Number(x.to_number()? + 1.0),
        "subtract" => ops::subtract(&x, &y)?,
        "multiply" => ops::multiply(&x, &y)?,
        "divide" => ops::divide(&x, &y)?,
        "greater-than" => Js::Bool(ops::greater_than(&x, &y)?),
        "less-than" => Js::Bool(ops::less_than(&x, &y)?),
        "less-or-equal" => Js::Bool(ops::less_equal(&x, &y)?),
        "greater-or-equal" => Js::Bool(ops::greater_equal(&x, &y)?),
        "loose-equal" => Js::Bool(loose_equal(&x, &y)?),
        "typeof-string" => Js::Bool(x.is_string()),
        "is-array" => Js::Bool(x.is_array()),
        "string" => Js::utf16(&x.to_js_string()?),
        "Number.isFinite" => Js::Bool(matches!(x, Js::Number(n) if n.is_finite())),
        // `${x}` / `${a}${b}...` / `${id}${suffix}` / `${a}:${b}`
        "template-string" => Js::utf16(&x.to_js_string()?),
        "template-concat" | "template-suffix" => {
            let mut out = Vec::new();
            for part in &a {
                out.extend_from_slice(&part.to_js_string()?);
            }
            Js::utf16(&out)
        }
        "colon-append" => {
            let mut out = x.to_js_string()?.to_vec();
            out.push(58);
            out.extend_from_slice(&y.to_js_string()?);
            Js::utf16(&out)
        }
        "id-prefix" => Js::str(&format!("id:{}", text(&x)?)),
        // `String(x).toUpperCase()` on an already-string expression
        "toUpperCase-expression" => Js::str(&text(&x)?.to_uppercase()),
        "to-property-key" => Js::utf16(&x.to_js_string()?),
        // `id.replace(/_[a-zA-Z]+$/, '')`
        "replace-order-id-suffix" => {
            let id = text(&x)?;
            let cut = id.rfind('_').filter(|&i| {
                i + 1 < id.len() && id[i + 1..].bytes().all(|b| b.is_ascii_alphabetic())
            });
            Js::str(cut.map_or(id.as_str(), |i| &id[..i]))
        }
        // `interval.split(/(\d+)/)` -> ['', amount, unit]
        "split-interval-digits" => {
            let s = text(&x)?;
            let start = s.find(|c: char| c.is_ascii_digit()).unwrap_or(s.len());
            let end = s[start..]
                .find(|c: char| !c.is_ascii_digit())
                .map_or(s.len(), |i| start + i);
            Js::array(vec![
                Js::str(&s[..start]),
                Js::str(&s[start..end]),
                Js::str(&s[end..]),
            ])
        }
        "join-comma" => {
            let parts: Result<Vec<String>, Thrown> = items(&x)?
                .iter()
                .map(|v| {
                    if v.is_nullish() {
                        Ok(String::new())
                    } else {
                        text(v)
                    }
                })
                .collect();
            Js::str(&parts?.join(","))
        }
        // `${url}${params ? `?${params}` : ''}`
        "signing-url" => {
            let url = text(&x)?;
            if y.truthy() {
                Js::str(&format!("{url}?{}", text(&y)?))
            } else {
                Js::str(&url)
            }
        }
        "ticker-percentage" => {
            let (last, open) = (x.to_number()?, y.to_number()?);
            Js::Number(((last - open) / open) * 100.0)
        }
        // `(o.remaining * o.price) / leverage`
        "order-notional-over-leverage" => {
            Js::Number((x.to_number()? * y.to_number()?) / arg(&a, 2).to_number()?)
        }
        "Math.min-current-plus-seven-days" => {
            // Math.min(currentStart + SEVEN_DAYS_MS, endTime)
            Js::Number((x.to_number()? + arg(&a, 2).to_number()?).min(y.to_number()?))
        }
        // `opts.to ? String(opts.to) : undefined` is applied by the caller's guard.
        "String-current-to" => Js::utf16(&x.get("to")?.to_js_string()?),
        "String-oldest-minus-one" => Js::str(&safe_cex_num::js_number_string(x.to_number()? - 1.0)),
        // rows.reduce over parseInt(String(row.uTime ?? '0'), 10), finite and > 0, from +Infinity
        "reduce-oldest-positive-uTime" => {
            let mut oldest = f64::INFINITY;
            for row in items(&x)? {
                let raw = row.get("uTime")?;
                let raw = if raw.is_nullish() { Js::str("0") } else { raw };
                let t = safe_cex_num::coercion::parse_int(&text(&raw)?, Some(10));
                if t.is_finite() && t > 0.0 {
                    oldest = oldest.min(t);
                }
            }
            Js::Number(oldest)
        }
        // `response.data.data`
        "response-data-data" => x.get("data")?.get("data")?,
        "Array.isArray-or-empty" => {
            if x.is_array() {
                x
            } else {
                Js::array(vec![])
            }
        }
        "last-index" => {
            let values = items(&x)?;
            values.last().cloned().unwrap_or(Js::Undefined)
        }
        "get-index" => x.get_key(&y.to_js_string()?)?,
        "set-index" => {
            x.set_key(&y.to_js_string()?, arg(&a, 2))?;
            Js::Undefined
        }
        "delete-index" => {
            x.delete(&y.to_js_string()?);
            Js::Bool(true)
        }
        "define-literal-field" => {
            x.define_key(&y.to_js_string()?, arg(&a, 2))?;
            Js::Undefined
        }
        "destructure-two" => {
            let values = items(&x)?;
            Js::array(vec![
                values.first().cloned().unwrap_or(Js::Undefined),
                values.get(1).cloned().unwrap_or(Js::Undefined),
            ])
        }
        "array-push" => {
            x.push(y)?;
            Js::Undefined
        }
        "array-push-spread" => {
            for v in y.iterable_values()? {
                x.push(v)?;
            }
            Js::Undefined
        }
        // `[...a, ...b]`, `[...acc]`, `[...acc, row]`, `[...a, b]`
        "array-spread" | "array-spread-two" => {
            let mut out = Vec::new();
            for value in a {
                out.extend(value.iterable_values()?);
            }
            Js::array(out)
        }
        "array-spread-prefix" | "iterable-spread" => Js::array(x.iterable_values()?),
        "array-prefix-append" | "spread-append" => {
            let mut out = x.iterable_values()?;
            out.push(y);
            Js::array(out)
        }
        // `{ ...x }` / `{ id, ...metadata }`
        "object-spread" => x.spread_object()?,
        "object-spread-after-id" => {
            let out = Js::object(vec![("id", x)]);
            for (k, v) in y.own_entries()? {
                out.define_key(&k, v)?;
            }
            out
        }
        "Object.values-expression" => {
            Js::array(x.own_entries()?.into_iter().map(|(_, v)| v).collect())
        }
        "new-Map" => Js::new_map(),
        "new-Set" => Js::new_set(),
        "Map.values-spread" => Js::array(x.call_method("values", vec![])?.iterable_values()?),
        "Map.set-tranId-record" => x.call_method("set", vec![y.get("tranId")?, y])?,
        "Set.has-order-id" => x.call_method("has", vec![y.get("id")?])?,
        "Set.add-order-id" => x.call_method("add", vec![y.get("id")?])?,
        // `new Ctor(...args)`
        "construct" => {
            let ctor = x;
            let args = a.into_iter().skip(1).collect();
            ctor.construct(args, None)?
        }
        // `opts?.symbol` / `opts?.from` / `opts?.to` / `opts?.limit`, `market?.id`
        "opts-optional-symbol"
        | "opts-optional-from"
        | "opts-optional-to"
        | "opts-optional-limit"
        | "optional-id" => {
            let key = match name {
                "opts-optional-symbol" => "symbol",
                "opts-optional-from" => "from",
                "opts-optional-to" => "to",
                "opts-optional-limit" => "limit",
                _ => "id",
            };
            if x.is_nullish() {
                Js::Undefined
            } else {
                x.get(key)?
            }
        }
        "market-by-opts-symbol" => store_member(host, "marketsBySymbol", &x.get("symbol")?)?,
        // `this.store.marketsById[String(row.instId ?? '')]`
        "market-by-row-instId" | "optional-market-by-row-instId" => {
            let id = x.get("instId")?;
            let id = if id.is_nullish() { Js::str("") } else { id };
            store_member(host, "marketsById", &id)?
        }
        "contractsToBase-bind" => {
            let this = host.this();
            let method = this.get("contractsToBase")?;
            Js::function("contractsToBase", move |_, args| {
                method.call(this.clone(), args)
            })
        }
        "candle-handler" => Js::str(&format!("candle.{}.{}", text(&x)?, text(&y)?)),
        "initialize-okx-fields" => {
            host.initialize_okx_fields(&x)?;
            Js::Undefined
        }
        "loaded-okx-message" => message(&["Loaded ", &text(&x)?, " OKX markets"]),
        "market-error-message" | "market-not-found-message" => {
            message(&["Market ", &text(&x)?, " not found on OKX"])
        }
        "position-not-found-message" => message(&["Position ", &text(&x)?, " not found on OKX"]),
        "market-validation-message" => message(&["Market ", &text(&x)?, " ", &text(&y)?]),
        "amount-validation-message" => message(&[
            "Order amount for ",
            &text(&x)?,
            " must be finite and positive after adjustment",
        ]),
        "api-error-message" => message(&["OKX API error (code ", &text(&x)?, ")"]),
        "bills-cap-message" => message(&[
            "OKX bills paging hit the ",
            "200",
            "-page cap after ",
            &text(&x)?,
            " rows; older rows in this window were not fetched",
        ]),
        "sort-createdTime-desc" => sort_desc(&x, "createdTime")?,
        "sort-closedTime-desc" => sort_desc(&x, "closedTime")?,
        "sort-timestamp-desc" => sort_desc(&x, "timestamp")?,
        // `sorted.slice(0, opts.limit)`
        "slice-zero-current-limit" => {
            let values = items(&x)?;
            let limit = y.get("limit")?.to_number()?;
            let n = if limit.is_nan() {
                0
            } else {
                limit.max(0.0).min(values.len() as f64) as usize
            };
            Js::array(values.into_iter().take(n).collect())
        }
        _ => return Err(Thrown::type_error(&format!("unknown OKX intrinsic {name}"))),
    })
}
