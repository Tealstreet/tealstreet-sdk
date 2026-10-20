//! V5 products share routes; categories, socket depth and account capabilities differ.
use crate::Options;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Category {
    Linear,
    Inverse,
    Spot,
    Option,
}

impl Category {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Linear => "linear",
            Self::Inverse => "inverse",
            Self::Spot => "spot",
            Self::Option => "option",
        }
    }

    pub(crate) fn for_symbol(symbol: &str) -> Self {
        if !symbol.contains('-') {
            return if crate::account::is_inverse_symbol(symbol) {
                Self::Inverse
            } else {
                Self::Linear
            };
        }
        let parts: Vec<&str> = symbol.split('-').collect();
        if let [base, expiry, strike, right, ..] = parts.as_slice() {
            let expiry = expiry.as_bytes();
            let day = expiry
                .iter()
                .take_while(|byte| byte.is_ascii_digit())
                .count();
            let expiry_valid = (1..=2).contains(&day)
                && expiry.len() == day + 5
                && expiry[day..day + 3].iter().all(u8::is_ascii_uppercase)
                && expiry[day + 3..].iter().all(u8::is_ascii_digit);
            let mut decimals = strike.split(['d', '.']);
            let integer = decimals.next().unwrap_or_default();
            let fraction = decimals.next();
            let strike_valid = !integer.is_empty()
                && integer.bytes().all(|byte| byte.is_ascii_digit())
                && fraction.is_none_or(|part| {
                    !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit())
                })
                && decimals.next().is_none();
            if !base.is_empty()
                && base
                    .bytes()
                    .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
                && expiry_valid
                && strike_valid
                && matches!(*right, "C" | "P")
                && (parts.len() == 4
                    || (parts.len() == 5
                        && !parts[4].is_empty()
                        && parts[4].bytes().all(|byte| byte.is_ascii_uppercase())))
            {
                return Self::Option;
            }
        }
        if crate::account::is_inverse_symbol(symbol) {
            Self::Inverse
        } else {
            Self::Linear
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Variant {
    #[default]
    Linear,
    Inverse,
    Spot,
    Demo,
    Hyrotrader,
}

impl Variant {
    pub fn is_spot(self) -> bool {
        self == Self::Spot
    }

    pub fn is_inverse(self) -> bool {
        self == Self::Inverse
    }

    pub fn is_hyro(self) -> bool {
        self == Self::Hyrotrader
    }
}

/// Hyro uses its own configured origins, never a fallback to Bybit.
#[derive(Clone, Debug)]
pub struct Endpoints {
    pub rest: String,
    pub public_ws: String,
    pub private_ws: String,
}

pub struct Paths {
    pub rest: String,
    pub public_ws: String,
    pub private_ws: String,
    pub category: &'static str,
    pub book_depth: u32,
    pub batch_orders: bool,
}

impl Paths {
    pub fn for_options(options: &Options) -> Self {
        let category = match options.variant {
            Variant::Inverse => "inverse",
            Variant::Spot => "spot",
            _ => "linear",
        };
        let network = if options.testnet { "-testnet" } else { "" };
        let demo = options.variant == Variant::Demo && !options.testnet;
        let rest = if demo {
            "https://api-demo.bybit.com".into()
        } else {
            format!("https://api{network}.bybit.com")
        };
        let public_ws = format!("wss://stream{network}.bybit.com/v5/public/{category}");
        let private_ws = if demo {
            "wss://stream-demo.bybit.com/v5/private".into()
        } else {
            format!("wss://stream{network}.bybit.com/v5/private")
        };
        let mut paths = Self {
            rest,
            public_ws,
            private_ws,
            category,
            book_depth: if options.variant.is_spot() { 200 } else { 1000 },
            batch_orders: !options.variant.is_spot() && !options.variant.is_hyro(),
        };
        if options.variant.is_hyro() {
            let endpoints = options.endpoints.as_ref();
            paths.rest =
                endpoints.map_or_else(String::new, |e| e.rest.trim_end_matches('/').into());
            paths.public_ws = endpoints.map_or_else(String::new, |e| e.public_ws.clone());
            paths.private_ws = endpoints.map_or_else(String::new, |e| e.private_ws.clone());
        }
        paths
    }
}

impl Options {
    pub fn validate(&self) -> Result<(), String> {
        if !self.variant.is_hyro() {
            return if self.endpoints.is_some() {
                Err("Custom endpoints are supported only for HyroTrader".into())
            } else {
                Ok(())
            };
        }
        let endpoints = self
            .endpoints
            .as_ref()
            .ok_or("HyroTrader endpoints are not configured")?;
        for (name, url, schemes) in [
            ("REST", endpoints.rest.as_str(), ["http://", "https://"]),
            (
                "public WebSocket",
                endpoints.public_ws.as_str(),
                ["ws://", "wss://"],
            ),
            (
                "private WebSocket",
                endpoints.private_ws.as_str(),
                ["ws://", "wss://"],
            ),
        ] {
            let valid = schemes.iter().any(|scheme| {
                url.strip_prefix(scheme).is_some_and(|rest| {
                    valid_authority(rest.split('/').next().unwrap_or_default())
                        && !rest.chars().any(char::is_whitespace)
                        && !rest.contains(['?', '#'])
                })
            });
            if !valid {
                return Err(format!(
                    "HyroTrader {name} URL is not configured or invalid"
                ));
            }
        }
        Ok(())
    }
}

fn valid_authority(authority: &str) -> bool {
    let (host, port) = if let Some(ipv6) = authority.strip_prefix('[') {
        let Some((host, suffix)) = ipv6.split_once(']') else {
            return false;
        };
        if host.parse::<std::net::Ipv6Addr>().is_err() {
            return false;
        }
        let port = if suffix.is_empty() {
            None
        } else if let Some(port) = suffix.strip_prefix(':') {
            Some(port)
        } else {
            return false;
        };
        (host, port)
    } else {
        let (host, port) = authority
            .split_once(':')
            .map_or((authority, None), |(host, port)| (host, Some(port)));
        if !host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-')
        {
            return false;
        }
        (host, port)
    };
    !host.is_empty() && port.is_none_or(|port| port.parse::<u16>().is_ok_and(|port| port != 0))
}
