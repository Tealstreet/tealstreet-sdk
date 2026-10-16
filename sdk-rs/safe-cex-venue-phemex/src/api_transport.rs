//! Factory and request interceptor preserve opaque options/config/header objects.
use safe_cex_num::{add, divide1, js_number_string, math_round};
pub trait RequestHost {
    type Config;
    type Value;
    type Hmac;
    type Error;
    fn copy(&mut self, config: &Self::Config) -> Result<Self::Config, Self::Error>;
    fn remote(&mut self) -> Result<bool, Self::Error>;
    fn key(&mut self) -> Result<Self::Value, Self::Error>;
    fn secret(&mut self) -> Result<Self::Value, Self::Error>;
    fn truthy(&mut self, value: &Self::Value) -> Result<bool, Self::Error>;
    fn params(&mut self, config: &Self::Config) -> Result<Option<Vec<u16>>, Self::Error>;
    fn data(&mut self, config: &Self::Config) -> Result<Option<Vec<u16>>, Self::Error>;
    fn clock(&mut self) -> Result<f64, Self::Error>;
    fn url(&mut self, config: &Self::Config) -> Result<Self::Value, Self::Error>;
    fn create_hmac(&mut self, secret: Self::Value) -> Result<Self::Hmac, Self::Error>;
    fn join(
        &mut self,
        url: Self::Value,
        params: Vec<u16>,
        expiry: &[u16],
        data: Option<Vec<u16>>,
    ) -> Result<Vec<u16>, Self::Error>;
    fn hmac(&mut self, hmac: Self::Hmac, payload: Vec<u16>) -> Result<Self::Value, Self::Error>;
    fn headers(
        &mut self,
        config: Self::Config,
        key: Self::Value,
        expiry: Vec<u16>,
        signature: Self::Value,
    ) -> Result<Self::Config, Self::Error>;
}
pub fn sign_request<H: RequestHost>(
    host: &mut H,
    config: H::Config,
) -> Result<H::Config, H::Error> {
    let copy = host.copy(&config)?;
    if host.remote()? {
        return Ok(copy);
    }
    let key = host.key()?;
    if !host.truthy(&key)? {
        return Ok(copy);
    }
    let secret = host.secret()?;
    if !host.truthy(&secret)? {
        return Ok(copy);
    }
    let params = host.params(&config)?.unwrap_or_default();
    let data = host.data(&config)?.filter(|v| !v.is_empty());
    let timestamp = host.clock()?;
    let expiry = js_number_string(math_round(divide1(add(timestamp, 60_000.0), 1000.0)))
        .encode_utf16()
        .collect::<Vec<_>>();
    let url = host.url(&config)?;
    let secret = host.secret()?;
    let hmac = host.create_hmac(secret)?;
    let payload = host.join(url, params, &expiry, data)?;
    let signature = host.hmac(hmac, payload)?;
    let key = host.key()?;
    host.headers(copy, key, expiry, signature)
}
pub trait ApiHost: RequestHost {
    type Client;
    type RetryCondition;
    fn testnet(&mut self) -> Result<bool, Self::Error>;
    fn create(
        &mut self,
        base: &'static str,
        timeout: f64,
        credentials: bool,
    ) -> Result<Self::Client, Self::Error>;
    fn retry_condition(&mut self) -> Result<Self::RetryCondition, Self::Error>;
    fn retry(
        &mut self,
        client: &Self::Client,
        retries: usize,
        condition: Self::RetryCondition,
    ) -> Result<(), Self::Error>;
    fn tunnel(&mut self) -> Result<bool, Self::Error>;
    fn install(&mut self, client: &Self::Client) -> Result<(), Self::Error>;
    fn interceptor(
        &mut self,
        client: &Self::Client,
        callback: fn(&mut Self, Self::Config) -> Result<Self::Config, Self::Error>,
    ) -> Result<(), Self::Error>
    where
        Self: Sized;
}
pub fn create_api<H: ApiHost>(host: &mut H) -> Result<H::Client, H::Error> {
    let network = if host.testnet()? {
        "testnet"
    } else {
        "livenet"
    };
    let base = crate::constants::BASE_URL
        .iter()
        .find(|(key, _)| *key == network)
        .unwrap()
        .1;
    let client = host.create(base, 20_000.0, false)?;
    let condition = host.retry_condition()?;
    host.retry(&client, 3, condition)?;
    if host.tunnel()? {
        host.install(&client)?;
    }
    host.interceptor(&client, sign_request::<H>)?;
    Ok(client)
}
