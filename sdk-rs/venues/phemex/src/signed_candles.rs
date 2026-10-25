//! Credentialed public candles retain their native wire shape and selection.
use crate::{Pending, PhemexPerpetuals, dispatch_signing, resolution};
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::model::Interval;
use safe_cex_engine::selected_read::{
    LocalRefusal, ReadLegPolicy, ReadRequest, ReadRole, SelectedRead,
};
use safe_cex_engine::{Cx, Method, Venue, VenueError};
impl PhemexPerpetuals {
    pub(crate) fn fetch_signed_candles(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        symbol: &str,
        interval: Interval,
        limit: Option<u32>,
        from_ms: Option<f64>,
        to_ms: Option<f64>,
    ) {
        let Ok(root) = SelectedRead::command_root(
            cx,
            SelectedRead::select(&self.public_read_slots, ReadRole::Read { scope: None }),
            LocalRefusal {
                diagnostic: None,
                error: VenueError::new("Phemex candle read requires credentials"),
            },
        ) else {
            return;
        };
        if cx.now < self.paused_until {
            let _ = cx.fail_context(
                root.context(),
                "Phemex requests are rate limited; candle read was not sent",
            );
            return;
        }
        let Some(n) = resolution(interval) else {
            let _ = cx.fail_context(root.context(), interval.unsupported("Phemex"));
            return;
        };
        let to = to_ms.unwrap_or(cx.now);
        let requested_limit = limit.filter(|n| *n > 0);
        let default_from = to - n as f64 * 1000. * requested_limit.unwrap_or(500).min(1000) as f64;
        let from = if requested_limit.is_some() {
            default_from
        } else {
            from_ms.unwrap_or(default_from)
        };
        let path = "/exchange/public/md/v2/kline/list";
        let query = format!(
            "symbol={}&resolution={n}&from={:.0}&to={:.0}",
            safe_cex_engine::url::encode(self.native_symbol(symbol)),
            (from / 1000.).floor(),
            (to / 1000.).ceil()
        );
        let leg = match root.leg(
            cx,
            ReadLegPolicy {
                timeout_ms: 20_000,
                pacing: None,
            },
        ) {
            Ok(leg) => leg,
            Err(error) => {
                let _ = cx.fail_context(root.context(), error.read_venue_error());
                return;
            }
        };
        let testnet = self.options.testnet;
        let url = format!("{}{path}?{query}", self.rest());
        if let Err((pending, error)) = leg.queue(
            cx,
            ReadRequest {
                method: Method::Get,
                url,
                body: None,
                stream: Some(ArrayAt::Path(&["data", "rows"])),
            },
            |identity| {
                Box::new(dispatch_signing::Http::new(
                    identity.clone(),
                    testnet,
                    Method::Get,
                    path.into(),
                    query,
                    None,
                ))
            },
            |leg| Pending::Candles {
                root,
                leg,
                rows: vec![],
                invalid: false,
            },
        ) {
            self.on_sign_error(cx, pending, error.read_venue_error());
        }
    }
}
