// Generated direct native projections; no model Serialize/roundtrip.
impl Native for WsFill {
    fn native(&self) -> Value {
        Value::Object(
            [
                ("coin".into(), serde_json::json!((&self.coin).0)),
                ("px".into(), serde_json::json!((&self.px).0)),
                ("sz".into(), serde_json::json!((&self.sz).0)),
                ("side".into(), serde_json::json!((&self.side).0)),
                ("time".into(), number(&self.time)),
                (
                    "startPosition".into(),
                    serde_json::json!((&self.start_position).0),
                ),
                ("dir".into(), serde_json::json!((&self.dir).0)),
                ("closedPnl".into(), serde_json::json!((&self.closed_pnl).0)),
                ("hash".into(), serde_json::json!((&self.hash).0)),
                ("oid".into(), number(&self.oid)),
                ("crossed".into(), serde_json::json!(&self.crossed)),
                ("fee".into(), serde_json::json!((&self.fee).0)),
                ("tid".into(), number(&self.tid)),
            ]
            .into_iter()
            .collect(),
        )
    }
}
impl Native for WsUserFunding {
    fn native(&self) -> Value {
        Value::Object(
            [
                ("time".into(), number(&self.time)),
                ("coin".into(), serde_json::json!((&self.coin).0)),
                ("usdc".into(), serde_json::json!((&self.usdc).0)),
                ("szi".into(), serde_json::json!((&self.szi).0)),
                (
                    "fundingRate".into(),
                    serde_json::json!((&self.funding_rate).0),
                ),
            ]
            .into_iter()
            .collect(),
        )
    }
}
impl Native for WsLiquidation {
    fn native(&self) -> Value {
        Value::Object(
            [
                ("lid".into(), number(&self.lid)),
                ("liquidator".into(), serde_json::json!((&self.liquidator).0)),
                (
                    "liquidated_user".into(),
                    serde_json::json!((&self.liquidated_user).0),
                ),
                (
                    "liquidated_ntl_pos".into(),
                    serde_json::json!((&self.liquidated_ntl_pos).0),
                ),
                (
                    "liquidated_account_value".into(),
                    serde_json::json!((&self.liquidated_account_value).0),
                ),
            ]
            .into_iter()
            .collect(),
        )
    }
}
impl Native for WsNonUserCancel {
    fn native(&self) -> Value {
        Value::Object(
            [
                ("coin".into(), serde_json::json!((&self.coin).0)),
                ("oid".into(), number(&self.oid)),
            ]
            .into_iter()
            .collect(),
        )
    }
}
