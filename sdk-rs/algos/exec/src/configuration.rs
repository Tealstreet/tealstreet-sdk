//! Hosted mode has no account-config or interactive prompt mutation authority.
use crate::{Evaluation, failed, rows};
use crate::{display::dollars, placement::n};
use serde_json::{Value, json};
fn refusal(feature: &str) -> Evaluation {
    failed(format!("{feature} is not available in hosted mode"))
}
fn preview(symbol: &str, store: &Value) -> String {
    let market = rows(store, "markets")
        .iter()
        .find(|m| m["symbol"] == symbol);
    let precision = market.and_then(|m| m["precision"]["price"].as_f64());
    let price = rows(store, "tickers")
        .iter()
        .find(|t| t["symbol"] == symbol)
        .and_then(|t| t["last"].as_f64())
        .map(|p| dollars(p, precision))
        .unwrap_or_default();
    let account = store["accountName"].as_str().unwrap_or("");
    let position = rows(store, "positions")
        .iter()
        .find(|p| p["symbol"] == symbol && n(p, "contracts") != 0.);
    let pos = position
        .map(|p| {
            format!(
                " {}:{} {}",
                if p["side"] == "long" { "Long" } else { "Short" },
                n(p, "contracts").abs(),
                market
                    .map(|m| if m["contractSize"].as_f64().is_some_and(|s| s != 1.) {
                        "Contracts"
                    } else if m["isInverse"] == true {
                        m["quote"].as_str().unwrap_or("")
                    } else {
                        m["base"].as_str().unwrap_or("")
                    })
                    .unwrap_or("")
            )
        })
        .unwrap_or_default();
    let health = &store["socketHealth"];
    let status = if health["privateInitialized"] != true {
        "[UNKNOWN] "
    } else if health["privateHealthy"] == true {
        ""
    } else {
        "[DEGRADED] "
    };
    format!("Prompt preview:\n{status}[{account} {symbol} {price}{pos}]> ")
}
pub(crate) fn evaluate(symbol: &str, args: &[String], store: &Value) -> Option<Evaluation> {
    let message = match args[0].as_str() {
        "account" => {
            if args.len() > 1 {
                return Some(refusal("Account management"));
            }
            format!(
                "Current account: {}\nUsage: account <name> or account <number>\nUse \"accounts\" to list available accounts",
                store["accountName"].as_str().unwrap_or("Not connected")
            )
        }
        "accounts" => match args.get(1).map(String::as_str) {
            None | Some("list") => {
                "Warning: No accounts configured.\nUse \"accounts add\" to add an account.".into()
            }
            Some("enabled") => "No accounts currently enabled".into(),
            _ => return Some(refusal("Account management")),
        },
        "prompt" => {
            let presets: Value = serde_json::from_str(include_str!("prompt-presets.json"))
                .expect("generated presets");
            match args.get(1).map(String::as_str) {
                Some("preset" | "set" | "prefix") => return Some(refusal("Prompt mutation")),
                Some("list") => format!(
                    "Available prompt presets:\n{}\nUse: prompt preset <name>",
                    ["minimal", "default", "detailed", "trader", "compact"]
                        .into_iter()
                        .map(|name| {
                            let row = &presets[name];
                            format!(
                                "  {name:12} {} {}",
                                row["prefix"].as_str().unwrap(),
                                row["format"].as_str().unwrap()
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("\n")
                ),
                Some("preview") => preview(symbol, store),
                _ => format!(
                    "Prompt Configuration\nCurrent format:\n  Format: {}\n  Prefix: (none)\nAvailable tokens: {{symbol}} {{account}} {{price}} {{side}} {{size}} {{entry}} {{pnl}} {{pnl%}} {{liq}} {{leverage}} {{orders}} {{conn}}\nPrompt mutations are not available in hosted mode",
                    presets["default"]["format"].as_str().unwrap()
                ),
            }
        }
        "exchanges" => {
            let exchanges: Vec<Value> = serde_json::from_str(include_str!("exchanges.json"))
                .expect("generated exchange catalog");
            format!(
                "Supported Exchanges:\n{}",
                exchanges
                    .iter()
                    .map(|e| format!(
                        "  {:15} {}\n                 Required: {}",
                        e["name"].as_str().unwrap(),
                        e["displayName"].as_str().unwrap(),
                        e["requiredFields"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|v| v.as_str().unwrap())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ))
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        }
        "clear" => return Some(refusal("Terminal clear")),
        _ => return None,
    };
    Some(Evaluation::Text {
        message,
        result: json!({"handled":true,"success":true}),
    })
}
