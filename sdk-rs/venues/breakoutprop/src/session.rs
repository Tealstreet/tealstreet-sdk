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
        let mut matching = users
            .user_details
            .iter()
            .filter(|user| user.login == username && user.domain == "default");
        let user = matching
            .next()
            .ok_or("DXtrade user not found or ambiguous")?;
        if matching.next().is_some() {
            return Err("DXtrade user not found or ambiguous".into());
        }
        let mut accounts = user
            .accounts
            .iter()
            .filter(|account| account.account_status == "FULL_TRADING");
        let first = accounts.next().ok_or("No valid accounts found")?;
        let selected = if let Some(second) = accounts.next() {
            let requested = subaccount
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    "Multiple accounts found. Please specify subaccount number".to_owned()
                })?;
            let mut matches = [first, second]
                .into_iter()
                .chain(accounts)
                .filter(|account| {
                    account.account == requested
                        || account.account.strip_prefix("default:") == Some(requested)
                });
            let selected = matches
                .next()
                .ok_or("Subaccount for number not found or ambiguous")?;
            if matches.next().is_some() {
                return Err("Subaccount for number not found or ambiguous".into());
            }
            selected
        } else {
            first
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
    let mut parts = value.split(':');
    let (Some(hours), Some(minutes), Some(seconds), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err("Invalid DXtrade session timeout".into());
    };
    if [hours, minutes, seconds]
        .iter()
        .any(|part| part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()))
    {
        return Err("Invalid DXtrade session timeout".into());
    }
    let hours = hours.parse::<u64>().ok();
    let minutes = minutes.parse::<u64>().ok();
    let seconds = seconds.parse::<u64>().ok();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovery_preserves_selection_cardinality_and_only_commits_success() {
        let account = |code, status| serde_json::json!({"account":code,"accountStatus":status});
        let user =
            |accounts| serde_json::json!({"login":"user","domain":"default","accounts":accounts});
        let full = |code| account(code, "FULL_TRADING");
        for (users, requested, expected) in [
            (
                vec![user(vec![
                    full("default:12"),
                    account("default:2", "CLOSE_ONLY"),
                ])],
                Some("2"),
                Ok("default:12"),
            ),
            (
                vec![user(vec![full("default:12"), full("default:2")])],
                Some("2"),
                Ok("default:2"),
            ),
            (
                vec![user(vec![full("default:12"), full("default:2")])],
                Some("default:2"),
                Ok("default:2"),
            ),
            (
                vec![user(vec![full("default:12"), full("default:2")])],
                None,
                Err("Multiple accounts found. Please specify subaccount number"),
            ),
            (
                vec![user(vec![full("default:12"), full("default:2")])],
                Some(""),
                Err("Multiple accounts found. Please specify subaccount number"),
            ),
            (
                vec![user(vec![
                    full("default:12"),
                    full("default:2"),
                    full("default:2"),
                ])],
                Some("2"),
                Err("Subaccount for number not found or ambiguous"),
            ),
            (
                vec![user(vec![full("default:12"), full("default:3")])],
                Some("2"),
                Err("Subaccount for number not found or ambiguous"),
            ),
            (
                vec![user(vec![full("invalid")])],
                None,
                Err("Invalid DXtrade account code"),
            ),
            (
                vec![user(vec![full("default:")])],
                None,
                Err("Invalid DXtrade account code"),
            ),
            (vec![user(vec![])], None, Err("No valid accounts found")),
            (
                vec![user(vec![]), user(vec![])],
                None,
                Err("DXtrade user not found or ambiguous"),
            ),
        ] {
            let mut session = Session {
                account: Some("default:old".into()),
                discovery_pending: true,
                ..Session::default()
            };
            let body = serde_json::json!({"userDetails":users}).to_string();
            match expected {
                Ok(selected) => {
                    assert_eq!(session.select_account(&body, "user", requested), Ok(()));
                    assert_eq!(session.account.as_deref(), Some(selected));
                    assert!(!session.discovery_pending);
                }
                Err(error) => {
                    assert_eq!(
                        session.select_account(&body, "user", requested),
                        Err(error.into())
                    );
                    assert_eq!(session.account.as_deref(), Some("default:old"));
                    assert!(session.discovery_pending);
                }
            }
        }
        // An unrelated malformed trailing user still invalidates the full JSON
        // contract before any matching/selection can commit an account.
        let mut session = Session {
            account: Some("default:old".into()),
            discovery_pending: true,
            ..Session::default()
        };
        let malformed = r#"{"userDetails":[{"login":"user","domain":"default","accounts":[{"account":"default:2","accountStatus":"FULL_TRADING"}]},{"login":"other","domain":"default","accounts":[{"account":"default:3"}]}]}"#;
        assert_eq!(
            session.select_account(malformed, "user", None),
            Err("Invalid DXtrade account discovery response".into())
        );
        assert_eq!(session.account.as_deref(), Some("default:old"));
        assert!(session.discovery_pending);
    }

    #[test]
    fn timeout_boundary_and_extra_components_remain_strict() {
        assert_eq!(timeout_ms("8760:00:00"), Ok(31_536_000_000.0));
        for text in [
            "8760:00:01",
            "00:30:00:",
            "00:30:00:00",
            "18446744073709551616:00:00",
            "00:٠١:00",
            "00:+1:00",
            "00::01",
            ":00:01",
        ] {
            assert_eq!(
                timeout_ms(text),
                Err("Invalid DXtrade session timeout".into()),
                "{text}"
            );
        }
    }
}
