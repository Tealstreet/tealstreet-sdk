//! Rare identity reads are independent of recurring account reconciliation.
use crate::{BinanceFutures, Pending, Variant};
use safe_cex_engine::credential_selection::CredentialRole;
use safe_cex_engine::request_context::Context;
use safe_cex_engine::{CommandId, Cx, Response, VenueError};
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
fn finish_unissued(cx: &mut Cx<'_, Pending>, command: CommandId, result: Result<Identity, String>) {
    let result = result
        .and_then(|identity| serde_json::to_string(&identity).map_err(|error| error.to_string()));
    match result {
        Ok(data) => cx.done(command, data),
        Err(error) => {
            cx.command_error(command, &error);
            cx.fail(command, error);
        }
    }
}
impl BinanceFutures {
    pub(crate) fn fetch_identity(&mut self, cx: &mut Cx<'_, Pending>, command: CommandId) {
        if let Some(read) = &self.options.read_credentials
            && let Err(error) = read.validate()
        {
            finish_unissued(cx, command, Err(error.into()));
            return;
        }
        let path = match self.options.variant {
            Variant::UsdM => "/fapi/v3/balance",
            Variant::PortfolioMargin => "/papi/v1/balance",
            _ => {
                finish_unissued(
                    cx,
                    command,
                    Err("Binance account identity is unsupported for this product".into()),
                );
                return;
            }
        };
        if self.options.public_data_only || !self.credentials.has_primary() {
            finish_unissued(
                cx,
                command,
                Err("Binance account identity requires credentials".into()),
            );
            return;
        }
        let (selection, credentials) = self
            .credentials
            .select(CredentialRole::Read { scope: None })
            .expect("validated above");
        let credentials = credentials.clone();
        let context = match Self::read_context(
            cx,
            Some(selection.credential(credentials.credential.clone())),
        ) {
            Ok(context) => context,
            Err(_) => return, // Engine owns initial admission rejection.
        };
        self.managed_read(
            cx,
            context.clone(),
            crate::reads::Read {
                url: format!("{}{path}", self.urls.account_rest),
                credentials: Some(credentials),
                query: crate::Query::new(),
                stream: None,
            },
            Pending::Identity(context),
        );
    }
    pub(crate) fn identity_response(
        &self,
        cx: &mut Cx<'_, Pending>,
        context: Context,
        response: Response<'_>,
    ) {
        let result = parse(response).and_then(|identity| {
            serde_json::to_string(&identity).map_err(|error| error.to_string())
        });
        match result {
            Ok(data) => {
                let _ = cx.done_context(&context, data);
            }
            Err(error) => self.identity_error(cx, &context, error.into()),
        }
    }
    pub(crate) fn identity_error(
        &self,
        cx: &mut Cx<'_, Pending>,
        context: &Context,
        error: VenueError,
    ) {
        if cx
            .command_error_context(context, error.message.clone())
            .is_ok()
        {
            let _ = cx.fail_context(context, error);
        }
    }
}
