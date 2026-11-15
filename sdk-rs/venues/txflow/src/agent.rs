//! Approval reads never carry a secret. Absence covers revocation and expiry.
use crate::{Pending, TxFlow};
use safe_cex_engine::request_context::Context;
use safe_cex_engine::{Cx, ErrorKind, Outcome, Response, VenueError};
use serde::Deserialize;
pub struct Request {
    pub context: Context,
}
#[derive(Deserialize)]
struct Reply {
    success: bool,
    code: i64,
    data: Data,
}
#[derive(Deserialize)]
struct Data {
    user: String,
    agents: Vec<Agent>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Agent {
    agent_address: String,
    agent_name: String,
}
pub fn approved(body: &str, owner: &str, agent: &str) -> Result<bool, String> {
    let reply: Reply =
        serde_json::from_str(body).map_err(|_| "Invalid TxFlow agent authorization response")?;
    let address = safe_cex_crypto::ethereum::get_address;
    if !reply.success || reply.code != 0 || address(&reply.data.user)? != address(owner)? {
        return Err("Invalid TxFlow agent authorization owner".into());
    }
    let expected = address(agent)?;
    let mut found = false;
    for row in reply.data.agents {
        if row.agent_name.chars().count() > 64 {
            return Err("Invalid TxFlow agent name".into());
        }
        let current = address(&row.agent_address)?;
        if current == expected {
            found = true;
        }
    }
    Ok(found)
}
impl TxFlow {
    pub(crate) fn agent_address(&self) -> Result<String, String> {
        let (_, identity) = self
            .credentials
            .select(safe_cex_engine::credential_selection::CredentialRole::Mutation)
            .map_err(|_| "TxFlow agent signing provider is required")?;
        let address = identity
            .verified_signing_address()
            .map_err(|_| "TxFlow agent address is not bound")?;
        Ok(format!("0x{}", safe_cex_crypto::hex(&address)))
    }
    pub(crate) fn approval_body(&self) -> String {
        serde_json::json!({"type":"approvedAgents", "user":self.options.application_id}).to_string()
    }
    pub(crate) fn agent_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        response: Result<Response<'_>, &str>,
    ) {
        let result = response
            .map_err(|error| VenueError::new(error).kind(ErrorKind::Network).outcome(Outcome::NotSent))
            .and_then(|response| {
                if response.status != 200 {
                    return Err(VenueError::new("TxFlow agent authorization read failed")
                        .kind(ErrorKind::VenueUnavailable).outcome(Outcome::NotSent));
                }
                let agent = self.agent_address().map_err(|error| VenueError::new(error)
                    .kind(ErrorKind::InvalidInput).outcome(Outcome::NotSent))?;
                approved(response.body, self.options.application_id.as_deref().unwrap_or(""), &agent)
                    .map_err(|error| VenueError::new(error)
                        .kind(ErrorKind::VenueUnavailable).outcome(Outcome::NotSent))
            })
            .and_then(|present| {
                if present { Ok(()) } else {
                    Err(VenueError::new("TxFlow agent is not approved (revoked or expired); approve it with the owner wallet")
                        .kind(ErrorKind::AuthInvalid).outcome(Outcome::NotSent))
                }
            });
        self.trade_agent_response(cx, request.context, result);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    const OWNER: &str = "0x1111111111111111111111111111111111111111";
    const AGENT: &str = "0x2222222222222222222222222222222222222222";
    #[test]
    fn absent_revoked_or_expired_agent_is_not_authorized() {
        let reply =
            format!(r#"{{"success":true,"code":0,"data":{{"user":"{OWNER}","agents":[]}}}}"#);
        assert!(!approved(&reply, OWNER, AGENT).unwrap());
        assert!(approved(&reply, AGENT, AGENT).is_err());
        let reply = format!(
            r#"{{"success":true,"code":0,"data":{{"user":"{OWNER}","agents":[{{"agentAddress":"{AGENT}","agentName":"test"}}]}}}}"#
        );
        assert!(approved(&reply, OWNER, AGENT).unwrap());
    }
}
