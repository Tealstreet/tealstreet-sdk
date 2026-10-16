// Direct native fields/tags; no model serde serialization.
impl Native for CacheOrderType {
    fn native(&self) -> Value {
        match self {
            Self::Market => serde_json::json!("market"),
            Self::Limit => serde_json::json!("limit"),
            Self::Conditional => serde_json::json!("conditional"),
            Self::StopLoss => serde_json::json!("stop_market"),
            Self::TakeProfit => serde_json::json!("take_profit_market"),
            Self::TrailingStopLoss => serde_json::json!("trailing_stop_market"),
        }
    }
}
impl Native for HyperliquidFastAssetCtx {
    fn native(&self) -> Value {
        serde_json::json!({"markPx":self.mark_px.as_ref().map(|v|(v).native()).unwrap_or_else(||serde_json::json!({"absent":true})),"midPx":self.mid_px.as_ref().map(|v|(v).native()).unwrap_or_else(||serde_json::json!({"absent":true}))})
    }
}
impl Native for PrePlacementEntry {
    fn native(&self) -> Value {
        serde_json::json!({"type":(&self.r#type).native(),"price":bits(&self.price),"amount":bits(&self.amount),"timestamp":bits(&self.timestamp)})
    }
}
impl Native for PendingOrderEntry {
    fn native(&self) -> Value {
        serde_json::json!({"type":(&self.r#type).native(),"price":bits(&self.price),"timestamp":bits(&self.timestamp)})
    }
}
impl Native for AgentWalletCreationResult {
    fn native(&self) -> Value {
        serde_json::json!({"success":serde_json::json!(&self.success),"message":(&self.message).native(),"agentAddress":self.agent_address.as_ref().map(|v|(v).native()).unwrap_or_else(||serde_json::json!({"absent":true})),"agentPrivateKey":self.agent_private_key.as_ref().map(|v|(v).native()).unwrap_or_else(||serde_json::json!({"absent":true}))})
    }
}
