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
}

impl Config {
    pub fn from_env() -> Result<Self, String> {
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
        let threads = std::thread::available_parallelism().map_or(2, usize::from);
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
            access: crate::access::Config::parse(
                var("TEALSTREET_ACCESS_TEAM_DOMAIN").as_deref(),
                var("TEALSTREET_ACCESS_AUD").as_deref(),
            ),
        })
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
    use super::release_label;

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
