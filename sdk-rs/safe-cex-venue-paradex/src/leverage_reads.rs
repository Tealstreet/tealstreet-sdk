//! Leverage snapshot updates preserve prior entries and the source catch policy.
use crate::{constants::ENDPOINTS, types::MarginResponse};
use std::future::Future;

// ts: ParadexExchange.fetchLeverageSettings
pub async fn fetch_leverage_settings<E, F>(
    get: impl FnOnce(&'static str) -> Result<F, E>,
    mut set: impl FnMut(&str, f64) -> Result<(), E>,
    mut stringify_error: impl FnMut(&E) -> Result<String, E>,
    mut log: impl FnMut(String, &'static str) -> Result<(), E>,
) -> Result<(), E>
where
    F: Future<Output = Result<Option<MarginResponse>, E>>,
{
    let result = async {
        log("Fetching leverage settings from API...".into(), "debug")?;
        let endpoint = ENDPOINTS
            .iter()
            .find(|(key, _)| *key == "ALL_MARGINS")
            .unwrap()
            .1;
        let Some(data) = get(endpoint)?.await? else {
            log("Margin API returned invalid data".into(), "warning")?;
            return Ok(());
        };
        let mut updated_count = 0;
        for config in data.configs {
            if config.leverage > 0.0 {
                set(&config.market, config.leverage)?;
                updated_count += 1;
            }
        }
        log(
            format!("Updated leverage settings for {updated_count} markets"),
            "debug",
        )?;
        Ok(())
    }
    .await;
    if let Err(error) = result {
        let text = stringify_error(&error)?;
        log(format!("Error fetching leverage settings: {text}"), "error")?;
        // The source discards the caught failure after logging; failures in
        // catch itself propagate, and earlier map updates remain visible.
    }
    Ok(())
}
