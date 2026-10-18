//! HyroTrader order observation, reconciliation, publication and cancellation.
use crate::{
    hyro_private_session::SessionFuture, variant_account_refresh::AccountRefreshHost as Host,
};
macro_rules! call {
    ($h:expr,$owner:expr,$key:expr,$args:expr) => {{
        let selected = $h.select($owner, $key)?;
        $h.apply(selected, $args)?
    }};
}
fn eager<'a, V: 'a, E: 'a>(mut future: SessionFuture<'a, V, E>) -> SessionFuture<'a, V, E> {
    let mut context = std::task::Context::from_waker(std::task::Waker::noop());
    match future.as_mut().poll(&mut context) {
        std::task::Poll::Ready(result) => Box::pin(async move { result }),
        std::task::Poll::Pending => future,
    }
}
fn concat<H: Host>(h: &H, parts: Vec<H::Value>) -> Result<H::Value, H::Error> {
    let mut units = vec![];
    for part in parts {
        units.extend(h.units(&part)?);
    }
    Ok(h.text(units))
}
fn endpoint<H: Host>(h: &H, key: &str) -> Result<H::Value, H::Error> {
    h.get(&h.intrinsic("ENDPOINTS", vec![])?, key)
}
fn iterator<H: Host>(h: &H, value: H::Value) -> Result<(H::Value, H::Value), H::Error> {
    let iter = h.intrinsic("iterator", vec![value])?;
    let next = h.select(iter.clone(), "next")?;
    Ok((iter, next))
}
fn next<H: Host>(h: &H, next: H::Value) -> Result<Option<H::Value>, H::Error> {
    let v = h.apply(next, vec![])?;
    if h.truthy(&h.get(&v, "done")?) {
        Ok(None)
    } else {
        Ok(Some(h.get(&v, "value")?))
    }
}
fn walk<H: Host, F: FnMut(H::Value) -> Result<(), H::Error>>(
    h: &H,
    value: H::Value,
    mut body: F,
) -> Result<(), H::Error> {
    let (iter, step) = iterator(h, value)?;
    while let Some(value) = next(h, step.clone())? {
        if let Err(error) = body(value) {
            return match h.intrinsic(
                "IteratorClose abrupt",
                vec![iter, h.error_value(error.clone())],
            ) {
                Err(original) => Err(original),
                Ok(_) => Err(error),
            };
        }
    }
    Ok(())
}
fn string<H: Host>(h: &H, value: H::Value) -> Result<bool, H::Error> {
    Ok(h.equal(&h.intrinsic("typeof", vec![value])?, &h.literal("string")))
}
fn meta<H: Host>(h: &H, data: H::Value) -> Result<H::Value, H::Error> {
    Ok(call!(h, h.global(), "getBybitResponseMeta", vec![data]))
}
fn emit<H: Host>(h: &H, owner: H::Value, error: H::Error) -> Result<(), H::Error> {
    let emit = h.select(owner, "emitAndLogError")?;
    let normalized = h.normalize_error(error)?;
    h.apply(emit, vec![normalized])?;
    Ok(())
}
/// Closed set of lexical callbacks created by this module and selected by the native host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Callback {
    RecoveryLink,
    ReconciliationLink,
    ExistingOrder,
    CancelChild,
    AcceptedOrder,
}
impl Callback {
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "recovery-link" => Some(Self::RecoveryLink),
            "reconciliation-link" => Some(Self::ReconciliationLink),
            "existing-order" => Some(Self::ExistingOrder),
            "cancel-child" => Some(Self::CancelChild),
            "accepted-order" => Some(Self::AcceptedOrder),
            _ => None,
        }
    }
}

