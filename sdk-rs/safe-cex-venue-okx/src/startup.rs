use std::{future::Future, rc::Rc};

// V clones retain raw values and opaque identity; constructors are pure intrinsics.
// Named call capabilities own complete JS member-call expressions and sync throws.
// Async callers poll immediately to preserve initiation before the first await.
pub fn connect_private<V, E>(
    disposed: impl FnOnce() -> Result<bool, E>,
    route: impl FnOnce(&str) -> Result<V, E>,
    mut read: impl FnMut(&str) -> Result<V, E>,
    mut get: impl FnMut(&V, &str) -> Result<V, E>,
    truthy: impl Fn(&V) -> bool,
    equal: impl Fn(&V, &V) -> bool,
    undefined: impl FnOnce() -> V,
    mut call: impl FnMut(&str, &[&V], Option<&str>) -> Result<V, E>,
) -> Result<(), E> {
    if disposed()? {
        return Ok(());
    }
    let route = route("wsPrivate")?;
    let resolve = call("capture-resolve", &[], None)?;
    let direct = if truthy(&get(&route, "tunnelled")?) {
        get(&route, "host")?
    } else {
        read("endpoint")?
    };
    let relay = if truthy(&call("relay-enabled", &[], None)?) {
        get(&route, "relay")?
    } else {
        undefined()
    };
    let fallback = get(&route, "fallback")?;
    let base = call("resolve", &[&resolve, &direct, &relay, &fallback], None)?;
    if equal(&base, &get(&route, "fallback")?) || equal(&base, &get(&route, "relay")?) {
        let info: Result<(), E> = (|| {
            let emitter = call("capture-info", &[], None)?;
            let role = if equal(&base, &get(&route, "relay")?) {
                "relay"
            } else {
                "proxy"
            };
            call(
                "info",
                &[&emitter],
                Some(&format!("Using tealstreet okx private {role}")),
            )?;
            Ok(())
        })();
        match info {
            Ok(()) | Err(_) => {}
        }
    }
    let socket = call("create", &[&base], None)?;
    call("set-ws", &[&socket], None)?;
    for event in ["open", "message", "close"] {
        call("listen", &[], Some(event))?;
    }
    Ok(())
}

pub fn connect_business<V, E>(
    disposed: impl FnOnce() -> Result<bool, E>,
    route: impl FnOnce(&str) -> Result<V, E>,
    mut read: impl FnMut(&str) -> Result<V, E>,
    mut get: impl FnMut(&V, &str) -> Result<V, E>,
    truthy: impl Fn(&V) -> bool,
    equal: impl Fn(&V, &V) -> bool,
    undefined: impl FnOnce() -> V,
    mut call: impl FnMut(&str, &[&V], Option<&str>) -> Result<V, E>,
) -> Result<(), E> {
    if disposed()? {
        return Ok(());
    }
    let route = route("wsBusiness")?;
    let resolve = call("capture-resolve", &[], None)?;
    let direct = if truthy(&get(&route, "tunnelled")?) {
        get(&route, "host")?
    } else {
        read("endpoint")?
    };
    let relay = if truthy(&call("relay-enabled", &[], None)?) {
        get(&route, "relay")?
    } else {
        undefined()
    };
    let fallback = get(&route, "fallback")?;
    let base = call("resolve", &[&resolve, &direct, &relay, &fallback], None)?;
    if equal(&base, &get(&route, "fallback")?) || equal(&base, &get(&route, "relay")?) {
        let info: Result<(), E> = (|| {
            let emitter = call("capture-info", &[], None)?;
            let role = if equal(&base, &get(&route, "relay")?) {
                "relay"
            } else {
                "proxy"
            };
            call(
                "info",
                &[&emitter],
                Some(&format!("Using tealstreet okx business {role}")),
            )?;
            Ok(())
        })();
        match info {
            Ok(()) | Err(_) => {}
        }
    }
    let socket = call("create", &[&base], None)?;
    call("set-ws", &[&socket], None)?;
    for event in ["open", "message", "close"] {
        call("listen", &[], Some(event))?;
    }
    Ok(())
}

