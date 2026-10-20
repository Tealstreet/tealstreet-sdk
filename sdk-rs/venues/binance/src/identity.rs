//! Rare identity reads are independent of recurring account reconciliation.
use crate::{BinanceFutures, Pending, Variant, sign::Query};
use safe_cex_engine::{CommandId, Cx, Method, Response};
use serde::{Deserialize, Serialize};
#[derive(Deserialize)]
#[serde(untagged)]
enum Id {
    Text(String),
    Integer(u64),
}
impl Id {
    fn value(self) -> Option<String> {
        let value = match self {
            Self::Text(value) => value,
            Self::Integer(value) => value.to_string(),
        };
        (!value.trim().is_empty()).then_some(value)
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Identity {
    user_id: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Row {
    account_alias: Option<Id>,
}
fn parse(response: Response<'_>) -> Result<Identity, String> {
    if !(200..300).contains(&response.status) {
        return Err(format!("Binance account identity HTTP {}", response.status));
    }
    let rows: Vec<Row> = serde_json::from_str(response.body)
        .map_err(|_| "Binance account identity response is invalid")?;
    let user_id = rows
        .into_iter()
        .next()
        .and_then(|row| row.account_alias)
        .and_then(Id::value)
        .ok_or("Binance balance response did not include an account alias")?;
    Ok(Identity { user_id })
}
fn finish(cx: &mut Cx<'_, Pending>, command: CommandId, result: Result<Identity, String>) {
    let result = result
        .and_then(|identity| serde_json::to_string(&identity).map_err(|error| error.to_string()));
    if let Err(error) = &result {
        cx.command_error(command, error);
    }
    cx.done(command, result);
}
impl BinanceFutures {
    pub(crate) fn fetch_identity(&self, cx: &mut Cx<'_, Pending>, command: CommandId) {
        if let Some(read) = &self.options.read_credentials
            && let Err(error) = read.validate()
        {
            finish(cx, command, Err(error.into()));
            return;
        }
        let path = match self.options.variant {
            Variant::UsdM => "/fapi/v3/balance",
            Variant::PortfolioMargin => "/papi/v1/balance",
            _ => {
                finish(
                    cx,
                    command,
                    Err("Binance account identity is unsupported for this product".into()),
                );
                return;
            }
        };
        if self.options.public_data_only
            || self
                .credentials
                .as_ref()
                .is_none_or(|c| c.key.is_empty() || c.secret.is_empty())
        {
            finish(
                cx,
                command,
                Err("Binance account identity requires credentials".into()),
            );
            return;
        }
        self.signed(
            cx,
            Method::Get,
            path,
            Query::new(),
            Pending::Identity(command),
        );
    }
    pub(crate) fn identity_response(
        &self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        response: Response<'_>,
    ) {
        finish(cx, command, parse(response));
    }
    pub(crate) fn identity_failed(
        &self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        error: &str,
    ) {
        finish(cx, command, Err(error.into()));
    }
}
