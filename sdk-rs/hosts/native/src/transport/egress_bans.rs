//! Venue IP bans shared by every account this process hosts (TEALSTREET-NEXT-EH0).
//!
//! Binance bans an egress IP, not a key, and names it: `IP(1.2.3.4) banned until <ms>`.
//! Every account here leaves through the same egress, so one account's ban is all of
//! theirs. A request through a proxy pool lands on a node the pool picks, so one node's
//! ban holds nothing here (the proxy holds its own IP); only several refusing at once
//! means the pool as a whole is refused.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// Distinct pool IPs refusing at once that count as the whole pool refusing.
const POOL_PRESSURE: usize = 3;
const MAX_BANS: usize = 64;
const MAX_HOLD_MS: f64 = 86_400_000.0;
/// A 418 that names no end: Binance's shortest automatic ban.
const UNTIMED_BAN_MS: f64 = 120_000.0;
const MAX_BODY: usize = 4 * 1024;

/// Where a request leaves from: the venue host, and the proxy host it went through
/// (`None`: this machine's own address).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) struct Egress {
    pub venue: String,
    pub via: Option<String>,
}

/// The refusal to repeat while the ban lasts, and when it ends (wall-clock ms).
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Ban {
    pub status: u16,
    pub body: String,
    pub until_ms: f64,
}

type Key = (Egress, String);

fn bans() -> &'static Mutex<HashMap<Key, Ban>> {
    static BANS: OnceLock<Mutex<HashMap<Key, Ban>>> = OnceLock::new();
    BANS.get_or_init(Default::default)
}

pub(super) fn now_ms() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0.0, |elapsed| elapsed.as_millis() as f64)
}

/// The IP a refusal names, if any, and when its ban ends. Only an IP-wide refusal
/// counts: a 418, or a 429 carrying Binance's IP-weight code -1003 (a -1015 429 is
/// one key's order limit and bans nobody).
pub(super) fn parse(
    status: u16,
    body: &str,
    retry_after: Option<&str>,
    now: f64,
) -> Option<(Option<String>, f64)> {
    let body = &body[..body.floor_char_boundary(MAX_BODY)];
    if !(status == 418 || status == 429 && body.contains("-1003")) {
        return None;
    }
    let ip = body
        .split_once("IP(")
        .and_then(|(_, rest)| rest.split_once(')'))
        .map(|(ip, _)| ip.trim())
        .filter(|ip| !ip.is_empty() && ip.len() <= 45)
        .map(str::to_owned);
    let banned_until = body.split_once("banned until ").and_then(|(_, rest)| {
        let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
        rest[..digits].parse::<f64>().ok()
    });
    let retry_after = retry_after
        .and_then(|seconds| seconds.trim().parse::<f64>().ok())
        .filter(|seconds| seconds.is_finite() && *seconds > 0.0)
        .map(|seconds| now + seconds * 1_000.0);
    let until = banned_until
        .or(retry_after)
        .or((status == 418).then_some(now + UNTIMED_BAN_MS))?;
    let until = until.min(now + MAX_HOLD_MS);
    (until > now).then_some((ip, until))
}

/// Remember a refusal. Through a pool only a named IP is attributable.
pub(super) fn record(egress: &Egress, ip: Option<String>, ban: Ban, now: f64) {
    if egress.via.is_some() && ip.is_none() {
        return;
    }
    let mut bans = bans()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    bans.retain(|_, ban| ban.until_ms > now);
    let key = (egress.clone(), ip.unwrap_or_default());
    if bans.len() >= MAX_BANS
        && !bans.contains_key(&key)
        && let Some(soonest) = bans
            .iter()
            .min_by(|a, b| a.1.until_ms.total_cmp(&b.1.until_ms))
            .map(|(key, _)| key.clone())
    {
        bans.remove(&soonest);
    }
    let longer = bans
        .get(&key)
        .is_none_or(|previous| previous.until_ms < ban.until_ms);
    if longer {
        bans.insert(key, ban);
    }
}

