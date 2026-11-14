//! The environment surface; see DESIGN.md "Environment".
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct Config {
    pub port: u16,
    pub region: Option<String>,
    pub release: Option<String>,
    pub engine_threads: usize,
    pub max_connections: usize,
    /// Unauthenticated public-data sockets per client IP.
    pub max_public_per_ip: usize,
    pub idle_timeout: Duration,
    pub resume_grace: Duration,
    pub account_flush: Duration,
    pub access: Option<crate::access::Config>,
    pub router_lease: crate::router_lease::Config,
    pub idle_release_ttl: Duration,
}

impl Config {
    pub fn from_env() -> Result<Self, String> {
        delegated_order_frames_from_env()?;
        let var = |name: &str| std::env::var(name).ok().filter(|value| !value.is_empty());
        let number = |name: &str, default: usize| -> Result<usize, String> {
            var(name).map_or(Ok(default), |value| {
                value
                    .parse()
                    .ok()
                    .filter(|parsed| *parsed > 0)
                    .ok_or(format!("{name} must be a positive integer"))
            })
        };
        let idle_ms = number(
            "TEALSTREET_IDLE_RELEASE_TTL_MS",
            crate::user_drain::DEFAULT_IDLE_RELEASE_TTL_MS as usize,
        )? as u64;
        let idle_release_ttl = Duration::from_millis(idle_ms);
        if crate::accounts::now_ms().checked_add(idle_ms).is_none()
            || idle_release_ttl
                .checked_mul(2)
                .and_then(|retention| std::time::Instant::now().checked_add(retention))
                .is_none()
        {
            return Err("TEALSTREET_IDLE_RELEASE_TTL_MS is out of range".into());
        }
        let threads = std::thread::available_parallelism().map_or(2, usize::from);
        let router_lease_ms = number("TEALSTREET_ROUTER_LEASE_MS", 90_000)?;
        if !(20_000..=3_600_000).contains(&router_lease_ms) {
            return Err("TEALSTREET_ROUTER_LEASE_MS must be between 20000 and 3600000".into());
        }
        Ok(Self {
            port: u16::try_from(number("TEALSTREET_PORT", 23456)?)
                .map_err(|_| "TEALSTREET_PORT is out of range".to_owned())?,
            region: var("CONNECT_REGION"),
            release: release_label(var("TEALSTREET_RELEASE")),
            engine_threads: number("TEALSTREET_ENGINE_THREADS", threads)?,
            max_connections: number("TEALSTREET_MAX_CONNECTIONS", 4096)?,
            max_public_per_ip: 8,
            idle_timeout: Duration::from_secs(30),
            resume_grace: Duration::from_secs(60),
            account_flush: Duration::from_millis(60),
            router_lease: crate::router_lease::Config::parse(
                var("TEALSTREET_ROUTER_BOX_ID"),
                var("TEALSTREET_BOX"),
                u64::try_from(router_lease_ms)
                    .map_err(|_| "TEALSTREET_ROUTER_LEASE_MS is out of range")?,
                var("TEALSTREET_ROUTER_LEASE_ENFORCE").as_deref(),
                var("TEALSTREET_ROUTER_LEASE_ENABLE_GRACE_UNTIL").as_deref(),
            )?,
            idle_release_ttl,
            access: crate::access::Config::parse(
                var("TEALSTREET_ACCESS_TEAM_DOMAIN").as_deref(),
                var("TEALSTREET_ACCESS_AUD").as_deref(),
            ),
        })
    }
}

/// Startup-only deployment control. Changing it uses the same image and the
/// existing drain/restart path; it does not change a live process's environment.
pub(crate) fn delegated_order_frames_from_env() -> Result<bool, String> {
    parse_delegated_order_frames(
        std::env::var("TEALSTREET_DELEGATED_ORDER_FRAMES")
            .ok()
            .as_deref(),
    )
}

fn parse_delegated_order_frames(value: Option<&str>) -> Result<bool, String> {
    match value {
        None | Some("" | "0") => Ok(false),
        Some("1") => Ok(true),
        Some(_) => Err("TEALSTREET_DELEGATED_ORDER_FRAMES must be 0 or 1".into()),
    }
}

fn release_label(value: Option<String>) -> Option<String> {
    let trimmed = value.as_deref()?.trim();
    (!trimmed.is_empty()
        && !["undefined", "null", "unknown"]
            .iter()
            .any(|placeholder| trimmed.eq_ignore_ascii_case(placeholder)))
    .then(|| trimmed.to_owned())
}

#[cfg(test)]
mod tests {
    use super::{parse_delegated_order_frames, release_label};

    #[test]
    fn order_frame_runtime_control_is_default_off_and_strict() {
        for (value, expected) in [
            (None, Ok(false)),
            (Some(""), Ok(false)),
            (Some("0"), Ok(false)),
            (Some("1"), Ok(true)),
            (
                Some("invalid"),
                Err("TEALSTREET_DELEGATED_ORDER_FRAMES must be 0 or 1".into()),
            ),
        ] {
            assert_eq!(parse_delegated_order_frames(value), expected, "{value:?}");
        }
        for value in ["true", "false", "on", "off", " 1", "1 ", "2"] {
            assert!(parse_delegated_order_frames(Some(value)).is_err());
        }
    }

    #[test]
    fn release_uses_the_image_revision_without_placeholder_strings() {
        assert_eq!(
            release_label(Some(" fb652156d52 ".into())).as_deref(),
            Some("fb652156d52")
        );
        assert_eq!(release_label(None), None);
        for value in ["", " ", "undefined", "null", "unknown", " Undefined "] {
            assert_eq!(release_label(Some(value.into())), None, "{value}");
        }
    }
}
