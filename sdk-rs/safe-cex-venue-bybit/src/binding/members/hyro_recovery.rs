//! HyroOrderRecovery's retained submission state; observation/publication use the
//! existing source ports in hyro_order_publication.
use crate::binding::{
    promise::await_js,
    rt::Rt,
    value::{Js, Thrown, arg, error_object},
};
use safe_cex_exchange::js_runtime::{ops, scope::scope_of};
use std::rc::Rc;

pub(super) fn submission_key(rt: &Rt) -> Result<Js, Thrown> {
    let json = rt.shared.globals.get("JSON")?;
    let collator = Rc::new(
        safe_cex_num::JsRootCollator::try_new()
            .map_err(|error| Thrown::error(error.name, &error.message))?,
    );
    Ok(Js::function("hyroSubmissionKey", move |_, args| {
        let collator = collator.clone();
        let replacer = Js::function("submissionKeyReplacer", move |_, args| {
            let item = arg(&args, 1);
            if !item.truthy() || item.type_of() != "object" || item.is_array() {
                return Ok(item);
            }
            // Object.entries reads every enumerable value before filter/sort.
            let entries = item.own_entries()?;
            let mut entries = entries
                .into_iter()
                .filter(|(key, _)| {
                    key.as_ref() != crate::binding::value::utf16("orderLinkId").as_ref()
                })
                .map(|(key, value)| {
                    let text = String::from_utf16(&key).map_err(|_| {
                        Thrown::type_error("lone UTF-16 key at the typed collation boundary")
                    })?;
                    Ok((key, text, value))
                })
                .collect::<Result<Vec<_>, Thrown>>()?;
            entries.sort_by(|left, right| collator.compare(&left.1, &right.1));
            let object = Js::object(vec![]);
            for (key, _, value) in entries {
                object.define_key(&key, value)?;
            }
            Ok(object)
        });
        json.call_method("stringify", vec![arg(&args, 0), replacer])
    }))
}

fn rejected(brand: &Js, message: Js) -> Result<Thrown, Thrown> {
    let error = error_object("Error", "");
    error.set("message", message)?;
    if let Js::Object(object) = &error {
        *object.proto.borrow_mut() = Some(brand.clone());
    }
    Ok(Thrown::new(error))
}
fn is_rejected(brand: &Js, error: &Thrown) -> bool {
    matches!(&error.value, Js::Object(object) if object.proto.borrow().as_ref().is_some_and(|prototype| prototype.strict_equals(brand)))
}
fn unknown(rt: &Rt, payload: Js, cause: Js) -> Result<Thrown, Thrown> {
    Ok(Thrown::new(crate::hyro_order_publication::unknown(
        rt, payload, cause,
    )?))
}
fn accepted(payload: Js, id: Js) -> Js {
    Js::object(vec![
        ("payload", payload),
        (
            "data",
            Js::object(vec![
                ("retCode", Js::Number(0.0)),
                ("retMsg", Js::str("OK")),
                ("result", Js::object(vec![("orderId", id)])),
            ]),
        ),
    ])
}
fn ready(response: Js, boundary: Js) -> Js {
    Js::object(vec![("response", response), ("boundary", boundary)])
}
fn duplicate(rt: &Rt, meta: &Js) -> Result<bool, Thrown> {
    let code = meta.get("retCode")?;
    if code.strict_equals(&Js::Number(10014.0)) || code.strict_equals(&Js::Number(110072.0)) {
        return Ok(true);
    }
    let message = meta.get("retMsg")?;
    let message = if message.is_undefined() {
        Js::str("")
    } else {
        message
    };
    let regex = rt.shared.globals.get("RegExp")?.call(
        Js::Undefined,
        vec![Js::str("duplicate|duplicated|already.*exist"), Js::str("i")],
    )?;
    Ok(regex.call_method("test", vec![message])?.truthy())
}
fn meta(rt: &Rt, data: Js) -> Result<Js, Thrown> {
    rt.shared
        .globals
        .get("getBybitResponseMeta")?
        .call(Js::Undefined, vec![data])
}
fn endpoint(rt: &Rt, name: &str) -> Result<Js, Thrown> {
    rt.shared.globals.get("ENDPOINTS")?.get(name)
}
fn present(value: &Js) -> bool {
    !value.is_undefined()
}
fn ge(value: &Js, number: f64) -> Result<bool, Thrown> {
    ops::greater_equal(value, &Js::Number(number))
}
fn lt(value: &Js, number: f64) -> Result<bool, Thrown> {
    ops::less_than(value, &Js::Number(number))
}