pub fn connect_public<'a, V: 'a, E: 'a>(
    disposed: impl FnOnce() -> Result<bool, E>,
    route: impl FnOnce(&str) -> Result<V, E>,
    direct_url: impl FnOnce() -> Result<V, E>,
    mut get: impl FnMut(&V, &str) -> Result<V, E>,
    equal: impl Fn(&V, &V) -> bool,
    mut call: impl FnMut(&str, &[&V], Option<&str>) -> Result<V, E>,
    mut map_and_assign: impl FnMut(&str, Rc<dyn Fn(V) -> Result<V, E> + 'a>) -> Result<(), E>,
    market_id: Rc<dyn Fn(&V) -> Result<V, E> + 'a>,
    index_ticker_id: Rc<dyn Fn(&V) -> Result<V, E> + 'a>,
    literal: Rc<dyn Fn(&str) -> V + 'a>,
    object: Rc<dyn Fn(Vec<(&'static str, V)>) -> V + 'a>,
) -> Result<(), E> {
    if disposed()? {
        return Ok(());
    }
    let route = route("wsPublic")?;
    let direct = direct_url()?;
    let resolve = call("capture-resolve", &[], None)?;
    let fallback = get(&route, "fallback")?;
    let base = call("resolve", &[&resolve, &direct, &fallback], None)?;
    if equal(&base, &get(&route, "fallback")?) {
        let info: Result<(), E> = (|| {
            let emitter = call("capture-info", &[], None)?;
            call(
                "info",
                &[&emitter],
                Some("Using tealstreet okx public proxy"),
            )?;
            Ok(())
        })();
        match info {
            Ok(()) | Err(_) => {}
        }
    }
    let socket = call("create", &[&base], None)?;
    call("set-ws", &[&socket], None)?;
    for channel in [
        "tickers",
        "mark-price",
        "index-tickers",
        "open-interest",
        "funding-rate",
    ] {
        let market_id = market_id.clone();
        let index_ticker_id = index_ticker_id.clone();
        let literal = literal.clone();
        let object = object.clone();
        map_and_assign(
            channel,
            Rc::new(move |market| {
                let channel_value = literal(channel);
                let id = if channel == "index-tickers" {
                    index_ticker_id(&market)?
                } else {
                    market_id(&market)?
                };
                Ok(object(vec![("channel", channel_value), ("instId", id)]))
            }),
        )?;
    }
    for event in ["open", "message", "close"] {
        call("listen", &[], Some(event))?;
    }
    Ok(())
}

pub async fn on_open_private<V, E, F: Future<Output = Result<V, E>>>(
    mut disposed: impl FnMut() -> Result<bool, E>,
    auth: impl FnOnce() -> Result<F, E>,
    truthy: impl FnOnce(&V) -> bool,
    emit_error: impl FnOnce(&E) -> Result<(), E>,
    mut invoke: impl FnMut(&str) -> Result<(), E>,
    reconnect_enabled: impl FnOnce() -> Result<bool, E>,
) -> Result<(), E> {
    if disposed()? {
        return Ok(());
    }
    let authed = match async { auth()?.await }.await {
        Ok(value) => truthy(&value),
        Err(error) => {
            emit_error(&error)?;
            false
        }
    };
    if disposed()? {
        return Ok(());
    }
    if !authed {
        invoke("disregardSilence")?;
        invoke("reconnect")?;
        return Ok(());
    }
    invoke("ping")?;
    if reconnect_enabled()? {
        invoke("private-snapshot")?;
    }
    Ok(())
}

pub async fn on_open_business<V, E, F: Future<Output = Result<V, E>>>(
    mut disposed: impl FnMut() -> Result<bool, E>,
    auth: impl FnOnce() -> Result<F, E>,
    truthy: impl FnOnce(&V) -> bool,
    emit_error: impl FnOnce(&E) -> Result<(), E>,
    mut invoke: impl FnMut(&str) -> Result<(), E>,
    reconnect_enabled: impl FnOnce() -> Result<bool, E>,
) -> Result<(), E> {
    if disposed()? {
        return Ok(());
    }
    let authed = match async { auth()?.await }.await {
        Ok(value) => truthy(&value),
        Err(error) => {
            emit_error(&error)?;
            false
        }
    };
    if disposed()? {
        return Ok(());
    }
    if !authed {
        invoke("disregardSilence")?;
        invoke("reconnect")?;
        return Ok(());
    }
    invoke("ping")?;
    if reconnect_enabled()? {
        invoke("emitWsReconnected")?;
    }
    Ok(())
}

pub async fn auth_private<V: Clone, E, F: Future<Output = Result<V, E>>>(
    mut read: impl FnMut(&str) -> Result<V, E>,
    mut get: impl FnMut(&V, &str) -> Result<V, E>,
    truthy: impl Fn(&V) -> bool,
    mut call: impl FnMut(&str, &[&V], Option<&str>) -> Result<V, E>,
    sign: impl FnOnce(V, V) -> Result<F, E>,
    mut template: impl FnMut(&V) -> Result<Vec<u16>, E>,
    mut literal: impl FnMut(&str) -> V,
    mut string: impl FnMut(Vec<u16>) -> V,
    mut object: impl FnMut(Vec<(&'static str, V)>) -> V,
    mut array: impl FnMut(Vec<V>) -> V,
) -> Result<bool, E> {
    let timestamp = read("timestamp")?;
    let remote = truthy(&read("remoteSigner")?);
    let result_or_signature = if remote {
        let signer = call("capture-sign", &[], None)?;
        let id = read("nanoid")?;
        let request_type = literal("ws-auth");
        let name = read("name")?;
        let account = read("remote-account")?;
        let mut to_sign = template(&timestamp)?;
        to_sign.extend("GET/users/self/verify".encode_utf16());
        let to_sign = string(to_sign);
        let output = literal("base64");
        let extra = object(vec![("timestamp", timestamp.clone())]);
        let ws_auth = object(vec![
            ("toSign", to_sign),
            ("outputFormat", output),
            ("extraData", extra),
        ]);
        let request = object(vec![
            ("id", id),
            ("type", request_type),
            ("exchangeName", name),
            ("accountName", account),
            ("wsAuth", ws_auth),
        ]);
        let result = sign(signer, request)?.await?;
        if truthy(&get(&result, "error")?) {
            let emit = call("capture-error", &[], None)?;
            let error = get(&result, "error")?;
            let mut message = "Remote WS signing unavailable: "
                .encode_utf16()
                .collect::<Vec<_>>();
            message.extend(template(&error)?);
            let message = string(message);
            call("emit-error", &[&emit, &message], None)?;
            return Ok(false);
        }
        result
    } else {
        call("hmac", &[&timestamp], Some("GET/users/self/verify"))?
    };
    let send = call("capture-send", &[], None)?;
    let stringify = call("capture-json", &[], None)?;
    let key = read("key")?;
    let passphrase = read("passphrase")?;
    let signature = if remote {
        get(&result_or_signature, "wsAuthSignature")?
    } else {
        result_or_signature
    };
    let argument = object(vec![
        ("apiKey", key),
        ("passphrase", passphrase),
        ("timestamp", timestamp),
        ("sign", signature),
    ]);
    let op = literal("login");
    let args = array(vec![argument]);
    let frame = object(vec![("op", op), ("args", args)]);
    let wire = call("stringify", &[&stringify, &frame], None)?;
    call("send", &[&send, &wire], None)?;
    Ok(true)
}

pub async fn auth_business<V: Clone, E, F: Future<Output = Result<V, E>>>(
    mut read: impl FnMut(&str) -> Result<V, E>,
    mut get: impl FnMut(&V, &str) -> Result<V, E>,
    truthy: impl Fn(&V) -> bool,
    mut call: impl FnMut(&str, &[&V], Option<&str>) -> Result<V, E>,
    sign: impl FnOnce(V, V) -> Result<F, E>,
    mut template: impl FnMut(&V) -> Result<Vec<u16>, E>,
    mut literal: impl FnMut(&str) -> V,
    mut string: impl FnMut(Vec<u16>) -> V,
    mut object: impl FnMut(Vec<(&'static str, V)>) -> V,
    mut array: impl FnMut(Vec<V>) -> V,
) -> Result<bool, E> {
    let timestamp = read("timestamp")?;
    let remote = truthy(&read("remoteSigner")?);
    let result_or_signature = if remote {
        let signer = call("capture-sign", &[], None)?;
        let id = read("nanoid")?;
        let request_type = literal("ws-auth");
        let name = read("name")?;
        let account = read("remote-account")?;
        let mut to_sign = template(&timestamp)?;
        to_sign.extend("GET/users/self/verify".encode_utf16());
        let to_sign = string(to_sign);
        let output = literal("base64");
        let extra = object(vec![("timestamp", timestamp.clone())]);
        let ws_auth = object(vec![
            ("toSign", to_sign),
            ("outputFormat", output),
            ("extraData", extra),
        ]);
        let request = object(vec![
            ("id", id),
            ("type", request_type),
            ("exchangeName", name),
            ("accountName", account),
            ("wsAuth", ws_auth),
        ]);
        let result = sign(signer, request)?.await?;
        if truthy(&get(&result, "error")?) {
            let emit = call("capture-error", &[], None)?;
            let error = get(&result, "error")?;
            let mut message = "Remote WS signing unavailable: "
                .encode_utf16()
                .collect::<Vec<_>>();
            message.extend(template(&error)?);
            let message = string(message);
            call("emit-error", &[&emit, &message], None)?;
            return Ok(false);
        }
        result
    } else {
        call("hmac", &[&timestamp], Some("GET/users/self/verify"))?
    };
    let send = call("capture-send", &[], None)?;
    let stringify = call("capture-json", &[], None)?;
    let key = read("key")?;
    let passphrase = read("passphrase")?;
    let signature = if remote {
        get(&result_or_signature, "wsAuthSignature")?
    } else {
        result_or_signature
    };
    let argument = object(vec![
        ("apiKey", key),
        ("passphrase", passphrase),
        ("timestamp", timestamp),
        ("sign", signature),
    ]);
    let op = literal("login");
    let args = array(vec![argument]);
    let frame = object(vec![("op", op), ("args", args)]);
    let wire = call("stringify", &[&stringify, &frame], None)?;
    call("send", &[&send, &wire], None)?;
    Ok(true)
}

pub fn create_order_channels<S, E>(new_set: impl FnOnce(&[&str]) -> Result<S, E>) -> Result<S, E> {
    new_set(&["orders", "orders-algo", "algo-advance"])
}
