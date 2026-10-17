//! Native SDK primitives; source initialization/JWT/signing workflows remain in their modules.
use super::{Rt, host::Host, text};
use crate::auth_initialization::{AuthConstruction as C, AuthOperation as Op};
use safe_cex_exchange::js_runtime::{
    promise::await_js,
    value::{Js, Thrown, arg, from_typed, to_json},
};
use std::rc::Rc;
fn failure(e: impl ToString) -> Thrown {
    Thrown::error("Error", &e.to_string())
}
pub(super) fn fetch_function(rt: &Rt) -> Js {
    let weak = Rc::downgrade(&rt.shared);
    Js::function("fetch", move |_, a| {
        let rt = super::scope(&weak, Js::Undefined)?;
        let url = arg(&a, 0);
        let init = arg(&a, 1);
        let config = Js::object(vec![
            ("url", url),
            (
                "method",
                if init.get("method").is_ok_and(|v| v.truthy()) {
                    init.get("method")?
                } else {
                    Js::str("GET")
                },
            ),
            (
                "headers",
                if init.is_nullish() {
                    Js::Undefined
                } else {
                    init.get("headers")?
                },
            ),
            ("responseType", Js::str("text")),
        ]);
        let send = rt.shared.http.send(config);
        let task = rt.clone();
        Ok(rt.spawn(Box::pin(async move {
            let r = send.await?;
            let body = r.get("data")?;
            let status = r.get("status")?;
            let numeric = status.to_number()?;
            let text_body = if body.is_string() {
                body.clone()
            } else {
                task.global("JSON.stringify")?
                    .call(Js::Undefined, vec![body.clone()])?
            };
            let for_json = text_body.clone();
            let for_text = text_body;
            let global = task.shared.globals.clone();
            Ok(Js::object(vec![
                ("ok", Js::Bool((200.0..300.0).contains(&numeric))),
                ("status", status),
                ("statusText", r.get("statusText")?),
                (
                    "json",
                    Js::function("json", move |_, _| {
                        global
                            .get("JSON")?
                            .call_method("parse", vec![for_json.clone()])
                    }),
                ),
                (
                    "text",
                    Js::function("text", move |_, _| Ok(for_text.clone())),
                ),
            ]))
        })))
    })
}
pub(super) fn typed_signer(rt: &Rt) -> Js {
    let weak = Rc::downgrade(&rt.shared);
    Js::function("signParadexTypedData", move |_, a| {
        let rt = super::scope(&weak, Js::Undefined)?;
        let task = rt.clone();
        Ok(rt.spawn(Box::pin(async move {
            crate::typed_data_signing::sign_paradex_typed_data(
                &Host::new(&task),
                &arg(&a, 0),
                &arg(&a, 1),
                &arg(&a, 2),
            )
            .await
        })))
    })
}
pub(super) fn starknet_module(rt: &Rt) -> Result<Js, Thrown> {
    let weak = Rc::downgrade(&rt.shared);
    Ok(Js::object(vec![(
        "typedData",
        Js::object(vec![(
            "getMessageHash",
            Js::function("getMessageHash", move |_, a| {
                let rt = super::scope(&weak, Js::Undefined)?;
                let raw = text(
                    &rt.global("JSON.stringify")?
                        .call(Js::Undefined, vec![arg(&a, 0)])?,
                )?;
                Ok(Js::str(
                    &crate::typed_data_signing::native_message_hash(&raw, &text(&arg(&a, 1))?)
                        .map_err(failure)?,
                ))
            }),
        )]),
    )]))
}