pub(super) fn install(rt: &Rt, recovery: &Js, key: Js) -> Result<(), Thrown> {
    // Source's unexported subclass has Error's name/message ABI, with distinct
    // prototype identity for the recovery catch. No string-name rejection test.
    let brand = Js::object(vec![]);
    for name in ["unknown", "runGroup", "findOrder", "submit", "submitOne"] {
        let weak = Rc::downgrade(&rt.shared);
        let key = key.clone();
        let brand = brand.clone();
        recovery.define(
            name,
            Js::function(name, move |owner, args| {
                let rt = scope_of(&weak)?;
                if name == "unknown" {
                    return crate::hyro_order_publication::unknown(
                        &rt,
                        arg(&args, 0),
                        arg(&args, 1),
                    );
                }
                let body = rt.clone();
                let key = key.clone();
                let brand = brand.clone();
                Ok(rt.spawn(Box::pin(async move {
                    match name {
                        "runGroup" => {
                            crate::hyro_order_publication::run_group(
                                &body,
                                owner,
                                arg(&args, 0),
                                arg(&args, 1),
                            )
                            .await
                        }
                        "findOrder" => {
                            crate::hyro_order_publication::find_order(&body, owner, arg(&args, 0))
                                .await
                        }
                        "submit" => {
                            submit(&body, owner, arg(&args, 0), arg(&args, 1), key, brand).await
                        }
                        "submitOne" => submit_one(&body, owner, arg(&args, 0), brand).await,
                        _ => unreachable!(),
                    }
                })))
            }),
        )?;
    }
    Ok(())
}

async fn submit(
    rt: &Rt,
    owner: Js,
    payloads: Js,
    on_accepted: Js,
    key_fn: Js,
    brand: Js,
) -> Result<Js, Thrown> {
    let key = key_fn.call(Js::Undefined, vec![payloads.clone()])?;
    let pending = owner.get("pending")?;
    let mut group = pending.call_method("get", vec![key.clone()])?;
    if !group.is_nullish() && group.get("running")?.truthy() {
        return await_js(&rt.exchange_runtime(), group.get("running")?).await;
    }
    if !group.truthy() {
        let creates = payloads.call_method(
            "map",
            vec![Js::function("pendingCreate", |_, args| {
                Ok(Js::object(vec![
                    ("payload", arg(&args, 0)),
                    ("uncertain", Js::Bool(false)),
                ]))
            })],
        )?;
        group = Js::object(vec![
            ("creates", creates),
            ("responses", Js::array(vec![])),
            ("observedLinkIds", Js::new_set()),
        ]);
        pending.call_method("set", vec![key.clone(), group.clone()])?;
    }
    let running = owner.call_method("runGroup", vec![group.clone(), on_accepted])?;
    group.set("running", running.clone())?;
    let outcome = await_js(&rt.exchange_runtime(), running).await;
    let result = (|| -> Result<Js, Thrown> {
        match outcome {
            Ok(responses) => {
                pending.call_method("delete", vec![key])?;
                Ok(responses)
            }
            Err(error) => {
                if is_rejected(&brand, &error) {
                    let index = group.get("responses")?.get("length")?;
                    let create = group.get("creates")?.get_key(&index.to_js_string()?)?;
                    let payload = create.get("payload")?.spread_object()?;
                    payload.set(
                        "orderLinkId",
                        rt.shared
                            .globals
                            .get("nanoid")?
                            .call(Js::Undefined, vec![])?,
                    )?;
                    create.set("payload", payload)?;
                    create.set("uncertain", Js::Bool(false))?;
                    if index.strict_equals(&Js::Number(0.0)) {
                        pending.call_method("delete", vec![key])?;
                    }
                }
                Err(error)
            }
        }
    })();
    group.set("running", Js::Undefined)?;
    result
}

