//! Session generations prevent delayed unauthorized responses from discarding a fresh token.
use crate::wire;

#[derive(Default)]
pub(crate) struct Session {
    pub token: Option<String>,
    pub expires_at: f64,
    pub generation: u64,
    pub login_pending: bool,
    pub discovery_pending: bool,
    pub account: Option<String>,
}
impl Session {
    pub fn valid(&self, now: f64) -> bool {
        self.token.is_some() && now < self.expires_at
    }

    pub fn accept(&mut self, body: &str, now: f64) -> Result<(), String> {
        let login: wire::Login<'_> =
            serde_json::from_str(body).map_err(|_| "Invalid DXtrade login response".to_owned())?;
        if login.session_token.is_empty() || login.session_token.contains(['\r', '\n']) {
            return Err("Invalid DXtrade session token".into());
        }
        let timeout = timeout_ms(&login.timeout)?;
        let expires_at = now + timeout;
        if !expires_at.is_finite() {
            return Err("Invalid DXtrade session timeout".into());
        }
        self.token = Some(login.session_token.into_owned());
        self.expires_at = expires_at;
        self.generation = self.generation.wrapping_add(1);
        self.login_pending = false;
        Ok(())
    }

    pub fn select_account(
        &mut self,
        body: &str,
        username: &str,
        subaccount: Option<&str>,
    ) -> Result<(), String> {
        let users: wire::UserResponse<'_> = serde_json::from_str(body)
            .map_err(|_| "Invalid DXtrade account discovery response".to_owned())?;
        let matching: Vec<_> = users
            .user_details
            .iter()
            .filter(|user| user.login == username && user.domain == "default")
            .collect();
        if matching.len() != 1 {
            return Err("DXtrade user not found or ambiguous".into());
        }
        let accounts: Vec<_> = matching[0]
            .accounts
            .iter()
            .filter(|account| account.account_status == "FULL_TRADING")
            .collect();
        let selected = if accounts.is_empty() {
            return Err("No valid accounts found".into());
        } else if accounts.len() == 1 {
            accounts[0]
        } else {
            let requested = subaccount
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    "Multiple accounts found. Please specify subaccount number".to_owned()
                })?;
            let matches: Vec<_> = accounts
                .iter()
                .copied()
                .filter(|account| {
                    account.account == requested
                        || account.account.strip_prefix("default:") == Some(requested)
                })
                .collect();
            if matches.len() != 1 {
                return Err("Subaccount for number not found or ambiguous".into());
            }
            matches[0]
        };
        if !selected.account.starts_with("default:") || selected.account.len() == "default:".len() {
            return Err("Invalid DXtrade account code".into());
        }
        self.account = Some(selected.account.to_string());
        self.discovery_pending = false;
        Ok(())
    }
}

pub fn timeout_ms(value: &str) -> Result<f64, String> {
    let parts: Vec<_> = value.split(':').collect();
    if parts.len() != 3
        || parts
            .iter()
            .any(|part| part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()))
    {
        return Err("Invalid DXtrade session timeout".into());
    }
    let hours = parts[0].parse::<u64>().ok();
    let minutes = parts[1].parse::<u64>().ok();
    let seconds = parts[2].parse::<u64>().ok();
    match (hours, minutes, seconds) {
        (Some(hours), Some(minutes), Some(seconds)) if minutes < 60 && seconds < 60 => {
            let total = hours as f64 * 3_600.0 + minutes as f64 * 60.0 + seconds as f64;
            if total > 0.0 && total <= 31_536_000.0 {
                Ok(total * 1_000.0)
            } else {
                Err("Invalid DXtrade session timeout".into())
            }
        }
        _ => Err("Invalid DXtrade session timeout".into()),
    }
}