fn account(address: Js, key: String) -> Js {
    let own_address = address.clone();
    Js::object_with_class(
        "StarknetAccount",
        vec![
            ("address", address),
            (
                "signMessage",
                Js::function("signMessage", move |_, a| {
                    let raw = serde_json::to_string(&to_json(&arg(&a, 0))?).map_err(failure)?;
                    from_typed(
                        &crate::typed_data_signing::native_sign_message(
                            &raw,
                            &text(&own_address)?,
                            &key,
                        )
                        .map_err(failure)?,
                    )
                }),
            ),
        ],
    )
}
pub(super) fn construct(rt: &Rt, k: C, a: Vec<Js>) -> Result<Js, Thrown> {
    Ok(match k {
        C::Provider => Js::object_with_class("ParaclearProvider", vec![("config", arg(&a, 1))]),
        C::StarknetAccount => account(
            arg(&a, 2).call_method("toLowerCase", vec![])?,
            text(&arg(&a, 3))?,
        ),
        C::Wallet => {
            let key = text(&arg(&a, 0))?;
            safe_cex_crypto::wallet_utils::create_wallet_from_private_key(&key).map_err(failure)?;
            Js::object_with_class("Wallet", vec![("key", Js::str(&key))])
        }
        C::EthereumAdapter => arg(&a, 1),
        C::RequestSigner => {
            let o = crate::runtime_assembly::allocate_request_signer(&Host::new(rt), a.clone())?;
            crate::auth_operations::initialize_request_signer(
                &arg(&a, 0),
                (!arg(&a, 1).is_undefined()).then(|| arg(&a, 1)),
                || Ok::<_, Thrown>(Js::object(vec![])),
                |v| o.set("sdkAccount", v.clone()),
                |v| o.set("options", v),
            )?;
            o
        }
        C::JwtManager => {
            let o = crate::runtime_assembly::allocate_jwt_manager(&Host::new(rt), a.clone())?;
            crate::auth_operations::initialize_jwt_manager(
                &arg(&a, 0),
                arg(&a, 1).truthy(),
                arg(&a, 2),
                (!arg(&a, 3).is_undefined()).then(|| arg(&a, 3)),
                || Ok::<_, Thrown>(Js::object(vec![])),
                |v| o.set("sdkAccount", v.clone()),
                |v| o.set("options", v),
                |v| o.set("subkeyAddress", v),
                |v| v.get("account")?.get("address"),
                |v| o.set("mainAccountAddress", v),
                |v| o.set("apiBaseUrl", Js::str(v)),
            )?;
            o
        }
    })
}
pub(super) async fn auth_operation(rt: &Rt, op: Op, a: Vec<Js>) -> Result<Js, Thrown> {
    match op {
        Op::LoadSdk | Op::ImportAccount => Ok(Js::object_with_class("NativeParadexSdk", vec![])),
        Op::Config => {
            let env = text(&arg(&a, 1))?;
            let url = Js::str(&format!("https://api.{env}.paradex.trade/v1/system/config"));
            let response = await_js(fetch_function(rt).call(Js::Undefined, vec![url])?).await?;
            if !response.get("ok")?.truthy() {
                return Err(failure(format!(
                    "Failed to fetch Paradex config: {} {}",
                    text(&response.get("statusText")?)?,
                    text(&response.get("status")?)?
                )));
            }
            let raw = await_js(response.call_method("json", vec![])?).await?;
            let out = Js::object(vec![]);
            for (source, target) in [
                ("starknet_fullnode_rpc_url", "paradexFullNodeRpcUrl"),
                ("starknet_chain_id", "paradexChainId"),
                ("l1_chain_id", "ethereumChainId"),
                ("paraclear_account_hash", "paraclearAccountHash"),
                ("paraclear_account_proxy_hash", "paraclearAccountProxyHash"),
                ("paraclear_address", "paraclearAddress"),
                ("paraclear_decimals", "paraclearDecimals"),
            ] {
                out.define(target, raw.get(source)?)?;
            }
            let tokens = Js::object(vec![]);
            for token in super::primitives::iterate(&raw.get("bridged_tokens")?)? {
                let mapped = Js::object(vec![]);
                for (source, target) in [
                    ("name", "name"),
                    ("symbol", "symbol"),
                    ("decimals", "decimals"),
                    ("l1_token_address", "l1TokenAddress"),
                    ("l1_bridge_address", "l1BridgeAddress"),
                    ("l2_token_address", "l2TokenAddress"),
                    ("l2_bridge_address", "l2BridgeAddress"),
                ] {
                    mapped.define(target, token.get(source)?)?;
                }
                tokens.define_key(&token.get("symbol")?.to_js_string()?, mapped)?;
            }
            out.define("bridgedTokens", tokens)?;
            out.define(
                "starknetChainId",
                Js::str(if raw.get("l1_chain_id")?.strict_equals(&Js::str("1")) {
                    "SN_MAIN"
                } else {
                    "SN_SEPOLIA"
                }),
            )?;
            Ok(out)
        }
        Op::InitializeSdk => {
            crate::auth_initialization::initialize_paradex_sdk(&Host::new(rt), arg(&a, 0)).await
        }
        Op::GetAccount => await_js(rt.this.get("xhr")?.call_method("get", a)?).await,
        Op::GetToken => await_js(rt.this.get("jwtManager")?.call_method("getToken", vec![])?).await,
        Op::Wait => {
            rt.shared
                .runtime
                .sleep(arg(&a, 0).to_number()?.max(0.0) as u64)
                .await
                .map_err(Thrown::from)?;
            Ok(Js::Undefined)
        }
        Op::FromEthereum => ethereum_account(rt, arg(&a, 1)).await,
    }
}
async fn ethereum_account(_rt: &Rt, args: Js) -> Result<Js, Thrown> {
    use num_bigint::{BigInt, Sign};
    use starknet_crypto::Felt;
    let config = args.get("config")?;
    let key = text(&args.get("signer")?.get("key")?)?;
    let typed=serde_json::json!({"domain":{"name":"Paradex","chainId":text(&config.get("ethereumChainId")?)?,"version":"1"},"primaryType":"Constant","types":{"EIP712Domain":[{"name":"name","type":"string"},{"name":"version","type":"string"},{"name":"chainId","type":"uint256"}],"Constant":[{"name":"action","type":"string"}]},"message":{"action":"STARK Key"}}).to_string();
    let seed = safe_cex_crypto::eip712::sign_typed_data(&typed, &key).map_err(failure)?;
    let additional = safe_cex_crypto::eip712::sign_typed_data(&typed, &key).map_err(failure)?;
    if seed != additional {
        return Err(failure(
            "Wallet does not support deterministic signing. Please use different wallet.",
        ));
    }
    let sig = seed.strip_prefix("0x").unwrap_or(&seed);
    if sig.len() != 130 || !sig.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(failure("Invalid Ethereum signature"));
    }
    let bytes = sig.as_bytes()[..64]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|b| u8::from_str_radix(std::str::from_utf8(b).unwrap(), 16).map_err(failure))
        .collect::<Result<Vec<_>, _>>()?;
    let order = BigInt::parse_bytes(
        b"3618502788666131213697322783095070105526743751716087489154079457884512865583",
        10,
    )
    .unwrap();
    let max = BigInt::from(1u8) << 256;
    let allowed = &max - (&max % &order);
    let mut index = 0u64;
    let private = loop {
        let mut input = bytes.clone();
        let counter = format!("{index:02x}");
        let counter = if counter.len() % 2 == 1 {
            format!("0{counter}")
        } else {
            counter
        };
        for pair in counter.as_bytes().as_chunks::<2>().0 {
            input
                .push(u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).map_err(failure)?);
        }
        let hash = BigInt::from_bytes_be(Sign::Plus, &safe_cex_crypto::sha256(&input));
        if hash < allowed {
            break hash % &order;
        }
        index += 1;
    };
    let private_hex = format!("0x{}", private.to_str_radix(16));
    let felt = safe_cex_crypto::starknet::parse_felt(&private_hex).map_err(failure)?;
    let public = starknet_crypto::get_public_key(&felt);
    let class = safe_cex_crypto::starknet::parse_felt(&text(&config.get("paraclearAccountHash")?)?)
        .map_err(failure)?;
    let proxy =
        safe_cex_crypto::starknet::parse_felt(&text(&config.get("paraclearAccountProxyHash")?)?)
            .map_err(failure)?;
    let selector = starknet_core::utils::get_selector_from_name("initialize").map_err(failure)?;
    let address = starknet_core::utils::get_contract_address(
        public,
        proxy,
        &[class, selector, Felt::TWO, public, Felt::ZERO],
        Felt::ZERO,
    );
    Ok(account(Js::str(&format!("{address:#x}")), private_hex))
}
pub(super) async fn dispatch(rt: &Rt, n: &str, a: Vec<Js>, _c: Vec<Js>) -> Result<Js, Thrown> {
    match n {
        "signOrder" => sign_order(rt, arg(&a, 0), false).await,
        "signModifyOrder" => sign_order(rt, arg(&a, 0), true).await,
        "signRequest" => {
            crate::account_rest::sign_request(
                &Host::new(rt),
                arg(&a, 0),
                arg(&a, 1),
                arg(&a, 2),
                arg(&a, 3),
            )
            .await
        }
        "getToken" => {
            let o = &rt.this;
            crate::auth_operations::get_token(
                || Ok(rt.now().floor()),
                || Ok(o.get("token")?.truthy()),
                || o.get("tokenExpiry")?.to_number(),
                || o.get("token"),
                || Ok(o.get("refreshPromise")?.truthy()),
                || o.call_method("generateNewToken", vec![]),
                |v| o.set("refreshPromise", v),
                || Ok::<_, Thrown>(async move { await_js(o.get("refreshPromise")?).await }),
                |v| o.set("token", v),
                |v| o.set("tokenExpiry", Js::Number(v)),
                |p, e| {
                    rt.global("console.error")?
                        .call(Js::Undefined, vec![Js::str(p), e.value.clone()])?;
                    Ok(())
                },
                || o.set("refreshPromise", Js::Null),
            )
            .await
        }
        _ => Err(Thrown::type_error(&format!(
            "Paradex member {n} is not bound"
        ))),
    }
}
pub(super) fn quantum(rt: &Rt, value: &Js, field: &Js) -> Result<Js, Thrown> {
    crate::signing_operations::to_chain_quantum(
        &value.to_js_string()?,
        &field.to_js_string()?,
        |source| {
            let number = rt
                .global("BigNumber")?
                .call(Js::Undefined, vec![Js::utf16(source)])?;
            let negative_zero = String::from_utf16_lossy(source)
                .trim_start()
                .starts_with('-')
                && number.call_method("eq", vec![Js::Number(0.)])?.truthy();
            Ok::<_, Thrown>((number, negative_zero))
        },
        |(number, _)| Ok(number.call_method("isFinite", vec![])?.truthy()),
        |(number, _)| Ok(number.call_method("isNaN", vec![])?.truthy()),
        |(number, negative_zero)| {
            Ok(*negative_zero || number.call_method("lt", vec![Js::Number(0.)])?.truthy())
        },
        |(number, _), factor| number.call_method("multipliedBy", vec![Js::Number(factor)]),
        |number, rounding| {
            debug_assert_eq!(rounding, 1);
            number.call_method("dividedToIntegerBy", vec![Js::Number(1.)])
        },
        |number, places| number.call_method("toFixed", vec![Js::Number(places as f64)]),
        |message| Ok::<_, Thrown>(Thrown::error("Error", &String::from_utf16_lossy(&message))),
    )
}