async fn submit_one(rt: &Rt, owner: Js, create: Js, brand: Js) -> Result<Js, Thrown> {
    let payload = create.get("payload")?;
    let mut attempts = 0;
    let mut cause = Js::str("submission could not be confirmed");
    while !owner.call_method("isDisposed", vec![])?.truthy() {
        if create.get("uncertain")?.truthy() {
            let boundary = owner.call_method("getPublicationBoundary", vec![])?;
            let found = match await_js(&rt.exchange_runtime(), owner.call_method("findOrder", vec![payload.clone()])?).await
            {
                Ok(found) => found,
                Err(error) => return Err(unknown(rt, payload, error.value)?),
            };
            if found.truthy() {
                let prior = create.get("accepted")?;
                if prior.truthy()
                    && !found
                        .get("orderId")?
                        .strict_equals(&prior.get("data")?.get("result")?.get("orderId")?)
                {
                    return Err(unknown(
                        rt,
                        payload,
                        Js::str("reconciled identity differs from the acknowledged order"),
                    )?);
                }
                if !found
                    .get("orderStatus")?
                    .strict_equals(&Js::str("Rejected"))
                {
                    create.set("accepted", accepted(payload.clone(), found.get("orderId")?))?;
                }
            }
            if !boundary.strict_equals(&owner.call_method("getPublicationBoundary", vec![])?) {
                return Err(unknown(
                    rt,
                    payload,
                    Js::str(
                        "private session or authoritative orders snapshot changed during reconciliation",
                    ),
                )?);
            }
            if found.truthy() {
                if found
                    .get("orderStatus")?
                    .strict_equals(&Js::str("Rejected"))
                {
                    let reason = found.get("rejectReason")?;
                    let reason = if reason.truthy() {
                        reason
                    } else {
                        payload.get("orderLinkId")?
                    };
                    return Err(rejected(
                        &brand,
                        ops::add(&Js::str("HyroTrader order rejected: "), &reason)?,
                    )?);
                }
                let response = accepted(payload.clone(), found.get("orderId")?);
                response.set("recoveredOrder", found.clone())?;
                let status = found.get("orderStatus")?;
                response.set(
                    "terminal",
                    Js::Bool(
                        [
                            "Filled",
                            "Cancelled",
                            "Deactivated",
                            "PartiallyFilledCanceled",
                        ]
                        .iter()
                        .any(|candidate| status.strict_equals(&Js::str(candidate))),
                    ),
                )?;
                return Ok(ready(response, boundary));
            }
            let prior = create.get("accepted")?;
            if prior.truthy() {
                let response = prior.spread_object()?;
                response.set("reconciled", Js::Bool(true))?;
                return Ok(ready(response, boundary));
            }
            if attempts >= 2 {
                return Err(unknown(rt, payload, cause)?);
            }
            create.set("uncertain", Js::Bool(false))?;
        }
        if owner.call_method("isDisposed", vec![])?.truthy() {
            break;
        }
        owner.call_method("assertCanCreate", vec![])?;
        attempts += 1;
        let boundary = owner.call_method("getPublicationBoundary", vec![])?;
        let attempt: Result<Option<Js>, Thrown> = async {
            let xhr = owner.call_method("getClient", vec![])?;
            let response = await_js(&rt.exchange_runtime(), xhr.call_method("post", vec![endpoint(rt, "CREATE_ORDER")?, payload.clone(),
                Js::object(vec![("axios-retry", Js::object(vec![("retries", Js::Number(0.0))]))])])?).await?;
            let data = response.get("data")?;
            let http_status = response.get("status")?;
            let status = meta(rt, data.clone())?;
            let result = if data.is_nullish() { Js::Undefined } else { data.get("result")? };
            let id = if result.is_nullish() { Js::Undefined } else { result.get("orderId")? };
            if status.get("isSuccess")?.truthy() && matches!(id, Js::String(_)) && id.truthy() {
                let accepted = Js::object(vec![("data", data), ("payload", payload.clone())]);
                create.set("accepted", accepted.clone())?;
                if boundary.strict_equals(&owner.call_method("getPublicationBoundary", vec![])?) {
                    return Ok(Some(ready(accepted, boundary.clone())));
                }
                create.set("uncertain", Js::Bool(true))?;
                cause = Js::str("private session or authoritative orders snapshot changed before acknowledgement");
                return Ok(None);
            }
            let code = status.get("retCode")?;
            let ambiguous = duplicate(rt, &status)? || http_status.strict_equals(&Js::Number(409.0)) || http_status.strict_equals(&Js::Number(408.0))
                || (present(&http_status) && ge(&http_status, 500.0)?) || code.strict_equals(&Js::Number(408.0))
                || (present(&code) && ge(&code, 500.0)? && lt(&code, 600.0)?) || code.strict_equals(&Js::Number(10000.0)) || code.strict_equals(&Js::Number(10016.0));
            if !status.get("isSuccess")?.truthy() && present(&code) && !ambiguous {
                let message = status.get("retMsg")?;
                let message = if message.truthy() { message } else { ops::add(&ops::add(&Js::str("HyroTrader order rejected ("), &code)?, &Js::str(")"))? };
                return Err(rejected(&brand, message)?);
            }
            let message = status.get("retMsg")?;
            cause = if message.truthy() { message } else { Js::str("Malformed create response") };
            Ok(None)
        }.await;
        match attempt {
            Ok(Some(response)) => return Ok(response),
            Ok(None) => {}
            Err(error) => {
                if is_rejected(&brand, &error) {
                    return Err(error);
                }
                let response = if error.value.is_nullish() {
                    Js::Undefined
                } else {
                    error.value.get("response")?
                };
                let status = if response.is_nullish() {
                    Js::Undefined
                } else {
                    response.get("status")?
                };
                let data = if response.is_nullish() {
                    Js::Undefined
                } else {
                    response.get("data")?
                };
                let meta = meta(rt, data)?;
                if present(&status)
                    && ge(&status, 400.0)?
                    && lt(&status, 500.0)?
                    && !status.strict_equals(&Js::Number(408.0))
                    && !status.strict_equals(&Js::Number(409.0))
                    && !duplicate(rt, &meta)?
                {
                    let message = if matches!(&error.value, Js::Object(object) if matches!(object.class, "Error" | "AxiosError"))
                    {
                        error.value.get("message")?
                    } else {
                        ops::add(
                            &ops::add(&Js::str("HyroTrader request rejected ("), &status)?,
                            &Js::str(")"),
                        )?
                    };
                    return Err(rejected(&brand, message)?);
                }
                cause = error.value;
            }
        }
        create.set("uncertain", Js::Bool(true))?;
    }
    Err(unknown(
        rt,
        payload,
        Js::str("exchange was disposed during submission"),
    )?)
}
