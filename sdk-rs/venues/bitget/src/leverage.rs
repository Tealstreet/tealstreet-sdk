//! One symbol's leverage on demand, flat or not. Classic keeps it per symbol on
//! the account read; UTA lists it in the account settings.
use crate::wire::{Envelope, SymbolConfig, UtaSettings};
use crate::{BitgetFutures, Pending, paths, sign};
use safe_cex_engine::model::{MarginType, PositionMode};
use safe_cex_engine::request_context::Context;
use safe_cex_engine::{Cx, ErrorKind, Method, Outcome, Response, Venue, VenueError};
use serde::Serialize;

pub struct Read {
    root: Context,
    symbol: String,
    uta: bool,
    settings_version: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Fetched {
    buy_leverage: f64,
    sell_leverage: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    margin_type: Option<MarginType>,
    position_mode: PositionMode,
}

impl BitgetFutures {
    pub(crate) fn fetch_leverage(&mut self, cx: &mut Cx<'_, Pending>, symbol: String) {
        let coin = self.margin_coin(cx, &symbol).map(str::to_owned);
        let uta = self.account.uta;
        let path = match coin {
            Ok(_) if uta => paths::SETTINGS.to_owned(),
            Ok(coin) => format!(
                "{}?symbol={symbol}&productType={}&marginCoin={coin}",
                paths::ACCOUNT,
                self.product()
            ),
            Err(_) => String::new(),
        };
        let refusal = if !self.private_enabled() {
            Some("Bitget leverage requires credentials".to_owned())
        } else if !self.account.detected {
            Some("Bitget account type is not known yet".to_owned())
        } else if cx.store.market(&symbol).is_none() {
            Some(format!("unknown market {symbol}"))
        } else {
            None
        };
        let (credential, identity) = if refusal.is_none() {
            self.read_selection(&path)
        } else {
            (None, None)
        };
        // A rejected admission already published the command's typed failure.
        let Ok(root) = cx.admit_http_command(crate::read_policy(credential)) else {
            return;
        };
        if let Some(message) = refusal {
            let error = VenueError::new(message)
                .kind(ErrorKind::InvalidInput)
                .outcome(Outcome::NotSent);
            let _ = cx.fail_context(&root, error);
            return;
        }
        let read = Read {
            root: root.clone(),
            symbol,
            uta,
            settings_version: self.account.settings_version,
        };
        let recipe = sign::Http::new(identity, path.clone());
        if let Err((pending, reason)) = cx.request_signed_managed(
            root,
            None,
            Method::Get,
            format!("{}{path}", paths::REST),
            None,
            Box::new(recipe),
            Pending::Leverage(read),
        ) {
            self.on_sign_error(cx, pending, reason.venue_error());
        }
    }

    /// False means a fresh, flat UTA symbol has no published leverage yet.
    /// A newer settings write owns the value instead of this read.
    fn apply_leverage_read(
        &self,
        cx: &mut Cx<'_, Pending>,
        read: &Read,
        response: &Response<'_>,
    ) -> Result<bool, String> {
        if !(200..300).contains(&response.status) {
            return Err(format!("Bitget leverage read HTTP {}", response.status));
        }
        let current = self.account.settings_version == read.settings_version;
        let invalid = |error: serde_json::Error| error.to_string();
        if read.uta {
            let settings = serde_json::from_str::<Envelope<UtaSettings>>(response.body)
                .map_err(invalid)?
                .checked()?;
            if !matches!(settings.hold_mode.as_ref(), "one_way_mode" | "hedge_mode") {
                return Err("Bitget returned an invalid UTA hold mode".to_owned());
            }
            let row = settings.symbol_config_list.iter().find(|row| {
                row.symbol == read.symbol
                    && (row.category.is_empty()
                        || row.category.eq_ignore_ascii_case(self.product()))
            });
            let Some(row) = row else {
                if !current {
                    return Ok(true);
                }
                let known =
                    cx.store.setting(&read.symbol).is_some_and(|setting| {
                        setting.leverage.is_finite() && setting.leverage > 0.0
                    }) || cx.store.positions().iter().any(|position| {
                        position.symbol == read.symbol && position.contracts != 0.0
                    });
                if !known {
                    return Ok(false);
                }
                return Err(format!("Bitget settings do not list {}", read.symbol));
            };
            if current {
                let has_leverage = [
                    row.leverage,
                    row.long_leverage,
                    row.short_leverage,
                    row.crossed_margin_leverage,
                    row.isolated_long_lever,
                    row.isolated_short_lever,
                ]
                .iter()
                .any(|number| {
                    number
                        .0
                        .is_some_and(|value| value.is_finite() && value > 0.0)
                });
                if !has_leverage {
                    return Err(format!("Bitget returned no leverage for {}", read.symbol));
                }
                self.apply_settings(cx, &settings);
            }
            return Ok(true);
        }
        let row = serde_json::from_str::<Envelope<SymbolConfig>>(response.body)
            .map_err(invalid)?
            .checked()?;
        if row.pos_mode.is_empty() || row.margin_mode.is_empty() {
            return Err(format!("Bitget returned no leverage for {}", read.symbol));
        }
        if current {
            self.apply_symbol_config(cx, &read.symbol, &row);
        }
        Ok(true)
    }

    pub(crate) fn leverage_response(
        &self,
        cx: &mut Cx<'_, Pending>,
        read: Read,
        response: Response<'_>,
    ) {
        match self.apply_leverage_read(cx, &read, &response) {
            Ok(true) => {}
            Ok(false) => {
                let _ = cx.done_context(&read.root, "null".to_owned());
                return;
            }
            Err(error) => {
                let _ = cx.fail_context(&read.root, VenueError::new(error));
                return;
            }
        }
        let Some(setting) = cx
            .store
            .setting(&read.symbol)
            .filter(|setting| setting.leverage.is_finite() && setting.leverage > 0.0)
        else {
            let error = format!("Bitget returned no leverage for {}", read.symbol);
            let _ = cx.fail_context(&read.root, VenueError::new(error));
            return;
        };
        let fetched = Fetched {
            buy_leverage: setting.buy_leverage.unwrap_or(setting.leverage),
            sell_leverage: setting.sell_leverage.unwrap_or(setting.leverage),
            margin_type: setting.margin_type,
            position_mode: if cx.store.hedged() {
                PositionMode::Hedged
            } else {
                PositionMode::Oneway
            },
        };
        let _ = match serde_json::to_string(&fetched) {
            Ok(data) => cx.done_context(&read.root, data),
            Err(error) => cx.fail_context(&read.root, VenueError::new(error.to_string())),
        };
    }

    pub(crate) fn leverage_failed(&self, cx: &mut Cx<'_, Pending>, read: Read, error: VenueError) {
        let _ = cx.fail_context(&read.root, error);
    }
}
