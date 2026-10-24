//! Trusted KuCoin broker input is consumed before opaque venue construction.
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrokerInput {
    pub partner: String,
    #[serde(default)]
    pub broker_key: String,
    pub broker_name: String,
    pub verify: Option<bool>,
}
safe_cex_crypto::redacted_debug!(BrokerInput);
impl Drop for BrokerInput {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.broker_key.zeroize();
    }
}
impl BrokerInput {
    pub fn metadata(&self) -> safe_cex_venue_kucoin::sign::BrokerOptions {
        safe_cex_venue_kucoin::sign::BrokerOptions {
            partner: self.partner.clone(),
            broker_name: self.broker_name.clone(),
            verify: self.verify,
        }
    }
}
