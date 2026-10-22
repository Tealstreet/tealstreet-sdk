//! Host-only compiler for the JSON shared with the TypeScript resolver.
use std::collections::BTreeMap;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
enum Input {
    Key,
    Secret,
    RemoteSigner,
    PayloadSigner,
    ApplicationId,
    Vault,
    ParadexAddress,
    SubkeyAddress,
    ApiKeyIndex,
    AsterLocalAuth,
    CanSign,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Rule {
    can_sign: Option<Vec<Vec<Input>>>,
    private_eligible: Option<Vec<Vec<Input>>>,
    explicit_private_error: Option<String>,
    #[serde(default)]
    bridge: BridgeRule,
}

#[derive(Default, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
struct BridgeRule {
    spot: bool,
    demo: DemoRule,
    primary_credentials_only: bool,
    payload_signing: bool,
    api_key_index_required: bool,
    isolated_book_subscriptions: bool,
    dispatch_signing: bool,
}

#[derive(Default, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
enum DemoRule {
    #[default]
    Never,
    Always,
    HyroMode,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Rules {
    default: Rule,
    venues: BTreeMap<String, Rule>,
}

fn conditions(value: &Option<Vec<Vec<Input>>>) -> String {
    let Some(groups) = value else {
        return "None".into();
    };
    let groups = groups
        .iter()
        .map(|group| {
            let inputs = group
                .iter()
                .map(|input| format!("Input::{input:?}"))
                .collect::<Vec<_>>()
                .join(",");
            format!("&[{inputs}]")
        })
        .collect::<Vec<_>>()
        .join(",");
    format!("Some(&[{groups}])")
}

fn rule(value: &Rule) -> String {
    let error = match &value.explicit_private_error {
        Some(error) => format!("Some({error:?})"),
        None => "None".into(),
    };
    let bridge = &value.bridge;
    let bridge = format!(
        "BridgeRule {{ spot: {}, demo: DemoRule::{:?}, primary_credentials_only: {}, payload_signing: {}, api_key_index_required: {}, isolated_book_subscriptions: {}, dispatch_signing: {} }}",
        bridge.spot,
        bridge.demo,
        bridge.primary_credentials_only,
        bridge.payload_signing,
        bridge.api_key_index_required,
        bridge.isolated_book_subscriptions,
        bridge.dispatch_signing
    );
    format!(
        "Rule {{ can_sign: {}, private_eligible: {}, explicit_private_error: {error}, bridge: {bridge} }}",
        conditions(&value.can_sign),
        conditions(&value.private_eligible),
    )
}

pub fn generate(json: &str) -> Result<String, String> {
    let rules: Rules = serde_json::from_str(json).map_err(|error| error.to_string())?;
    if rules.default.can_sign.is_none() || rules.default.private_eligible.is_none() {
        return Err("default signing and private conditions are required".into());
    }
    let mut output = format!("const DEFAULT_RULE: Rule = {};\n", rule(&rules.default));
    output.push_str("fn venue_rule(venue: &str) -> Option<&'static Rule> { match venue {\n");
    for (venue, value) in rules.venues {
        output.push_str(&format!("{venue:?} => Some(&{}),\n", rule(&value)));
    }
    output.push_str("_ => None,\n} }\n");
    Ok(output)
}
