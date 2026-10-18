//! Private transport lifecycle over original handles and model references.
use safe_cex_num::{add, divide1, js_number_string, math_round};
use std::{
    future::{Future, poll_fn},
    pin::Pin,
    task::Poll,
};
fn units(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}
fn expiry(timestamp: f64) -> Vec<u16> {
    units(&js_number_string(math_round(divide1(
        add(timestamp, 60_000.0),
        1000.0,
    ))))
}
pub trait SessionHost {
    type Timer;
    type Symbol;
    type Symbols;
    type Socket;
    type Call;
    type Error;
    fn timeout(&mut self) -> Result<Option<Self::Timer>, Self::Error>;
    fn timeout_value(&mut self) -> Result<Self::Timer, Self::Error>;
    fn clear_timeout(&mut self, timer: Self::Timer) -> Result<(), Self::Error>;
    fn unset_timeout(&mut self) -> Result<(), Self::Error>;
    fn clear_symbols(&mut self) -> Result<(), Self::Error>;
    fn base_dispose(&mut self) -> Result<(), Self::Error>;
    fn empty_symbols(&mut self) -> Result<Self::Symbols, Self::Error>;
    fn symbols_length(&mut self, symbols: &Self::Symbols) -> Result<usize, Self::Error>;
    fn symbol_at(
        &mut self,
        symbols: &Self::Symbols,
        index: usize,
    ) -> Result<Option<Self::Symbol>, Self::Error>;
    fn add_symbol(&mut self, symbol: Self::Symbol) -> Result<(), Self::Error>;
    fn debounce(&mut self) -> Result<f64, Self::Error>;
    fn schedule(&mut self, delay: f64, member: &'static str) -> Result<Self::Timer, Self::Error>;
    fn set_timeout(&mut self, timer: Self::Timer) -> Result<(), Self::Error>;
    fn disposed(&mut self) -> Result<bool, Self::Error>;
    fn testnet(&mut self) -> Result<bool, Self::Error>;
    fn create(&mut self, url: &'static str) -> Result<Self::Socket, Self::Error>;
    fn set_socket(&mut self, socket: Self::Socket) -> Result<(), Self::Error>;
    fn listener(&mut self, event: &'static str, member: &'static str) -> Result<(), Self::Error>;
    fn start_auth(&mut self) -> Result<(), Self::Error>;
    fn ping(&mut self) -> Result<(), Self::Error>;
    fn now(&mut self) -> Result<f64, Self::Error>;
    fn set_ping_at(&mut self, time: f64) -> Result<(), Self::Error>;
    fn send_target(&mut self) -> Result<Self::Call, Self::Error>;
    fn next_id(&mut self) -> Result<f64, Self::Error>;
    fn send(&mut self, call: Self::Call, text: Vec<u16>) -> Result<(), Self::Error>;
}
pub fn dispose<H: SessionHost>(host: &mut H) -> Result<(), H::Error> {
    if host.timeout()?.is_some() {
        let timer = host.timeout_value()?;
        host.clear_timeout(timer)?;
        host.unset_timeout()?;
    }
    host.clear_symbols()?;
    host.base_dispose()
}
pub fn schedule_positions_and_balance_sync<H: SessionHost>(
    host: &mut H,
    symbols: Option<H::Symbols>,
) -> Result<(), H::Error> {
    let symbols = match symbols {
        Some(symbols) => symbols,
        None => host.empty_symbols()?,
    };
    let length = host.symbols_length(&symbols)?;
    for index in 0..length {
        if let Some(symbol) = host.symbol_at(&symbols, index)? {
            host.add_symbol(symbol)?;
        }
    }
    if host.timeout()?.is_some() {
        let timer = host.timeout_value()?;
        host.clear_timeout(timer)?;
    }
    let delay = host.debounce()?;
    let timer = host.schedule(delay, "syncPositionsAndBalance")?;
    host.set_timeout(timer)
}
pub fn connect_and_subscribe<H: SessionHost>(host: &mut H) -> Result<(), H::Error> {
    if !host.disposed()? {
        let key = if host.testnet()? {
            "testnet"
        } else {
            "livenet"
        };
        let base = crate::constants::BASE_WS_URL
            .iter()
            .find(|(name, _)| *name == "private")
            .unwrap()
            .1
            .iter()
            .find(|(name, _)| *name == key)
            .unwrap()
            .1;
        let socket = host.create(base)?;
        host.set_socket(socket)?;
        for (event, member) in [
            ("open", "onOpen"),
            ("message", "onMessage"),
            ("close", "onClose"),
        ] {
            host.listener(event, member)?;
        }
    }
    Ok(())
}
pub fn on_open<H: SessionHost>(host: &mut H) -> Result<(), H::Error> {
    if !host.disposed()? {
        host.start_auth()?;
        host.ping()?;
    }
    Ok(())
}
pub fn ping<H: SessionHost>(host: &mut H) -> Result<(), H::Error> {
    if !host.disposed()? {
        let now = host.now()?;
        host.set_ping_at(now)?;
        let call = host.send_target()?;
        let id = host.next_id()?;
        let text = units("{\"id\":")
            .into_iter()
            .chain(units(&js_number_string(id)))
            .chain(units(",\"method\":\"server.ping\",\"params\":[]}"))
            .collect();
        host.send(call, text)?;
    }
    Ok(())
}
pub trait AuthHost {
    type Value;
    type Signer;
    type Hmac;
    type Request;
    type Reply;
    type Call;
    type Error;
    fn clock(&mut self) -> Result<f64, Self::Error>;
    fn remote(&mut self) -> Result<bool, Self::Error>;
    fn signer_target(&mut self) -> Result<Self::Signer, Self::Error>;
    fn nonce(&mut self) -> Result<Self::Value, Self::Error>;
    fn account(&mut self) -> Result<Self::Value, Self::Error>;
    fn key(&mut self) -> Result<Self::Value, Self::Error>;
    fn key_text(&mut self, key: Self::Value, template: bool) -> Result<Vec<u16>, Self::Error>;
    fn number_text(&mut self, text: &[u16]) -> Result<f64, Self::Error>;
    fn request(
        &mut self,
        id: Self::Value,
        account: Self::Value,
        to_sign: Vec<u16>,
        expiry: f64,
    ) -> Result<Self::Request, Self::Error>;
    fn sign(
        &mut self,
        signer: Self::Signer,
        request: Self::Request,
    ) -> impl Future<Output = Result<Self::Reply, Self::Error>>;
    fn reply_error(&mut self, reply: &Self::Reply) -> Result<bool, Self::Error>;
    fn error_text(&mut self, reply: &Self::Reply) -> Result<Vec<u16>, Self::Error>;
    fn emit_error(&mut self, message: Vec<u16>) -> Result<(), Self::Error>;
    fn signature(&mut self, reply: Self::Reply) -> Result<Self::Value, Self::Error>;
    fn secret(&mut self) -> Result<Self::Value, Self::Error>;
    fn create_hmac(&mut self, secret: Self::Value) -> Result<Self::Hmac, Self::Error>;
    fn hmac(&mut self, hmac: Self::Hmac, message: Vec<u16>) -> Result<Self::Value, Self::Error>;
    fn send_target(&mut self) -> Result<Self::Call, Self::Error>;
    fn serialize(
        &mut self,
        key: Self::Value,
        signature: Self::Value,
        expiry: f64,
        id: f64,
    ) -> Result<Vec<u16>, Self::Error>;
    fn send(&mut self, call: Self::Call, text: Vec<u16>) -> Result<(), Self::Error>;
}
pub async fn auth<H: AuthHost>(host: &mut H) -> Result<(), H::Error> {
    let timestamp = host.clock()?;
    let expiry = expiry(timestamp);
    let signature = if host.remote()? {
        let signer = host.signer_target()?;
        let id = host.nonce()?;
        let account = host.account()?;
        let key = host.key()?;
        let mut to_sign = host.key_text(key, true)?;
        to_sign.extend(&expiry);
        let number = host.number_text(&expiry)?;
        let request = host.request(id, account, to_sign, number)?;
        let reply = host.sign(signer, request).await?;
        if host.reply_error(&reply)? {
            let error = host.error_text(&reply)?;
            host.emit_error(
                units("Remote WS signing unavailable: ")
                    .into_iter()
                    .chain(error)
                    .collect(),
            )?;
            return Ok(());
        }
        host.signature(reply)?
    } else {
        let key = host.key()?;
        let secret = host.secret()?;
        let hmac = host.create_hmac(secret)?;
        let mut to_sign = host.key_text(key, false)?;
        to_sign.extend(&expiry);
        host.hmac(hmac, to_sign)?
    };
    let call = host.send_target()?;
    let key = host.key()?;
    let number = host.number_text(&expiry)?;
    let text = host.serialize(key, signature, number, 999.0)?;
    host.send(call, text)
}
pub trait SyncHost {
    type Symbol;
    type Balance;
    type Position: Clone;
    type Positions;
    type Account;
    type Id;
    type Error;
    type Read: Future<Output = Result<Self::Positions, Self::Error>>;
    fn unset_timeout(&mut self) -> Result<(), Self::Error>;
    fn symbols(&mut self) -> Result<Vec<Self::Symbol>, Self::Error>;
    fn clear_symbols(&mut self) -> Result<(), Self::Error>;
    fn disposed(&mut self) -> Result<bool, Self::Error>;
    type AccountRead: Future<Output = Result<Self::Account, Self::Error>>;
    fn account(&mut self) -> Self::AccountRead;
    fn parts(
        &mut self,
        account: Self::Account,
    ) -> Result<(Self::Balance, Self::Positions), Self::Error>;
    fn fetch(&mut self, symbol: Self::Symbol) -> Result<Self::Read, Self::Error>;
    fn rows(&mut self, positions: Self::Positions) -> Result<Vec<Self::Position>, Self::Error>;
    fn id(&mut self, position: &Self::Position) -> Result<Self::Id, Self::Error>;
    fn same_id(&mut self, a: &Self::Id, b: &Self::Id) -> Result<bool, Self::Error>;
    fn store_positions(&mut self, positions: Vec<Self::Position>) -> Result<(), Self::Error>;
    fn update(
        &mut self,
        balance: Self::Balance,
        balance_loaded: bool,
        positions_loaded: bool,
    ) -> Result<(), Self::Error>;
}
pub(crate) struct SyncStart<S, R> {
    symbols: Vec<S>,
    account: R,
}
pub(crate) fn begin_sync<H: SyncHost>(
    host: &mut H,
) -> Result<Option<SyncStart<H::Symbol, H::AccountRead>>, H::Error> {
    host.unset_timeout()?;
    let symbols = host.symbols()?;
    host.clear_symbols()?;
    if host.disposed()? {
        return Ok(None);
    }
    Ok(Some(SyncStart {
        symbols,
        account: host.account(),
    }))
}
pub fn sync_positions_and_balance<H: SyncHost>(
    host: &mut H,
) -> impl Future<Output = Result<(), H::Error>> {
    // The source async function runs this prefix before returning its Promise.
    let started = begin_sync(host);
    async move { finish_sync(host, started?).await }
}
pub(crate) async fn finish_sync<H: SyncHost>(
    host: &mut H,
    started: Option<SyncStart<H::Symbol, H::AccountRead>>,
) -> Result<(), H::Error> {
    let Some(SyncStart { symbols, account }) = started else {
        return Ok(());
    };
    let account = account.await?;
    let (balance, positions) = host.parts(account)?;
    if host.disposed()? {
        return Ok(());
    }
    let mut reads = Vec::new();
    for symbol in symbols {
        reads.push(Some(Box::pin(host.fetch(symbol)?)));
    }
    let mut outcomes: Vec<Option<Result<H::Positions, H::Error>>> =
        (0..reads.len()).map(|_| None).collect();
    poll_fn(|cx| {
        let mut pending = false;
        for (read, outcome) in reads.iter_mut().zip(outcomes.iter_mut()) {
            if let Some(future) = read {
                match Pin::as_mut(future).poll(cx) {
                    Poll::Ready(result) => {
                        *outcome = Some(result);
                        *read = None
                    }
                    Poll::Pending => pending = true,
                }
            }
        }
        if pending {
            Poll::Pending
        } else {
            Poll::Ready(())
        }
    })
    .await;
    let mut symbol_positions = Vec::new();
    for outcome in outcomes {
        if let Some(Ok(positions)) = outcome {
            symbol_positions.extend(host.rows(positions)?);
        }
    }
    let mut symbol_ids = Vec::new();
    for position in &symbol_positions {
        symbol_ids.push(host.id(position)?);
    }
    let mut merged = Vec::new();
    for position in host.rows(positions)? {
        let id = host.id(&position)?;
        let mut replaced = false;
        for symbol_id in &symbol_ids {
            if host.same_id(symbol_id, &id)? {
                replaced = true;
                break;
            }
        }
        if !replaced {
            merged.push(position);
        }
    }
    merged.extend(symbol_positions);
    let mut seen = Vec::new();
    let mut unique = Vec::new();
    for position in merged {
        let id = host.id(&position)?;
        let mut duplicate = false;
        for existing in &seen {
            if host.same_id(existing, &id)? {
                duplicate = true;
                break;
            }
        }
        if !duplicate {
            seen.push(id);
            unique.push(position);
        }
    }
    host.store_positions(unique)?;
    host.update(balance, true, true)
}

#[cfg(test)]
mod accepted_native_contract {
    use super::*;

    #[test]
    fn utf16_encoder_preserves_native_units() {
        assert_eq!(units(""), Vec::<u16>::new());
        assert_eq!(units("A🚀\0"), vec![65, 55357, 56960, 0]);
        assert_eq!(units("é雪"), vec![233, 38634]);
    }
}
