//! Read-only CLI projections of the account's engine frames.
use crate::placement::{Placement, n};
use crate::{Evaluation, failed, resolve, rows};
use serde_json::{Value, json};
use std::sync::OnceLock;
fn catalog() -> &'static [Value] {
    static CATALOG: OnceLock<Value> = OnceLock::new();
    CATALOG
        .get_or_init(|| {
            serde_json::from_str(include_str!("catalog.json")).expect("generated command catalog")
        })
        .as_array()
        .unwrap()
}
pub(crate) fn canonical(name: &str) -> Option<&'static str> {
    catalog()
        .iter()
        .find(|c| {
            c["name"]
                .as_str()
                .is_some_and(|s| s.eq_ignore_ascii_case(name))
        })
        .or_else(|| {
            catalog().iter().rev().find(|c| {
                c["aliases"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|a| a.as_str().is_some_and(|s| s.eq_ignore_ascii_case(name)))
            })
        })
        .and_then(|c| c["name"].as_str())
}
fn s<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key].as_str().unwrap_or("")
}
fn table(headers: &[&str], rows: Vec<Vec<String>>, footer: Option<String>) -> String {
    let mut widths = headers.iter().map(|h| h.len()).collect::<Vec<_>>();
    for row in &rows {
        for (i, cell) in row.iter().enumerate() {
            widths[i] = widths[i].max(cell.chars().count());
        }
    }
    let pad = |row: &[String]| {
        row.iter()
            .enumerate()
            .map(|(i, c)| {
                format!(
                    "{c}{}",
                    "\u{a0}".repeat(widths[i].saturating_sub(c.chars().count()))
                )
            })
            .collect::<Vec<_>>()
            .join("\u{a0}\u{a0}")
    };
    let mut out = format!(
        "{}\n{}\n",
        pad(&headers.iter().map(|s| s.to_string()).collect::<Vec<_>>()),
        widths
            .iter()
            .map(|n| "-".repeat(*n))
            .collect::<Vec<_>>()
            .join("\u{a0}\u{a0}")
    );
    for row in rows {
        out.push_str(&pad(&row));
        out.push('\n');
    }
    if let Some(footer) = footer {
        out.push('\n');
        out.push_str(&footer);
        out.push('\n');
    }
    out
}
pub(crate) fn dollars(value: f64, precision: Option<f64>) -> String {
    let decimals = precision
        .filter(|p| p.is_finite() && *p > 0.)
        .map(|p| {
            // Matches the TS getSigDigitsFromPrecision used by formatPrice.
            let text = format!("{p:.16}");
            text.trim_matches('0')
                .chars()
                .filter(|c| c.is_ascii_digit())
                .count()
        })
        .unwrap_or(2)
        .min(16);
    let maximum = decimals.max(2);
    let rounded = format!("{:.*}", maximum, value.abs());
    let (whole, fraction) = rounded.split_once('.').unwrap_or((&rounded, ""));
    let grouped = whole
        .chars()
        .enumerate()
        .map(|(i, c)| {
            if i > 0 && (whole.len() - i).is_multiple_of(3) {
                format!(",{c}")
            } else {
                c.to_string()
            }
        })
        .collect::<String>();
    let fraction = fraction.trim_end_matches('0');
    let mut fraction = fraction.to_string();
    while fraction.len() < decimals {
        fraction.push('0');
    }
    format!(
        "{}${grouped}{}",
        if value.is_sign_negative() { "-" } else { "" },
        if fraction.is_empty() {
            String::new()
        } else {
            format!(".{fraction}")
        }
    )
}
fn money(value: f64) -> String {
    dollars(value, None)
}
fn market<'a>(store: &'a Value, symbol: &str) -> Option<&'a Value> {
    rows(store, "markets")
        .iter()
        .find(|m| m["symbol"] == symbol)
}
fn precision(store: &Value, symbol: &str, key: &str) -> Option<f64> {
    market(store, symbol).and_then(|m| m["precision"][key].as_f64())
}
fn orders(symbol: &str, all: bool, store: &Value) -> String {
    let selected = rows(store, "orders")
        .iter()
        .filter(|o| (all || o["symbol"] == symbol) && matches!(s(o, "status"), "open" | "creating"))
        .collect::<Vec<_>>();
    if selected.is_empty() {
        return if all {
            "No open orders".into()
        } else {
            format!("No open orders for {symbol}")
        };
    }
    table(
        &["Symbol", "Side", "Type", "Size", "Price", "Trigger"],
        selected
            .iter()
            .map(|o| {
                let sym = s(o, "symbol");
                let pp = precision(store, sym, "price");
                let kind = match s(o, "type") {
                    "limit" => "Limit",
                    "market" => "Market",
                    "stop_market" => "SL",
                    "take_profit_market" => "TP",
                    "trailing_stop_market" => "TSL",
                    other => other,
                };
                let trigger = if n(o, "takeProfit") != 0. {
                    n(o, "takeProfit")
                } else {
                    n(o, "stopLoss")
                };
                vec![
                    sym.into(),
                    s(o, "side").to_uppercase(),
                    kind.into(),
                    dollars(n(o, "amount"), precision(store, sym, "amount")),
                    if n(o, "price") != 0. {
                        dollars(n(o, "price"), pp)
                    } else {
                        "Market".into()
                    },
                    if trigger != 0. {
                        dollars(trigger, pp)
                    } else {
                        "-".into()
                    },
                ]
            })
            .collect(),
        None,
    )
}
fn positions(symbol: &str, all: bool, side: Option<&str>, store: &Value) -> String {
    let selected = rows(store, "positions")
        .iter()
        .filter(|p| {
            n(p, "notional") != 0.
                && (all || p["symbol"] == symbol)
                && side.is_none_or(|side| p["side"] == side)
        })
        .collect::<Vec<_>>();
    if selected.is_empty() {
        let side = side.map(|s| format!("{s} ")).unwrap_or_default();
        return if all {
            format!("No open {side}positions")
        } else {
            format!("No open {side}positions for {symbol}")
        };
    }
    let mut pnl = 0.;
    let total = n(&store["balance"], "total");
    let values = selected
        .iter()
        .map(|p| {
            let symbol = s(p, "symbol");
            let pp = precision(store, symbol, "price");
            let ticker = rows(store, "tickers")
                .iter()
                .find(|t| t["symbol"] == symbol);
            let mark = ticker
                .and_then(|t| t["mark"].as_f64().or_else(|| t["last"].as_f64()))
                .unwrap_or(0.);
            let upnl = n(p, "unrealizedPnl");
            pnl += upnl;
            let sign = if upnl >= 0. { "+" } else { "" };
            let percent = if n(p, "entryPrice") > 0. {
                upnl / n(p, "notional") * 100.
            } else {
                0.
            };
            let lev = if total > 0. {
                n(p, "notional").abs() / total
            } else {
                0.
            };
            vec![
                symbol.into(),
                s(p, "side").to_uppercase(),
                money(n(p, "notional").abs()),
                dollars(n(p, "entryPrice"), pp),
                dollars(mark, pp),
                format!("{sign}{} ({sign}{percent:.1}%)", money(upnl)),
                format!("{lev:.2}/{}x", n(p, "leverage")),
            ]
        })
        .collect();
    table(
        &["Symbol", "Side", "Size", "Entry", "Mark", "uPnL", "Lev"],
        values,
        Some(format!(
            "Total uPnL: {}{}",
            if pnl >= 0. { "+" } else { "" },
            money(pnl)
        )),
    )
}
fn position(symbol: &str, store: &Value) -> String {
    let Some(p) = rows(store, "positions")
        .iter()
        .find(|p| p["symbol"] == symbol && n(p, "notional") != 0.)
    else {
        return format!("No open position for {symbol}");
    };
    let ticker = rows(store, "tickers")
        .iter()
        .find(|t| t["symbol"] == symbol);
    let mark = ticker
        .and_then(|t| t["mark"].as_f64().or_else(|| t["last"].as_f64()))
        .unwrap_or(0.);
    let pp = precision(store, symbol, "price");
    let ap = precision(store, symbol, "amount");
    let size = n(p, "contracts").abs();
    let decimals = ap
        .map(|p| {
            format!("{p:.16}")
                .trim_matches('0')
                .chars()
                .filter(|c| c.is_ascii_digit())
                .count()
        })
        .unwrap_or(if size >= 1. { 2 } else { 4 })
        .min(16);
    let size = if size >= 1000000. {
        format!("{:.2}M", size / 1000000.)
    } else if size >= 10000. {
        format!("{:.1}k", size / 1000.)
    } else {
        format!("{size:.decimals$}")
    };
    let upnl = n(p, "unrealizedPnl");
    let sign = if upnl >= 0. { "+" } else { "" };
    let percent = upnl / n(p, "notional").abs() * 100.;
    let total = n(&store["balance"], "total");
    let account_lev = if total != 0. {
        n(p, "notional").abs() / total
    } else {
        0.
    };
    format!(
        "\n{line}\n{symbol}  {}\n{line}\nSize:   {size} ({})\nEntry:  {} → {}\nuPnL:   {sign}{} ({sign}{percent:.2}%)\nLiq:    {}\nLev:    {account_lev:.2}x / {}x\n{line}",
        s(p, "side").to_uppercase(),
        money(n(p, "notional").abs()),
        dollars(n(p, "entryPrice"), pp),
        dollars(mark, pp),
        money(upnl),
        if n(p, "liquidationPrice") > 0. {
            dollars(n(p, "liquidationPrice"), pp)
        } else {
            "N/A".into()
        },
        n(p, "leverage"),
        line = "─".repeat(40)
    )
}

