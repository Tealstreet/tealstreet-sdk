//! One symbol's leverage on demand, flat or not: margin mode, then the cross
//! leverage read. KuCoin exposes no isolated leverage read; it is 1, as in TS.
use crate::account::Metadata;
use crate::wire::Envelope;
use crate::{Kucoin, Pending, api_symbol};
use safe_cex_engine::model::{MarginType, PositionMode};
use safe_cex_engine::{CommandId, Cx, Method, Response};
use serde::Serialize;

#[derive(Clone, Copy)]
enum Step {
    Margin,
    Cross,
}

pub struct Read {
    command: CommandId,
    symbol: String,
    revision: u64,
    step: Step,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Fetched {
    buy_leverage: f64,
    sell_leverage: f64,
    margin_type: MarginType,
    #[serde(skip_serializing_if = "Option::is_none")]
    position_mode: Option<PositionMode>,
}

/// The row a read answered with, or why it cannot be used.
fn row<'a>(response: &Response<'a>) -> Result<Metadata<'a>, String> {
    if !(200..300).contains(&response.status) {
        return Err(format!("KuCoin leverage read HTTP {}", response.status));
    }
    let envelope: Envelope<'a> = serde_json::from_str(response.body)
        .map_err(|_| "KuCoin leverage response is invalid".to_owned())?;
    if envelope.code != "200000" {
        return Err(format!("KuCoin {}: {}", envelope.code, envelope.msg));
    }
    envelope
        .data
        .and_then(|data| serde_json::from_str(data.get()).ok())
        .ok_or_else(|| "KuCoin leverage response has no usable data".to_owned())
}

impl Read {
    pub(crate) fn command(&self) -> CommandId {
        self.command
    }
}

impl Kucoin {
    pub(crate) fn fetch_leverage(
        &self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        symbol: String,
    ) {
        if self.options.public_data_only || self.credentials.is_none() {
            return cx.fail(command, "KuCoin leverage requires credentials");
        }
        if cx.store.market(&symbol).is_none() {
            return cx.fail(command, format!("unknown market {symbol}"));
        }
        let read = Read {
            command,
            symbol,
            revision: self.account_revision(),
            step: Step::Margin,
        };
        self.send_leverage_read(cx, read);
    }

    fn send_leverage_read(&self, cx: &mut Cx<'_, Pending>, read: Read) {
        let id = api_symbol(cx.store, &read.symbol);
        let path = match read.step {
            Step::Margin => format!("/api/v2/position/getMarginMode?symbol={id}"),
            Step::Cross => format!("/api/v2/getCrossUserLeverage?symbol={id}"),
        };
        self.signed(cx, Method::Get, &path, None, Pending::Leverage(read));
    }

    pub(crate) fn leverage_response(
        &self,
        cx: &mut Cx<'_, Pending>,
        mut read: Read,
        response: Response<'_>,
    ) {
        let row = match row(&response) {
            Ok(row) => row,
            Err(error) => return cx.fail(read.command, error),
        };
        let current = self.account_settings_unchanged_since(read.revision);
        match read.step {
            Step::Margin => {
                if !matches!(row.margin_mode, "CROSS" | "ISOLATED") {
                    let error = format!("KuCoin returned no margin mode for {}", read.symbol);
                    return cx.fail(read.command, error);
                }
                if row.margin_mode == "CROSS" {
                    if current {
                        cx.store.update_setting(&read.symbol, |s| {
                            s.margin_type = Some(MarginType::Cross);
                        });
                    }
                    read.step = Step::Cross;
                    return self.send_leverage_read(cx, read);
                }
                if current {
                    cx.store.update_setting(&read.symbol, |s| {
                        s.margin_type = Some(MarginType::Isolated);
                        if !(s.leverage.is_finite() && s.leverage > 0.0) {
                            s.leverage = 1.0;
                        }
                    });
                }
            }
            Step::Cross => {
                let leverage = row.leverage.get();
                if !(leverage.is_finite() && leverage > 0.0) {
                    let error = format!("KuCoin returned invalid leverage for {}", read.symbol);
                    return cx.fail(read.command, error);
                }
                if current {
                    cx.store
                        .update_setting(&read.symbol, |s| s.leverage = leverage);
                }
            }
        }
        // A settings write since this read was sent owns the setting; report it.
        let Some(setting) = cx
            .store
            .setting(&read.symbol)
            .filter(|setting| setting.leverage.is_finite() && setting.leverage > 0.0)
        else {
            let error = format!(
                "KuCoin leverage for {} changed during the read",
                read.symbol
            );
            return cx.fail(read.command, error);
        };
        let fetched = Fetched {
            buy_leverage: setting.leverage,
            sell_leverage: setting.leverage,
            margin_type: setting.margin_type.unwrap_or(MarginType::Cross),
            position_mode: self.mode_known().then(|| {
                if cx.store.hedged() {
                    PositionMode::Hedged
                } else {
                    PositionMode::Oneway
                }
            }),
        };
        match serde_json::to_string(&fetched) {
            Ok(data) => cx.done(read.command, data),
            Err(error) => cx.fail(read.command, error.to_string()),
        }
    }
}
