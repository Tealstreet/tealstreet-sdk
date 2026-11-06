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

    /// Writes the setting, unless a settings write since the read owns it.
    fn apply_leverage_read(
        &self,
        cx: &mut Cx<'_, Pending>,
        read: &Read,
        response: &Response<'_>,
    ) -> Result<(), String> {
        if !(200..300).contains(&response.status) {
            return Err(format!("Bitget leverage read HTTP {}", response.status));
        }
        let current = self.account.settings_version == read.settings_version;
        let invalid = |error: serde_json::Error| error.to_string();
        if read.uta {
            let settings = serde_json::from_str::<Envelope<UtaSettings>>(response.body)
                .map_err(invalid)?
                .checked()?;
            if !settings
                .symbol_config_list
                .iter()
                .any(|row| row.symbol == read.symbol)
            {
                return Err(format!("Bitget settings do not list {}", read.symbol));
            }
            if current {
                self.apply_settings(cx, &settings);
            }
            return Ok(());
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
        Ok(())
    }

    pub(crate) fn leverage_response(
        &self,
        cx: &mut Cx<'_, Pending>,
        read: Read,
        response: Response<'_>,
    ) {
        if let Err(error) = self.apply_leverage_read(cx, &read, &response) {
            let _ = cx.fail_context(&read.root, VenueError::new(error));
            return;
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