fn margin(store: &Value) -> String {
    let balance = &store["balance"];
    if balance.is_null() {
        return "No balance data available".into();
    }
    let positions = rows(store, "positions")
        .iter()
        .filter(|p| n(p, "notional") != 0.);
    let (notional, pnl) = positions.fold((0., 0.), |(a, b), p| {
        (a + n(p, "notional").abs(), b + n(p, "unrealizedPnl"))
    });
    let total = n(balance, "total");
    let lev = if total > 0. { notional / total } else { 0. };
    table(
        &["Metric", "Value"],
        vec![
            vec!["Total Balance".into(), money(total)],
            vec!["Free Margin".into(), money(n(balance, "free"))],
            vec!["Used Margin".into(), money(n(balance, "used"))],
            vec!["Position Size".into(), money(notional)],
            vec!["Account Lev".into(), format!("{lev:.2}x")],
            vec!["Margin Ratio".into(), format!("{:.1}%", lev * 100.)],
            vec![
                "Unrealized PnL".into(),
                format!("{}{}", if pnl >= 0. { "+" } else { "" }, money(pnl)),
            ],
        ],
        None,
    )
}
fn spot(store: &Value) -> String {
    let coins = store.get("coins").or_else(|| store["balance"].get("coins"));
    let mut coins = match coins {
        Some(Value::Array(rows)) => rows.iter().collect::<Vec<_>>(),
        Some(Value::Object(rows)) => rows.values().collect::<Vec<_>>(),
        _ => return "Spot balances unknown / not yet available".into(),
    };
    coins.retain(|c| n(c, "notional") > 0.);
    coins.sort_by(|a, b| n(b, "notional").total_cmp(&n(a, "notional")));
    if coins.is_empty() {
        return "No spot balances\n".into();
    }
    let total = coins.iter().map(|c| n(c, "notional")).sum();
    table(
        &["Asset", "Amount", "Value (USD)"],
        coins
            .iter()
            .map(|c| {
                vec![
                    s(c, "base").into(),
                    dollars(n(c, "amount"), Some(4.)),
                    money(n(c, "notional")),
                ]
            })
            .collect(),
        Some(format!("Total: {}", money(total))),
    )
}
fn max(symbol: &str, args: &[String], store: &Value) -> Result<String, String> {
    let symbol = match args.get(1) {
        Some(s) => resolve(store, s).ok_or_else(|| format!("Unknown symbol \"{s}\""))?,
        None => symbol.into(),
    };
    let balance = &store["balance"];
    if balance.is_null() {
        return Err("Balance is not available yet, try again".into());
    }
    let last = rows(store, "tickers")
        .iter()
        .find(|t| t["symbol"] == symbol)
        .map(|t| n(t, "last"))
        .filter(|p| *p > 0.)
        .ok_or_else(|| format!("Price not available for {symbol}"))?;
    let market = market(store, &symbol).ok_or_else(|| format!("Market not found for {symbol}"))?;
    let position = rows(store, "positions")
        .iter()
        .find(|p| p["symbol"] == symbol && n(p, "contracts") != 0.);
    let leverage = position.map(|p| n(p, "leverage")).unwrap_or(1.);
    let notional = n(balance, "free") * leverage;
    let amount = Placement::sized(&symbol, "buy", &format!("{notional}$"), false).size(
        store,
        market,
        last,
        n(&market["precision"], "amount"),
    )?;
    Ok(format!(
        "{symbol}\n  Free balance: {}\n  Leverage:     {leverage}x ({})\n  Max notional: ${notional:.2}\n  Max contracts: {amount}",
        n(balance, "free"),
        if position.is_some() {
            "from open position"
        } else {
            "no open position — assumed; set with 'lev <n>'"
        }
    ))
}
pub(crate) fn evaluate(
    symbol: &str,
    args: &[String],
    store: &Value,
    raw: &str,
) -> Option<Evaluation> {
    let all = raw.split_whitespace().any(|s| matches!(s, "--all" | "-a"));
    let result=match args[0].as_str() {
        "orders"=>Ok(orders(symbol,all||args.get(1).is_some_and(|s|s.eq_ignore_ascii_case("all")),store)),
        "positions"=>Ok(positions(symbol,all,args.get(1).map(String::as_str).filter(|s|matches!(*s,"long"|"short")),store)),
        "position"=> {
            let target=args.get(if args.get(1).is_some_and(|s|s.eq_ignore_ascii_case("show")){2}else{1});
            let target=target.map(|s|resolve(store,s).unwrap_or_else(||s.to_uppercase())).unwrap_or_else(||symbol.into());
            Ok(position(&target,store))
        }
        "margin"=>Ok(margin(store)),"spot"=>Ok(spot(store)),"max"=>max(symbol,args,store),
        "status"=>Ok(format!("Account: {}\nExchange: {}\nSymbol: {symbol}",store["accountName"].as_str().unwrap_or("Not connected"),store["venue"].as_str().unwrap_or("Not connected"))),
        "symbol"=>match args.get(1) {None=>Ok(format!("Current symbol: {symbol}\nUsage: symbol <symbol>")),Some(typed)=>resolve(store,typed).map(|s|format!("Symbol set to {s}")).ok_or_else(||format!("Unknown symbol \"{}\" on {}.",typed.to_uppercase(),s(store,"venue")))},
        "markets"=> {let mut symbols=rows(store,"markets").iter().map(|m|s(m,"symbol")).collect::<Vec<_>>();symbols.sort();Ok(format!("Available markets ({}):\n{}",symbols.len(),symbols.chunks(4).map(|chunk|format!("  {}",chunk.join("\t"))).collect::<Vec<_>>().join("\n")))},
        "connection"=> {
            if args.get(1).is_some_and(|s|matches!(s.as_str(),"reconnect"|"rc")) {return Some(failed("Manual reconnection is not available through hosted EXEC".into()));}
            let health=&store["socketHealth"];let private=if health["privateInitialized"]!=true {"unknown / not yet available"}else if health["privateHealthy"]==true {"healthy"}else{"degraded"};
            let loaded=store["loaded"].as_object().map(|rows|rows.iter().map(|(k,v)|format!("  {k}: {}",if *v==true{"Yes"}else{"No"})).collect::<Vec<_>>().join("\n")).unwrap_or_else(||"  unknown / not yet available".into());
            Ok(format!("Connection Status: {}\nPrivate WS: {private}\nData Loaded:\n{loaded}",s(store,"venue")))
        }
        "check"=> {
            let target=args.get(1).map(String::as_str).unwrap_or(symbol);let Some(m)=market(store,target) else{return Some(failed(format!("No market \"{target}\" on {}",s(store,"venue"))));};
            let venue=s(store,"venue");let reason=if matches!(venue,"bybit"|"bybitdemo"|"bybitinverse"|"bybitspot") {
                if store["instrumentClassificationsReady"]==false {"classification unknown / not yet available".to_string()}else{
                    let labels=m.get("instrumentClassification").or_else(||m.get("info")).unwrap_or(&Value::Null);
                    match labels.get("symbolType") {
                        None | Some(Value::Null) => "bybit instrument has no symbolType field".into(),
                        Some(Value::String(label)) => match label.trim() {
                            "" | "innovation" => "crypto perpetual".into(),
                            known @ ("stock"|"ETF"|"commodity"|"forex") => {
                                let asset=match known {"stock"=>"tokenized equity","ETF"=>"tokenized ETF","commodity"=>"tokenized commodity",_=>"tokenized FX pair"};
                                format!("{asset}{} — may require signing a trading agreement",labels["underlyingTicker"].as_str().filter(|s|!s.is_empty()).map(|s|format!(" ({s})")).unwrap_or_default())
                            }
                            other=>format!("unrecognised bybit symbolType \"{other}\""),
                        },
                        Some(_)=>"bybit symbolType is not a string".into(),
                    }
                }
            }else{format!("no instrument classifier implemented for {venue}")};Ok(format!("{target} on {venue}: {reason}"))
        }
        "help"=> {
            if let Some(name)=args.get(1) {let canonical=canonical(name);match catalog().iter().find(|c|c["name"].as_str()==canonical){Some(row)=>Ok(format!("Command: {}\n  {}\n  Aliases: {}",s(row,"name"),s(row,"description"),row["aliases"].as_array().unwrap().iter().map(|a|a.as_str().unwrap()).collect::<Vec<_>>().join(", "))+"\n  Examples:\n"+&row["examples"].as_array().unwrap().iter().map(|v|format!("    {}",v.as_str().unwrap())).collect::<Vec<_>>().join("\n")),None=>Ok(format!("Warning: Unknown command: {name}"))}}
            else{Ok(format!("Tealstreet CLI Commands:\n{}",catalog().iter().map(|c|format!("  {:16} {}",s(c,"name"),s(c,"description"))).collect::<Vec<_>>().join("\n")))}
        }
        "history"=>Ok("history not available in this runtime".into()),
        "set"|"sets"|"track"|"untrack"|"unset"|"print"|"alias"|"unalias"|"whitelist"|"unwhitelist"|"fatfinger"|"unfatfinger"|"budget"|"simulation"=>Err(format!("{}: preprocess store unavailable",args[0])),
        "when"=>Err("when: needs a session that stays open. A one-shot command exits before a trigger could fire — run it from the interactive CLI.".into()),
        _=>return None,
    };
    Some(match result {
        Ok(message) => Evaluation::Text {
            message,
            result: json!({"handled":true,"success":true}),
        },
        Err(error) => failed(error),
    })
}