/// The ban a read through `egress` must repeat instead of sending, if any: any ban on
/// this machine's own address, or, through a pool, the soonest-ending of at least
/// `POOL_PRESSURE` refusing IPs, so reads resume as soon as one node frees up.
pub(super) fn held(egress: &Egress, now: f64) -> Option<Ban> {
    let mut bans = bans()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    bans.retain(|_, ban| ban.until_ms > now);
    let active = bans
        .iter()
        .filter(|((key, _), _)| key == egress)
        .map(|(_, ban)| ban);
    if egress.via.is_none() {
        return active
            .max_by(|a, b| a.until_ms.total_cmp(&b.until_ms))
            .cloned();
    }
    let active: Vec<_> = active.collect();
    (active.len() >= POOL_PRESSURE)
        .then(|| {
            active
                .into_iter()
                .min_by(|a, b| a.until_ms.total_cmp(&b.until_ms))
        })
        .flatten()
        .cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: f64 = 1_800_000_000_000.0;

    fn egress(venue: &str, via: Option<&str>) -> Egress {
        Egress {
            venue: venue.into(),
            via: via.map(str::to_owned),
        }
    }

    fn banned(ip: &str, until: f64) -> String {
        format!(
            r#"{{"code":-1003,"msg":"Way too many requests; IP({ip}) banned until {until}. Please use the websocket for live updates to avoid bans."}}"#
        )
    }

    fn ban(body: &str, until_ms: f64) -> Ban {
        Ban {
            status: 418,
            body: body.into(),
            until_ms,
        }
    }

    #[test]
    fn banned_until_is_the_exact_end_and_beats_retry_after() {
        let body = banned("108.61.162.122", NOW + 300_000.0);
        assert_eq!(
            parse(418, &body, Some("120"), NOW),
            Some((Some("108.61.162.122".into()), NOW + 300_000.0))
        );
    }

    #[test]
    fn without_a_timestamp_retry_after_then_two_minutes() {
        let body = r#"{"code":-1003,"msg":"IP banned"}"#;
        assert_eq!(
            parse(418, body, Some("7"), NOW),
            Some((None, NOW + 7_000.0))
        );
        assert_eq!(parse(418, body, None, NOW), Some((None, NOW + 120_000.0)));
    }

    #[test]
    fn only_ip_wide_refusals_count() {
        let weight = r#"{"code":-1003,"msg":"Too much request weight used"}"#;
        assert_eq!(
            parse(429, weight, Some("30"), NOW),
            Some((None, NOW + 30_000.0))
        );
        // One key's order-count limit bans nobody else.
        let orders = r#"{"code":-1015,"msg":"Too many new orders"}"#;
        assert_eq!(parse(429, orders, Some("10"), NOW), None);
        assert_eq!(parse(429, weight, None, NOW), None);
        assert_eq!(parse(403, &banned("1.2.3.4", NOW + 1.0), None, NOW), None);
        assert_eq!(parse(418, &banned("1.2.3.4", NOW - 1.0), None, NOW), None);
        assert_eq!(
            parse(418, &banned("1.2.3.4", 1e30), None, NOW),
            Some((Some("1.2.3.4".into()), NOW + MAX_HOLD_MS))
        );
    }

    #[test]
    fn this_machines_own_ban_holds_every_read_until_it_ends() {
        let own = egress("fapi.own-ban.test", None);
        let body = banned("203.0.113.7", NOW + 60_000.0);
        record(
            &own,
            Some("203.0.113.7".into()),
            ban(&body, NOW + 60_000.0),
            NOW,
        );
        assert_eq!(held(&own, NOW).map(|ban| ban.body), Some(body));
        assert_eq!(held(&own, NOW + 60_000.0), None);
        assert_eq!(held(&egress("api.own-ban.test", None), NOW), None);
    }

    #[test]
    fn a_pool_holds_only_when_several_of_its_ips_refuse() {
        let pool = egress("fapi.pool-ban.test", Some("proxies.pool-ban.test"));
        record(
            &pool,
            Some("10.0.0.1".into()),
            ban("a", NOW + 50_000.0),
            NOW,
        );
        record(
            &pool,
            Some("10.0.0.2".into()),
            ban("b", NOW + 20_000.0),
            NOW,
        );
        // Two refusing nodes of nine: the pool keeps serving, each proxy holds itself.
        assert_eq!(held(&pool, NOW), None);
        // An unattributed refusal through a pool names no node and is dropped.
        record(&pool, None, ban("x", NOW + 90_000.0), NOW);
        assert_eq!(held(&pool, NOW), None);
        record(
            &pool,
            Some("10.0.0.3".into()),
            ban("c", NOW + 40_000.0),
            NOW,
        );
        assert_eq!(held(&pool, NOW).map(|ban| ban.body), Some("b".into()));
        // The soonest node frees up and the pool is back under pressure.
        assert_eq!(held(&pool, NOW + 20_000.0), None);
    }

    #[test]
    fn a_shorter_repeat_never_cuts_a_known_ban() {
        let own = egress("fapi.repeat-ban.test", None);
        record(&own, Some("ip".into()), ban("long", NOW + 90_000.0), NOW);
        record(&own, Some("ip".into()), ban("short", NOW + 10_000.0), NOW);
        assert_eq!(
            held(&own, NOW + 30_000.0).map(|ban| ban.body),
            Some("long".into())
        );
    }
}
