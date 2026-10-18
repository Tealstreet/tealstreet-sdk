use super::Host;
use crate::{agent, browser_approval_flows as browser, builder_fee, constants};
use safe_cex_exchange::js_runtime::value::{Js, Thrown};
use std::rc::Rc;

fn arg(args: &[Js], index: usize) -> Js {
    args.get(index).cloned().unwrap_or(Js::Undefined)
}

impl agent::EthereumWindow for Host {
    type Error = Thrown;
    fn window_defined(&mut self) -> Result<bool, Thrown> {
        use browser::BrowserApprovalHost;
        Ok(!self
            .global_typeof("window")?
            .strict_equals(&Js::str("undefined")))
    }
    fn ethereum_truthy(&mut self) -> Result<bool, Thrown> {
        Ok(self.global_binding("window")?.get("ethereum")?.truthy())
    }
}

pub(super) fn bind(host: &Host) {
    host.bind_import(
        "TEALSTREET_BUILDER_FEE",
        Js::Number(constants::TEALSTREET_BUILDER_FEE),
    );
    host.bind_import(
        "DEFAULT_BUILDER_FEE_TENTHS_BPS",
        Js::Number(builder_fee::DEFAULT_BUILDER_FEE_TENTHS_BPS),
    );
    host.bind_import(
        "DEFAULT_BUILDER_FEE_RATE",
        Js::str(&builder_fee::DEFAULT_BUILDER_FEE_RATE),
    );
    for (name, domain) in [
        (
            "HYPERLIQUID_BUILDER_DOMAIN",
            builder_fee::HYPERLIQUID_BUILDER_DOMAIN,
        ),
        (
            "HYPERLIQUID_BUILDER_DOMAIN_TESTNET",
            builder_fee::HYPERLIQUID_BUILDER_DOMAIN_TESTNET,
        ),
    ] {
        host.bind_import(
            name,
            Js::object(vec![
                ("name", Js::str(domain.name)),
                ("version", Js::str(domain.version)),
                ("chainId", Js::Number(domain.chain_id)),
                ("verifyingContract", Js::str(domain.verifying_contract)),
            ]),
        );
    }
    for (name, types) in [
        ("APPROVE_AGENT_TYPES", agent::APPROVE_AGENT_TYPES),
        (
            "APPROVE_BUILDER_FEE_TYPES",
            builder_fee::APPROVE_BUILDER_FEE_TYPES,
        ),
    ] {
        let object = Js::object(vec![]);
        for (name, fields) in types {
            object
                .define(
                    name,
                    Js::array(
                        fields
                            .iter()
                            .map(|field| {
                                Js::object(vec![
                                    ("name", Js::str(field.name)),
                                    ("type", Js::str(field.r#type)),
                                ])
                            })
                            .collect(),
                    ),
                )
                .expect("fresh typed-data table");
        }
        host.bind_import(name, object);
    }
    host.bind_import(
        "hasEthereumWallet",
        host.native("hasEthereumWallet", 0, |host, _, _| {
            agent::has_ethereum_wallet(host).map(Js::Bool)
        }),
    );
    host.bind_import(
        "generateAgentWalletName",
        host.native("generateAgentWalletName", 0, |host, _, _| {
            agent::generate_agent_wallet_name(|| {
                let math = host.global_binding("Math")?;
                math.get("random")?.call(math, vec![])?.to_number()
            })
            .map(|name| Js::str(&name))
        }),
    );
    host.bind_import(
        "reportEvent",
        host.native("reportEvent", 2, |host, _, args| {
            let sink = host.shared.extra.telemetry.borrow().clone();
            if let Some(sink) = sink {
                let name = arg(args, 0).to_js_string()?;
                sink.event(&String::from_utf16_lossy(&name), Rc::new(arg(args, 1)))
                    .map_err(|cause| {
                        if let Some(cause) = cause.downcast_ref::<Thrown>() {
                            return cause.clone();
                        }
                        if let Some(cause) = cause.downcast_ref::<Js>() {
                            return Thrown::new(cause.clone());
                        }
                        Thrown::type_error(
                            "Telemetry sink must return its original native thrown value",
                        )
                    })?;
            }
            Ok(Js::Undefined)
        }),
    );
    let signature = host.object_from(vec![(
        "from",
        host.native("from", 1, |host, _, args| {
            super::signature::from(host, &arg(args, 0))
        }),
    )]);
    host.bind_import("ethers", host.object_from(vec![("Signature", signature)]));
    for module in ["agent", "builder"] {
        let lexical = host.with_module(module);
        let switch = lexical.native("switchToCorrectNetwork", 1, move |host, _, args| {
            let mut owned = host.clone();
            let network = arg(args, 0);
            Ok(host.spawn_value(Box::pin(async move {
                if module == "agent" {
                    browser::agent_switch_to_correct_network(&mut owned, &network).await
                } else {
                    browser::builder_switch_to_correct_network(&mut owned, &network).await
                }
            })))
        });
        host.shared
            .extra
            .browser_modules
            .borrow_mut()
            .insert(module, Js::object(vec![("switchToCorrectNetwork", switch)]));
    }
    for (name, module, length) in [
        ("signAgentApproval", "agent", 4),
        ("signBuilderFeeApproval", "builder", 2),
        ("connectBrowserWallet", "agent", 0),
        ("createAgentWalletWithBrowserWallet", "agent", 1),
        ("approveBuilderFeeWithBrowserWallet", "builder", 1),
    ] {
        let lexical = host.with_module(module);
        host.bind_import(
            name,
            lexical.native(name, length, move |host, _, args| {
                let mut owned = host.clone();
                let args = args.to_vec();
                Ok(host.spawn_value(Box::pin(async move {
                    match name {
                        "signAgentApproval" => {
                            browser::sign_agent_approval(
                                &mut owned,
                                &arg(&args, 0),
                                &arg(&args, 1),
                                &arg(&args, 2),
                                &arg(&args, 3),
                                &arg(&args, 4),
                            )
                            .await
                        }
                        "signBuilderFeeApproval" => {
                            browser::sign_builder_fee_approval(
                                &mut owned,
                                &arg(&args, 0),
                                &arg(&args, 1),
                                &arg(&args, 2),
                                &arg(&args, 3),
                                &arg(&args, 4),
                            )
                            .await
                        }
                        "connectBrowserWallet" => browser::connect_browser_wallet(&mut owned).await,
                        "createAgentWalletWithBrowserWallet" => {
                            browser::create_agent_wallet_with_browser_wallet(
                                &mut owned,
                                &arg(&args, 0),
                                &arg(&args, 1),
                                &arg(&args, 2),
                            )
                            .await
                        }
                        "approveBuilderFeeWithBrowserWallet" => {
                            browser::approve_builder_fee_with_browser_wallet(
                                &mut owned,
                                &arg(&args, 0),
                                &arg(&args, 1),
                                &arg(&args, 2),
                                &arg(&args, 3),
                            )
                            .await
                        }
                        _ => unreachable!(),
                    }
                })))
            }),
        );
    }
}