fn signing_data(data: crate::signing_operations::OrderSigningData) -> Js {
    let fields = |f: Vec<(&str, &str)>| {
        Js::array(
            f.into_iter()
                .map(|(n, t)| Js::object(vec![("name", Js::str(n)), ("type", Js::str(t))]))
                .collect(),
        )
    };
    let message = Js::object(vec![
        ("timestamp", Js::utf16(&data.timestamp)),
        ("market", Js::utf16(&data.market)),
        ("side", Js::str(data.side)),
        ("orderType", Js::utf16(&data.order_type)),
        ("size", Js::utf16(&data.size)),
        ("price", Js::utf16(&data.price)),
    ]);
    if let Some(id) = data.id {
        message
            .define("id", Js::utf16(&id))
            .expect("own message object");
    }
    Js::object(vec![
        (
            "domain",
            Js::object(vec![
                ("name", Js::str(data.domain_name)),
                ("chainId", Js::utf16(&data.chain_id)),
                ("version", Js::str(data.domain_version)),
            ]),
        ),
        (
            "types",
            Js::object(vec![
                (
                    "StarkNetDomain",
                    fields(data.domain_fields.into_iter().collect()),
                ),
                (data.primary_type, fields(data.message_fields)),
            ]),
        ),
        ("primaryType", Js::str(data.primary_type)),
        ("message", message),
    ])
}
async fn sign_order(rt: &Rt, p: Js, modify: bool) -> Result<Js, Thrown> {
    let chain = || {
        crate::account_rest::to_chain_id_hex(
            &Host::new(rt),
            rt.this
                .get("sdkAccount")?
                .get("config")?
                .get("paradexChainId")?,
        )
        .and_then(|v| v.to_js_string().map(|s| s.to_vec()))
    };
    let q = |key, field| {
        quantum(rt, &p.get(key)?, &Js::str(field))?
            .to_js_string()
            .map(|s| s.to_vec())
    };
    let sign = |d| {
        let args = vec![
            rt.this.get("sdkAccount")?,
            signing_data(d),
            rt.this.get("options")?,
        ];
        let promise = typed_signer(rt).call(Js::Undefined, args)?;
        Ok::<_, Thrown>(async move { await_js(promise).await })
    };
    let read = |r: Js| Ok::<_, Thrown>((r.get("signature")?, r.get("headers")?));
    let signed = if modify {
        crate::signing_operations::sign_modify_order(
            || Ok(p.get("id")?.truthy()),
            |m| Ok::<_, Thrown>(failure(m)),
            || Ok(rt.now().floor()),
            || Ok(p.get("side")?.strict_equals(&Js::str("BUY"))),
            || q("size", "size"),
            || q("price", "price"),
            chain,
            |n| Ok(safe_cex_num::js_number_string(n).encode_utf16().collect()),
            || p.get("market")?.to_js_string().map(|s| s.to_vec()),
            || p.get("type")?.to_js_string().map(|s| s.to_vec()),
            || p.get("id")?.to_js_string().map(|s| s.to_vec()),
            sign,
            read,
        )
        .await?
    } else {
        crate::signing_operations::sign_order(
            || Ok(rt.now().floor()),
            || Ok(p.get("side")?.strict_equals(&Js::str("BUY"))),
            || q("size", "size"),
            || p.get("type")?.to_js_string().map(|s| s.to_vec()),
            || Ok(p.get("price")?.truthy()),
            || q("price", "price"),
            chain,
            |n| Ok(safe_cex_num::js_number_string(n).encode_utf16().collect()),
            || p.get("market")?.to_js_string().map(|s| s.to_vec()),
            sign,
            read,
            |prefix, e| {
                rt.global("console.error")?
                    .call(Js::Undefined, vec![Js::str(prefix), e.value.clone()])?;
                Ok(())
            },
        )
        .await?
    };
    Ok(Js::object(vec![
        ("signature", signed.signature),
        (
            "signature_timestamp",
            Js::Number(signed.signature_timestamp),
        ),
        ("headers", signed.headers),
    ]))
}