pub fn dispatch<H: Host>(
    h: &H,
    owner: &H::Value,
    code: Callback,
    captures: &[H::Value],
    args: Vec<H::Value>,
) -> Result<H::Value, H::Error> {
    let value = args.first().cloned().unwrap_or_else(|| h.undefined());
    match code {
        Callback::RecoveryLink => Ok(h.boolean(h.equal(
            &h.get(&h.get(&value, "payload")?, "orderLinkId")?,
            &h.get(&captures[0], "orderLinkId")?,
        ))),
        Callback::ReconciliationLink => Ok(h.boolean(h.equal(
            &h.get(&value, "orderLinkId")?,
            &h.get(&captures[0], "orderLinkId")?,
        ))),
        Callback::ExistingOrder => Ok(h.boolean(h.equal(&h.get(&value, "id")?, &captures[0]))),
        Callback::CancelChild => {
            let id = h.get(&value, "id")?;
            let left = concat(
                h,
                vec![h.get(&captures[0], "id")?, h.literal("__stop_loss")],
            )?;
            if h.equal(&id, &left) {
                return Ok(h.boolean(true));
            }
            let right = concat(
                h,
                vec![h.get(&captures[0], "id")?, h.literal("__take_profit")],
            )?;
            Ok(h.boolean(h.equal(&h.get(&value, "id")?, &right)))
        }
        Callback::AcceptedOrder => {
            h.set(&value, "recorded", h.boolean(true))?;
            let id = h.get(&h.get(&h.get(&value, "data")?, "result")?, "orderId")?;
            if h.truthy(&h.get(owner, "isDisposed")?)
                || h.truthy(&call!(h, captures[0].clone(), "has", vec![id.clone()]))
                || h.truthy(&h.get(&value, "authoritativeSeen")?)
            {
                return Ok(h.undefined());
            }
            if h.truthy(&h.get(&value, "terminal")?) {
                let remove = h.select(h.get(owner, "store")?, "removeOrders")?;
                let ids = h.array(vec![
                    h.object(vec![("id", id.clone())]),
                    h.object(vec![(
                        "id",
                        concat(h, vec![id.clone(), h.literal("__stop_loss")])?,
                    )]),
                    h.object(vec![(
                        "id",
                        concat(h, vec![id, h.literal("__take_profit")])?,
                    )]),
                ]);
                h.apply(remove, vec![ids])?;
            } else if h.truthy(&h.get(&value, "recoveredOrder")?) {
                let add = h.select(h.get(owner, "store")?, "addOrUpdateOrders")?;
                let map = h.select(owner.clone(), "mapValidatedOrders")?;
                let rows = h.array(vec![h.get(&value, "recoveredOrder")?]);
                h.apply(add, vec![h.apply(map, vec![rows])?])?;
            } else if !h.truthy(&h.get(&value, "reconciled")?) {
                let store = h.get(owner, "store")?;
                let some = h.select(h.get(&store, "orders")?, "some")?;
                let exists = h.apply(
                    some,
                    vec![h.callback(owner.clone(), "existing-order", vec![id.clone()])?],
                )?;
                if !h.truthy(&exists) {
                    let record = h.select(owner.clone(), "recordRecentlyPlacedOrderFromOpts")?;
                    h.apply(
                        record,
                        vec![id, captures[1].clone(), h.get(&value, "payload")?],
                    )?;
                }
            }
            Ok(h.undefined())
        }
    }
}
pub fn observe_orders<H: Host>(
    h: &H,
    owner: H::Value,
    rows: H::Value,
) -> Result<H::Value, H::Error> {
    walk(h, rows, |row| {
        if !string(h, h.get(&row, "orderLinkId")?)? {
            return Ok(());
        }
        let groups = call!(h, h.get(&owner, "pending")?, "values", vec![]);
        walk(h, groups, |group| {
            let some = h.select(h.get(&group, "creates")?, "some")?;
            let callback = h.callback(owner.clone(), "recovery-link", vec![row.clone()])?;
            if h.truthy(&h.apply(some, vec![callback])?) {
                let add = h.select(h.get(&group, "observedLinkIds")?, "add")?;
                h.apply(add, vec![h.get(&row, "orderLinkId")?])?;
            }
            Ok(())
        })
    })?;
    Ok(h.undefined())
}
pub fn dispose<H: Host>(h: &H, owner: H::Value) -> Result<H::Value, H::Error> {
    call!(h, h.get(&owner, "pending")?, "clear", vec![]);
    Ok(h.undefined())
}
pub fn unknown<H: Host>(h: &H, payload: H::Value, cause: H::Value) -> Result<H::Value, H::Error> {
    let reason = if h.truthy(&h.intrinsic("instanceof Error", vec![cause.clone()])?) {
        h.get(&cause, "message")?
    } else {
        h.intrinsic("String", vec![cause])?
    };
    let message = concat(
        h,
        vec![
            h.literal("HyroTrader order outcome is unknown (orderLinkId "),
            h.get(&payload, "orderLinkId")?,
            h.literal("). Reconcile before placing a replacement: "),
            reason,
        ],
    )?;
    Ok(h.error_value(h.error(message)?))
}
pub fn run_group<H: Host>(
    h: &H,
    owner: H::Value,
    group: H::Value,
    on_accepted: H::Value,
) -> SessionFuture<'_, H::Value, H::Error> {
    eager(Box::pin(async move {
        let mut index = h.get(&h.get(&group, "responses")?, "length")?;
        while h.truthy(&h.intrinsic(
            "less than",
            vec![index.clone(), h.get(&h.get(&group, "creates")?, "length")?],
        )?) {
            let create = h.intrinsic(
                "computed property",
                vec![h.get(&group, "creates")?, index.clone()],
            )?;
            let request = call!(h, owner.clone(), "submitOne", vec![create.clone()]);
            let found = h.wait(request).await?;
            let response = h.get(&found, "response")?;
            let boundary = h.get(&found, "boundary")?;
            if h.truthy(&call!(h, owner.clone(), "isDisposed", vec![]))
                || !h.equal(
                    &boundary,
                    &call!(h, owner.clone(), "getPublicationBoundary", vec![]),
                )
            {
                h.set(&create, "uncertain", h.boolean(true))?;
                let unknown = h.select(owner.clone(), "unknown")?;
                let error=h.apply(unknown,vec![h.get(&create,"payload")?,h.literal("private session or authoritative orders snapshot changed before publication")])?;
                return h.intrinsic("throw value", vec![error]);
            }
            let observed = h.select(h.get(&group, "observedLinkIds")?, "has")?;
            let link = h.get(&h.get(&response, "payload")?, "orderLinkId")?;
            let seen = h.apply(observed, vec![link])?;
            h.set(&response, "authoritativeSeen", seen)?;
            call!(
                h,
                h.get(&group, "responses")?,
                "push",
                vec![response.clone()]
            );
            h.receiver_call(h.undefined(), on_accepted.clone(), vec![response])?;
            index = h.number(h.numeric(&index)? + 1.0);
        }
        h.get(&group, "responses")
    }))
}
pub fn find_order<H: Host>(
    h: &H,
    owner: H::Value,
    payload: H::Value,
) -> SessionFuture<'_, H::Value, H::Error> {
    eager(Box::pin(async move {
        let endpoints = h.array(vec![
            endpoint(h, "UNFILLED_ORDERS")?,
            endpoint(h, "USER_ORDERS")?,
        ]);
        let (iter, step) = iterator(h, endpoints)?;
        while let Some(endpoint) = next(h, step.clone())? {
            let attempt: Result<Option<H::Value>, H::Error> = async {
                let mut cursor = h.literal("");
                let seen = h.intrinsic("new Set", vec![])?;
                for page in 0..100 {
                    if h.truthy(&call!(h, owner.clone(), "isDisposed", vec![])) {
                        return Err(
                            h.error(h.literal("exchange was disposed during reconciliation"))?
                        );
                    }
                    let client = call!(h, owner.clone(), "getClient", vec![]);
                    let get = h.select(client, "get")?;
                    let params = h.object(vec![
                        ("category", h.literal("linear")),
                        ("symbol", h.get(&payload, "symbol")?),
                        ("limit", h.number(50.0)),
                        ("cursor", cursor),
                    ]);
                    let request = h.apply(
                        get,
                        vec![endpoint.clone(), h.object(vec![("params", params)])],
                    )?;
                    let result = h.wait(request).await?;
                    let data = h.get(&result, "data")?;
                    let status = meta(h, data.clone())?;
                    if !h.truthy(&h.get(&status, "isSuccess")?)
                        || !h.truthy(&h.intrinsic(
                            "Array.isArray",
                            vec![h.optional(&h.optional(&data, "result")?, "list")?],
                        )?)
                    {
                        let reason = h.get(&status, "retMsg")?;
                        let message =
                            if h.truthy(&reason) && !h.truthy(&h.get(&status, "isSuccess")?) {
                                h.get(&status, "retMsg")?
                            } else {
                                h.literal("Malformed order reconciliation response")
                            };
                        return Err(h.error(message)?);
                    }
                    let list = h.get(&h.get(&data, "result")?, "list")?;
                    let find = h.select(list, "find")?;
                    let found = h.apply(
                        find,
                        vec![h.callback(
                            owner.clone(),
                            "reconciliation-link",
                            vec![payload.clone()],
                        )?],
                    )?;
                    if h.truthy(&found) {
                        let valid = string(h, h.get(&found, "orderId")?)?
                            && h.truthy(&h.get(&found, "orderId")?)
                            && h.truthy(&call!(
                                h,
                                h.array(
                                    [
                                        "New",
                                        "Created",
                                        "Untriggered",
                                        "Triggered",
                                        "PartiallyFilled",
                                        "Filled",
                                        "Cancelled",
                                        "Rejected",
                                        "Deactivated",
                                        "PartiallyFilledCanceled"
                                    ]
                                    .iter()
                                    .map(|s| h.literal(s))
                                    .collect()
                                ),
                                "includes",
                                vec![h.get(&found, "orderStatus")?]
                            ));
                        if !valid {
                            return Err(
                                h.error(h.literal("Matched order has no identity or status"))?
                            );
                        }
                        return Ok(Some(found));
                    }
                    let following = h.get(&h.get(&data, "result")?, "nextPageCursor")?;
                    if h.equal(&following, &h.undefined()) || h.equal(&following, &h.literal("")) {
                        break;
                    }
                    if !string(h, following.clone())?
                        || h.truthy(&call!(h, seen.clone(), "has", vec![following.clone()]))
                    {
                        return Err(
                            h.error(h.literal("Order reconciliation cursor did not advance"))?
                        );
                    }
                    call!(h, seen.clone(), "add", vec![following.clone()]);
                    cursor = following;
                    if page == 99 {
                        return Err(h.error(h.literal("Order reconciliation page limit exceeded"))?);
                    }
                }
                Ok(None)
            }
            .await;
            match attempt {
                Ok(Some(found)) => return h.intrinsic("IteratorClose normal", vec![iter, found]),
                Ok(None) => {}
                Err(error) => {
                    return match h.intrinsic(
                        "IteratorClose abrupt",
                        vec![iter, h.error_value(error.clone())],
                    ) {
                        Err(original) => Err(original),
                        Ok(_) => Err(error),
                    };
                }
            }
        }
        Ok(h.undefined())
    }))
}
pub fn observe_private_orders<H: Host>(
    h: &H,
    owner: H::Value,
    rows: H::Value,
) -> Result<H::Value, H::Error> {
    call!(
        h,
        h.get(&owner, "orderRecovery")?,
        "observeOrders",
        vec![rows.clone()]
    );
    walk(h, rows, |row| {
        if string(h, h.get(&row, "orderId")?)? {
            walk(
                h,
                h.get(&owner, "activeOrderObservations")?,
                |observations| {
                    let add = h.select(observations, "add")?;
                    h.apply(add, vec![h.get(&row, "orderId")?])?;
                    Ok(())
                },
            )?;
        }
        Ok(())
    })?;
    Ok(h.undefined())
}
pub fn submit_create_orders<H: Host>(
    h: &H,
    owner: H::Value,
    payloads: H::Value,
    opts: H::Value,
) -> SessionFuture<'_, H::Value, H::Error> {
    let initial: Result<(H::Value, Result<H::Value, H::Error>), H::Error> = (|| {
        let observations = h.intrinsic("new Set", vec![])?;
        call!(
            h,
            h.get(&owner, "activeOrderObservations")?,
            "add",
            vec![observations.clone()]
        );
        let request = (|| {
            let submit = h.select(h.get(&owner, "orderRecovery")?, "submit")?;
            let callback = h.callback(
                owner.clone(),
                "accepted-order",
                vec![observations.clone(), opts],
            )?;
            h.apply(submit, vec![payloads, callback])
        })();
        Ok((observations, request))
    })();
    eager(Box::pin(async move {
        let (observations, request) = initial?;
        let result = match request {
            Ok(value) => h.wait(value).await,
            Err(error) => Err(error),
        };
        let result = match result {
            Ok(value) => Ok(value),
            Err(error) => match emit(h, owner.clone(), error.clone()) {
                Ok(()) => Err(error),
                Err(emission) => Err(emission),
            },
        };
        call!(
            h,
            h.get(&owner, "activeOrderObservations")?,
            "delete",
            vec![observations]
        );
        result
    }))
}
pub fn cancel_orders_helper<H: Host>(
    h: &H,
    owner: H::Value,
    orders: H::Value,
    category: H::Value,
) -> SessionFuture<'_, H::Value, H::Error> {
    eager(Box::pin(async move {
        let (iter, step) = iterator(h, orders)?;
        while let Some(order) = next(h, step.clone())? {
            let run: Result<(), H::Error> = async {
                let post = h.select(h.get(&owner, "xhr")?, "post")?;
                let endpoint = endpoint(h, "CANCEL_ORDER")?;
                let payload = h.object(vec![
                    ("category", category.clone()),
                    ("symbol", h.get(&order, "symbol")?),
                    ("orderId", h.get(&order, "id")?),
                ]);
                let request = h.apply(post, vec![endpoint, payload])?;
                let result = h.wait(request).await?;
                let data = h.get(&result, "data")?;
                let status = meta(h, data)?;
                let regex = h.intrinsic(
                    "RegExp",
                    vec![
                        h.literal("order not exists|too late to cancel"),
                        h.literal("i"),
                    ],
                )?;
                let test = h.select(regex, "test")?;
                let msg = h.get(&status, "retMsg")?;
                let msg = if h.truthy(&msg) { msg } else { h.literal("") };
                let gone = h.apply(test, vec![msg])?;
                if !h.truthy(&h.get(&status, "isSuccess")?) && !h.truthy(&gone) {
                    let fail = h.select(owner.clone(), "fail")?;
                    let reason = h.get(&status, "retMsg")?;
                    let reason = if h.truthy(&reason) {
                        reason
                    } else {
                        h.literal("HyroTrader cancellation failed")
                    };
                    h.apply(fail, vec![reason])?;
                }
                let retire = h.select(owner.clone(), "retireCancelledOrders")?;
                let filter = h.select(h.get(&h.get(&owner, "store")?, "orders")?, "filter")?;
                let children = h.apply(
                    filter,
                    vec![h.callback(owner.clone(), "cancel-child", vec![order.clone()])?],
                )?;
                let (_, step) = iterator(h, children)?;
                let mut retiring = vec![order];
                while let Some(child) = next(h, step.clone())? {
                    retiring.push(child);
                }
                h.apply(retire, vec![h.array(retiring)])?;
                Ok(())
            }
            .await;
            if let Err(error) = run {
                let error = match emit(h, owner.clone(), error.clone()) {
                    Ok(()) => error,
                    Err(emission) => emission,
                };
                return match h.intrinsic(
                    "IteratorClose abrupt",
                    vec![iter, h.error_value(error.clone())],
                ) {
                    Err(original) => Err(original),
                    Ok(_) => Err(error),
                };
            }
        }
        Ok(h.undefined())
    }))
}
