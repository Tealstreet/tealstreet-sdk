//! WebAssembly host surface for browser workers and Cloudflare Workers.
//!
//! JavaScript owns sockets, fetch and timers; it forwards each event here and
//! executes the returned actions. Every call returns the pending actions as a
//! JSON array string, or an empty string when there are none. `wsMessageActions`
//! and `wsMessageBytes` return `undefined` instead, avoiding empty-string decoding.
#[cfg(feature = "hosted-operation-proof")]
mod hosted_operation_proof;
#[cfg(feature = "kraken")]
mod hosted_operations;
#[cfg(not(any(
    feature = "grvt",
    feature = "paradex",
    feature = "bitrue",
    feature = "bitunix",
    feature = "coinbase",
    feature = "binance",
    feature = "apexomni",
    feature = "asterdex",
    feature = "breakoutprop",
    feature = "kucoin",
    feature = "kucoinspot",
    feature = "polymarket",
    feature = "blofin",
    feature = "bybit",
    feature = "woo",
    feature = "cryptocom",
    feature = "bingx",
    feature = "mexc",
    feature = "bitget",
    feature = "backpack",
    feature = "phemex",
    feature = "weex",
    feature = "hyperliquid",
    feature = "kraken",
    feature = "txflow",
    feature = "orderly",
    feature = "lighter",
    feature = "okx"
)))]
compile_error!(
    "safe-cex-wasm requires at least one venue feature: breakoutprop, paradex, apexomni, asterdex, bitrue, binance, bitunix, kucoin, kucoinspot, bybit, cryptocom, bingx, blofin, mexc, backpack, bitget, grvt, hyperliquid, kraken, txflow, coinbase, lighter, orderly, polymarket, okx, phemex, woo or weex"
);

use safe_cex_engine::{
    Action, Engine, Event, Health, Notice, RequestId, SocketId, TimerId, Value, Venue,
};
use serde::Serialize;
use wasm_bindgen::prelude::*;

mod managed_http;
use managed_http::{HttpPacket, HttpPhaseOut};

trait AnyEngine {
    #[cfg(feature = "address-nonce")]
    fn has_started(&self) -> bool {
        false
    }
    #[cfg(feature = "address-nonce")]
    fn address_setup_support(
        &mut self,
        _supported: bool,
    ) -> Result<(), safe_cex_engine::VenueError> {
        Err(safe_cex_engine::signing::SignError::Unsupported.venue_error())
    }
    #[cfg(feature = "address-nonce")]
    fn initialize_address_setup(
        &mut self,
        _now: f64,
        _reinitialize: bool,
    ) -> Result<(), safe_cex_engine::VenueError> {
        Err(safe_cex_engine::signing::SignError::Unsupported.venue_error())
    }
    #[cfg(feature = "address-nonce")]
    fn address_setup_ready(&self) -> bool {
        false
    }
    #[cfg(feature = "address-nonce")]
    fn address_setup_retired(&mut self, _token: &str, _now: f64) -> bool {
        false
    }

    fn failed(&self) -> bool {
        false
    }
    #[cfg(any(feature = "hosted-signing", feature = "hosted-wallet"))]
    fn signer(&self) -> Option<safe_cex_engine::signing::Provider> {
        None
    }
    #[cfg(any(feature = "hosted-signing", feature = "hosted-wallet"))]
    fn signing_clock(&self, now: f64) -> (f64, u64) {
        (now, 0)
    }
    /// `None` leaves host capability undeclared (unknown), so admission fails closed.
    fn external_signer(
        &mut self,
        algorithms: Option<Vec<safe_cex_engine::signing::Algorithm>>,
    ) -> Result<(), safe_cex_engine::VenueError>;
    fn prepare_dispatch(&mut self, now: f64);
    fn set_entropy(
        &mut self,
        _fill: safe_cex_engine::entropy::Fill,
        _signing: bool,
    ) -> Result<(), safe_cex_engine::VenueError> {
        Err(entropy_configuration_error())
    }

    fn signing_pending(&self, _: RequestId) -> bool {
        false
    }
    #[cfg(feature = "sign-meta")]
    fn sign_meta(&self, _: RequestId) -> Option<safe_cex_engine::sign_meta::SignMeta> {
        None
    }
    fn http_preparation_pending(&self, _: safe_cex_engine::request_context::Preparation) -> bool {
        false
    }
    fn http_prepared(
        &mut self,
        _: f64,
        _: safe_cex_engine::request_context::Preparation,
        _: safe_cex_engine::request_context::Prepared,
    ) {
    }
    fn http_preparation_failed(
        &mut self,
        _: f64,
        _: safe_cex_engine::request_context::Preparation,
        _: safe_cex_engine::request_context::Error,
        _: safe_cex_engine::VenueError,
    ) {
    }
    #[cfg(feature = "http-mutation-barriers")]
    fn http_group_pending(&self, _: &safe_cex_engine::request_context::GroupPermit) -> bool {
        false
    }
    #[cfg(feature = "http-mutation-barriers")]
    fn claim_http_group(
        &mut self,
        _: f64,
        _: safe_cex_engine::request_context::GroupPermit,
    ) -> Result<safe_cex_engine::request_context::GroupClaim, safe_cex_engine::request_context::Error>
    {
        Err(safe_cex_engine::request_context::Error::Invalid)
    }
    fn claim_http(
        &mut self,
        _: f64,
        _: safe_cex_engine::request_context::Permit,
    ) -> Result<safe_cex_engine::request_context::Claim, safe_cex_engine::request_context::Error>
    {
        Err(safe_cex_engine::request_context::Error::Revoked)
    }
    fn capabilities(&self) -> safe_cex_engine::Capabilities;
    fn apply_public_feed(
        &mut self,
        now: f64,
        update: safe_cex_engine::PublicFeedUpdate,
    ) -> Result<(), &'static str>;
    fn start(&mut self, now: f64);
    fn handle(&mut self, now: f64, event: Event<'_>);
    /// The data source's clock offset for a frame-fed engine.
    fn set_source_clock_offset(&mut self, _offset_ms: Option<f64>) {}
    /// A data source's frame into a frame-fed engine (`Engine::apply_frame`).
    fn apply_frame(&mut self, _now: f64, _frame: &[u8], _snapshot: bool) -> Result<(), String> {
        Err("frame ingest requires a frameFed engine".into())
    }
    /// A host command with the caller's wait (see `Engine::handle_command`).
    fn handle_command(
        &mut self,
        now: f64,
        command: safe_cex_engine::Command,
        wait_ms: Option<u32>,
    ) -> Result<(), safe_cex_engine::request_context::Error> {
        let _ = wait_ms;
        self.handle(now, Event::Command(command));
        Ok(())
    }
    fn drain(&mut self) -> Vec<Action>;
    fn frame(&mut self, now: f64, market_only: bool) -> Option<&[u8]>;
    fn snapshot(&mut self, now: f64) -> &[u8];
    fn account_snapshot(&mut self, now: f64) -> &[u8];
    fn health(&self) -> Health {
        Health::default()
    }
    /// The engine store, projected for algos between engine calls.
    #[cfg(feature = "algos")]
    fn store(&self) -> Option<&safe_cex_engine::Store> {
        None
    }
}

impl<V: Venue> AnyEngine for Engine<V> {
    #[cfg(feature = "address-nonce")]
    fn has_started(&self) -> bool {
        Engine::has_started(self)
    }
    #[cfg(feature = "address-nonce")]
    fn address_setup_support(
        &mut self,
        supported: bool,
    ) -> Result<(), safe_cex_engine::VenueError> {
        self.set_address_setup_host_support(if supported {
            safe_cex_engine::SetupHostSupport::OwnedPublicAddress1
        } else {
            safe_cex_engine::SetupHostSupport::Unavailable
        })
    }
    #[cfg(feature = "address-nonce")]
    fn initialize_address_setup(
        &mut self,
        now: f64,
        reinitialize: bool,
    ) -> Result<(), safe_cex_engine::VenueError> {
        if reinitialize {
            self.reinitialize_address_nonce(now).map(|_| ())
        } else {
            self.initialize_address_nonce(now).map(|_| ())
        }
    }
    #[cfg(feature = "address-nonce")]
    fn address_setup_ready(&self) -> bool {
        self.address_nonce_setup_ready()
    }
    #[cfg(feature = "address-nonce")]
    fn address_setup_retired(&mut self, token: &str, now: f64) -> bool {
        self.address_setup_retired_wire(token, now)
    }

    #[cfg(any(feature = "hosted-signing", feature = "hosted-wallet"))]
    fn signer(&self) -> Option<safe_cex_engine::signing::Provider> {
        Engine::signer(self)
    }
    #[cfg(any(feature = "hosted-signing", feature = "hosted-wallet"))]
    fn signing_clock(&self, now: f64) -> (f64, u64) {
        Engine::signing_clock(self, now)
    }
    fn failed(&self) -> bool {
        Engine::failed(self)
    }
    fn external_signer(
        &mut self,
        algorithms: Option<Vec<safe_cex_engine::signing::Algorithm>>,
    ) -> Result<(), safe_cex_engine::VenueError> {
        self.set_signer(None)?;
        self.set_host_algorithms(algorithms)
    }
    fn prepare_dispatch(&mut self, now: f64) {
        Engine::prepare_dispatch(self, now);
    }
    fn set_entropy(
        &mut self,
        fill: safe_cex_engine::entropy::Fill,
        signing: bool,
    ) -> Result<(), safe_cex_engine::VenueError> {
        if signing {
            Engine::set_signing_entropy(self, fill)
        } else {
            Engine::set_entropy(self, fill)
        }
    }
    fn signing_pending(&self, attempt: RequestId) -> bool {
        Engine::signing_pending(self, attempt)
    }
    #[cfg(feature = "sign-meta")]
    fn sign_meta(&self, task: RequestId) -> Option<safe_cex_engine::sign_meta::SignMeta> {
        Engine::sign_meta(self, task)
    }
    fn http_preparation_pending(
        &self,
        token: safe_cex_engine::request_context::Preparation,
    ) -> bool {
        Engine::http_preparation_pending(self, token)
    }
    fn http_prepared(
        &mut self,
        now: f64,
        token: safe_cex_engine::request_context::Preparation,
        resource: safe_cex_engine::request_context::Prepared,
    ) {
        Engine::http_prepared(self, now, token, resource);
    }
    fn http_preparation_failed(
        &mut self,
        now: f64,
        token: safe_cex_engine::request_context::Preparation,
        reason: safe_cex_engine::request_context::Error,
        error: safe_cex_engine::VenueError,
    ) {
        Engine::http_preparation_failed(self, now, token, reason, error);
    }
    #[cfg(feature = "http-mutation-barriers")]
    fn http_group_pending(&self, permit: &safe_cex_engine::request_context::GroupPermit) -> bool {
        Engine::http_group_pending(self, permit)
    }
    #[cfg(feature = "http-mutation-barriers")]
    fn claim_http_group(
        &mut self,
        now: f64,
        permit: safe_cex_engine::request_context::GroupPermit,
    ) -> Result<safe_cex_engine::request_context::GroupClaim, safe_cex_engine::request_context::Error>
    {
        Engine::claim_http_group(self, now, permit)
    }
    fn claim_http(
        &mut self,
        now: f64,
        token: safe_cex_engine::request_context::Permit,
    ) -> Result<safe_cex_engine::request_context::Claim, safe_cex_engine::request_context::Error>
    {
        Engine::claim_http(self, now, token)
    }
    fn capabilities(&self) -> safe_cex_engine::Capabilities {
        Engine::capabilities(self)
    }
    fn apply_public_feed(
        &mut self,
        now: f64,
        update: safe_cex_engine::PublicFeedUpdate,
    ) -> Result<(), &'static str> {
        Engine::apply_public_feed(self, now, update)
    }
    fn start(&mut self, now: f64) {
        Engine::start(self, now);
    }
    fn apply_frame(&mut self, now: f64, frame: &[u8], snapshot: bool) -> Result<(), String> {
        Engine::apply_frame(self, now, frame, snapshot).map_err(|error| error.0.to_owned())
    }
    fn set_source_clock_offset(&mut self, offset_ms: Option<f64>) {
        Engine::set_source_clock_offset(self, offset_ms);
    }
    fn handle(&mut self, now: f64, event: Event<'_>) {
        Engine::handle(self, now, event);
    }
    fn handle_command(
        &mut self,
        now: f64,
        command: safe_cex_engine::Command,
        wait_ms: Option<u32>,
    ) -> Result<(), safe_cex_engine::request_context::Error> {
        Engine::handle_command(self, now, command, wait_ms)
    }
    fn drain(&mut self) -> Vec<Action> {
        self.drain_actions().collect()
    }
    fn frame(&mut self, now: f64, market_only: bool) -> Option<&[u8]> {
        if market_only {
            self.take_market_frame(now)
        } else {
            self.take_frame(now)
        }
    }
    fn snapshot(&mut self, now: f64) -> &[u8] {
        self.snapshot_frame(now)
    }
    fn account_snapshot(&mut self, now: f64) -> &[u8] {
        self.account_snapshot_frame(now)
    }
    fn health(&self) -> Health {
        Engine::health(self).clone()
    }

    #[cfg(feature = "algos")]
    fn store(&self) -> Option<&safe_cex_engine::Store> {
        Some(Engine::store(self))
    }
}

// Opaque marker: JS hosts fulfil the Action::Sign/Event::Signed seam. This
// contains no keys and cannot fall back to local signing if injection is missing.
#[cfg(feature = "dispatch-signing")]
struct ExternalSigner;
#[cfg(feature = "dispatch-signing")]
impl safe_cex_engine::signing::Signer for ExternalSigner {
    fn sign(
        &self,
        _: safe_cex_engine::signing::SignRequest,
    ) -> safe_cex_engine::signing::SignFuture {
        Box::pin(std::future::ready(Err(
            safe_cex_engine::signing::SignError::Unavailable,
        )))
    }
}

/// A JS provider's declared algorithms: a JSON array of SignAlgorithm names.
fn host_algorithms(
    json: &str,
) -> Result<Vec<safe_cex_engine::signing::Algorithm>, safe_cex_engine::VenueError> {
    serde_json::from_str(json).map_err(|_| {
        safe_cex_engine::VenueError::new("Invalid signer algorithm declaration")
            .kind(safe_cex_engine::ErrorKind::InvalidInput)
            .outcome(safe_cex_engine::Outcome::NotSent)
    })
}

/// Owns whichever venue engine the registry builds.
struct Boxed;

impl safe_cex_venues::Visit for Boxed {
    type Output = Box<dyn AnyEngine>;
    fn visit<V: Venue + 'static>(self, engine: Engine<V>) -> Box<dyn AnyEngine> {
        Box::new(engine)
    }
}

#[derive(Serialize)]
#[serde(tag = "op", rename_all = "camelCase")]
enum ActionOut<'a> {
    #[cfg(feature = "address-nonce")]
    RetireAddressSetup {
        task: u64,
        token: String,
    },
    AccountAuth {
        credential: safe_cex_engine::CredentialSlot,
        scope: safe_cex_engine::AccountReadScope,
        outcome: safe_cex_engine::AuthOutcome,
    },
    Deactivate,
    StartupFailed {
        detail: &'a safe_cex_engine::VenueError,
    },
    WsConnect {
        #[serde(skip_serializing_if = "is_false")]
        binary: bool,
        socket: u64,
        url: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        channel: Option<&'static str>,
        #[serde(skip_serializing_if = "Option::is_none")]
        context: Option<&'a safe_cex_engine::SocketContext>,
        #[serde(skip_serializing_if = "Option::is_none")]
        headers: Option<Vec<(&'static str, &'a str)>>,
    },
    WsSend {
        socket: u64,
        text: &'a str,
    },
    WsClose {
        socket: u64,
    },
    Http {
        #[serde(flatten)]
        packet: HttpPacket<'a>,
    },
    ManagedHttp {
        #[serde(flatten)]
        phase: HttpPhaseOut<'a>,
    },
    Sign {
        attempt: u64,
        work: &'a safe_cex_engine::signing::SignRequest,
        #[serde(rename = "timeoutMs")]
        timeout_ms: u32,
    },
    SetTimer {
        timer: u64,
        at: f64,
    },
    ClearTimer {
        timer: u64,
    },
    Event {
        level: &'static str,
        code: &'static str,
        message: &'a str,
        #[serde(skip_serializing_if = "Fields::is_empty")]
        fields: Fields<'a>,
    },
    CommandError {
        command: u32,
        message: &'a str,
    },
    Done {
        command: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        result: Option<&'a str>,
        /// The message, or the partial-batch JSON `host.mjs` unpacks.
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<std::borrow::Cow<'a, str>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        detail: Option<&'a safe_cex_engine::VenueError>,
    },
    CommandProgress {
        command: u32,
        receipts: &'a [safe_cex_engine::OrderIdentityReceipt],
    },
}

struct Fields<'a>(&'a [(&'static str, Value)]);

impl Fields<'_> {
    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl serde::Serialize for Fields<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(Some(self.0.len()))?;
        for (key, value) in self.0 {
            match value {
                Value::Str(text) => map.serialize_entry(key, text)?,
                // JSON has no non-finite numbers.
                Value::Num(number) if number.is_finite() => map.serialize_entry(key, number)?,
                Value::Num(_) => map.serialize_entry(key, &())?,
                Value::Bool(flag) => map.serialize_entry(key, flag)?,
                Value::Redacted => map.serialize_entry(key, safe_cex_engine::log::REDACTED)?,
            }
        }
        map.end()
    }
}

fn socket_channel(channel: &safe_cex_engine::SocketChannel) -> &'static str {
    match channel {
        safe_cex_engine::SocketChannel::Public => "public",
        safe_cex_engine::SocketChannel::Private => "private",
        safe_cex_engine::SocketChannel::Business => "business",
    }
}

fn encode(actions: &[Action]) -> String {
    if actions.is_empty() {
        return String::new();
    }
    let out: Vec<ActionOut<'_>> = actions
        .iter()
        .map(|action| match action {
            #[cfg(feature = "address-nonce")]
            Action::RetireAddressSetup { task, token } => ActionOut::RetireAddressSetup {
                task: task.0,
                token: token.to_wire(),
            },
            #[cfg(not(feature = "address-nonce"))]
            Action::RetireAddressSetup { .. } => std::process::abort(),
            Action::WsConnect {
                binary,
                socket,
                url,
                headers,
                channel,
                context,
            } => ActionOut::WsConnect {
                binary: *binary,
                socket: socket.0,
                url,
                channel: channel.as_ref().map(socket_channel),
                context: context.as_ref(),
                // Header-less sockets keep the field absent so browser hosts can open them.
                headers: (!headers.is_empty())
                    .then(|| headers.iter().map(|(k, v)| (*k, v.as_str())).collect()),
            },
            Action::WsSend { socket, text } => ActionOut::WsSend {
                socket: socket.0,
                text,
            },
            Action::WsClose { socket } => ActionOut::WsClose { socket: socket.0 },
            Action::Http(request) => ActionOut::Http {
                packet: HttpPacket::from(request),
            },
            Action::ManagedHttp(action) => ActionOut::ManagedHttp {
                phase: HttpPhaseOut::from(action.as_ref()),
            },
            Action::SignedOutbound { .. } => unreachable!("engine drain unwraps signed outbound"),
            Action::Sign {
                attempt,
                work,
                timeout_ms,
            } => ActionOut::Sign {
                attempt: attempt.0,
                work,
                timeout_ms: *timeout_ms,
            },
            Action::SetTimer { timer, at_ms } => ActionOut::SetTimer {
                timer: timer.0,
                at: *at_ms,
            },
            Action::ClearTimer { timer } => ActionOut::ClearTimer { timer: timer.0 },
            Action::Notice(Notice::AccountAuth {
                credential,
                scope,
                outcome,
            }) => ActionOut::AccountAuth {
                credential: *credential,
                scope: *scope,
                outcome: *outcome,
            },
            Action::Notice(Notice::Deactivate) => ActionOut::Deactivate,
            Action::Notice(Notice::CommandError { command, message }) => ActionOut::CommandError {
                command: *command,
                message,
            },
            Action::Notice(Notice::StartupFailed { error }) => {
                ActionOut::StartupFailed { detail: error }
            }
            Action::Notice(Notice::Event(event)) => ActionOut::Event {
                level: event.level.as_str(),
                code: event.code,
                message: &event.message,
                fields: Fields(&event.fields),
            },
            Action::Notice(Notice::CommandDone { command, result }) => ActionOut::Done {
                command: *command,
                result: Some(result.as_str()),
                error: None,
                detail: None,
            },
            Action::Notice(Notice::CommandFailed { command, error }) => ActionOut::Done {
                command: *command,
                result: None,
                error: Some(error.legacy_text().into()),
                detail: Some(error),
            },
            Action::Notice(Notice::CommandProgress { command, receipts }) => {
                ActionOut::CommandProgress {
                    command: *command,
                    receipts,
                }
            }
        })
        .collect();
    serde_json::to_string(&out).unwrap_or_default()
}

/// Bytes of WebAssembly linear memory currently reserved by this module.
#[wasm_bindgen(js_name = wasmMemoryBytes)]
pub fn wasm_memory_bytes() -> f64 {
    #[cfg(target_arch = "wasm32")]
    {
        (core::arch::wasm32::memory_size(0) * 65_536) as f64
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        0.0
    }
}

/// Resolve construction choices without constructing an engine or performing I/O.
#[wasm_bindgen(js_name = resolveConstruction)]
pub fn resolve_construction(venue: &str, options_json: &str) -> Result<String, JsError> {
    if !safe_cex_venues::supported(venue) {
        return Err(JsError::new(&format!("unsupported venue: {venue}")));
    }
    let options: safe_cex_venues::EngineOptions =
        serde_json::from_str(options_json).map_err(|_| JsError::new("Invalid engine options"))?;
    Ok(serde_json::to_string(&options.construction(venue))?)
}

fn entropy_configuration_error() -> safe_cex_engine::VenueError {
    safe_cex_engine::VenueError::new("Cryptographic host entropy is unavailable")
        .kind(safe_cex_engine::ErrorKind::InvalidInput)
}

#[cfg(target_arch = "wasm32")]
fn crypto_entropy() -> Result<safe_cex_engine::entropy::Fill, safe_cex_engine::VenueError> {
    #[wasm_bindgen]
    extern "C" {
        #[wasm_bindgen(thread_local_v2, js_name = globalThis)]
        static GLOBAL_THIS: Option<js_sys::Object>;
        #[wasm_bindgen(thread_local_v2, js_name = self)]
        static SELF: Option<js_sys::Object>;
        #[wasm_bindgen(thread_local_v2, js_name = window)]
        static WINDOW: Option<js_sys::Object>;
    }
    // js_sys::global() prefers self/window, which may be incomplete Node/Bun
    // shims. Prefer the real realm's crypto and only accept callable providers.
    let (crypto, fill) = std::iter::once_with(|| GLOBAL_THIS.with(Option::clone))
        .chain(std::iter::once_with(|| SELF.with(Option::clone)))
        .chain(std::iter::once_with(|| WINDOW.with(Option::clone)))
        .flatten()
        .find_map(|global| {
            let crypto = js_sys::Reflect::get(&global, &"crypto".into()).ok()?;
            let fill = js_sys::Reflect::get(&crypto, &"getRandomValues".into())
                .ok()?
                .dyn_into::<js_sys::Function>()
                .ok()?;
            Some((crypto, fill))
        })
        .ok_or_else(entropy_configuration_error)?;
    Ok(Box::new(move |bytes| {
        let size = u32::try_from(bytes.len()).map_err(|_| "Entropy request is too large")?;
        // Separate storage: arbitrary JS callbacks must never see a Rust memory view.
        let output = js_sys::Uint8Array::new_with_length(size);
        fill.call1(&crypto, output.as_ref())
            .map_err(|_| "Host entropy unavailable")?;
        output.copy_to(bytes);
        Ok(())
    }))
}

#[cfg(not(target_arch = "wasm32"))]
fn crypto_entropy() -> Result<safe_cex_engine::entropy::Fill, safe_cex_engine::VenueError> {
    Ok(Box::new(|bytes| {
        getrandom::getrandom(bytes).map_err(|error| error.to_string())
    }))
}

fn production_entropy() -> Result<safe_cex_engine::entropy::Entropy, safe_cex_engine::VenueError> {
    Ok(safe_cex_engine::entropy::Entropy::new(
        crypto_entropy()?,
        crypto_entropy()?,
    ))
}

#[cfg(target_arch = "wasm32")]
fn callback_entropy(
    source: js_sys::Function,
) -> Result<safe_cex_engine::entropy::Fill, safe_cex_engine::VenueError> {
    if !source.is_function() {
        return Err(entropy_configuration_error());
    }
    Ok(Box::new(move |bytes| {
        let size = u32::try_from(bytes.len()).map_err(|_| "Entropy request is too large")?;
        let output = source
            .call1(source.as_ref(), &f64::from(size).into())
            .map_err(|_| "Host entropy unavailable")?
            .dyn_into::<js_sys::Uint8Array>()
            .map_err(|_| "Entropy provider must return Uint8Array")?;
        if output.length() != size {
            return Err("Entropy provider returned the wrong byte length".into());
        }
        output.copy_to(bytes);
        Ok(())
    }))
}

#[cfg(not(target_arch = "wasm32"))]
fn callback_entropy(
    _source: js_sys::Function,
) -> Result<safe_cex_engine::entropy::Fill, safe_cex_engine::VenueError> {
    Err(entropy_configuration_error())
}

fn host_resource_id(value: f64) -> Result<u64, js_sys::Error> {
    safe_cex_engine::resource_ids::from_host_number(value).ok_or_else(|| {
        construction_error(
            safe_cex_engine::VenueError::new(
                "Engine resource ID must be a nonnegative safe integer",
            )
            .kind(safe_cex_engine::ErrorKind::InvalidInput)
            .outcome(safe_cex_engine::Outcome::NotSent),
        )
    })
}

fn construction_error(detail: safe_cex_engine::VenueError) -> js_sys::Error {
    let error = js_sys::Error::new(&detail.message);
    if let Ok(json) = serde_json::to_string(&detail) {
        let _ = js_sys::Reflect::set(error.as_ref(), &"venueError".into(), &json.into());
    }
    error
}

#[wasm_bindgen]
pub struct WasmEngine {
    dispatch_signing: bool,
    external_signer: bool,
    construction: String,
    inner: Box<dyn AnyEngine>,
    #[cfg(feature = "hosted-signing")]
    hosted_identity: Option<(String, safe_cex_engine::signing::Identities)>,
    #[cfg(feature = "hosted-signing")]
    hosted_captures:
        safe_cex_engine::signing_capture::Capture<safe_cex_venues::hosted_signing::OutputKind>,
    #[cfg(feature = "hosted-wallet")]
    hosted_wallet: Option<(
        String,
        safe_cex_engine::signing::Identities,
        safe_cex_venues::hosted_wallet::Account,
    )>,
    #[cfg(feature = "algos")]
    algos: algos::AlgoHost,
}

#[cfg(feature = "hosted-signing")]
fn hosted_error(error: safe_cex_engine::signing::SignError) -> js_sys::Error {
    construction_error(error.venue_error())
}

#[cfg(feature = "hosted-signing")]
#[wasm_bindgen]
impl WasmEngine {
    /// Clock capture only: does not start the venue or emit any action.
    #[wasm_bindgen(js_name = hostedSigningClock)]
    pub fn hosted_signing_clock(&self, now: f64) -> Result<String, js_sys::Error> {
        use safe_cex_engine::signing::{SignError, SignReceipt};
        if self.hosted_identity.is_none() || !now.is_finite() || now < 0. {
            return Err(hosted_error(SignError::Unavailable));
        }
        let (protocol_now, revision) = self.inner.signing_clock(now);
        let stamp_ms = safe_cex_engine::time::wire_stamp_ms(protocol_now);
        SignReceipt {
            nonce: None,
            stamp_ms: Some(stamp_ms),
            expiry_ms: None,
        }
        .validate()
        .map_err(hosted_error)?;
        // Fields in json!'s sorted-key order keep the output byte-identical.
        #[derive(serde::Serialize)]
        struct Clock {
            revision: String,
            #[serde(rename = "stampMs")]
            stamp_ms: f64,
        }
        serde_json::to_string(&Clock {
            revision: revision.to_string(),
            stamp_ms,
        })
        .map_err(|_| hosted_error(SignError::InvalidRequest))
    }

    #[wasm_bindgen(js_name = prepareHostedSigning)]
    pub fn prepare_hosted_signing(
        &mut self,
        input: &str,
        now: f64,
    ) -> Result<String, js_sys::Error> {
        use safe_cex_engine::signing::SignError;
        if input.len() > 1024 * 1024 {
            return Err(hosted_error(SignError::InvalidRequest));
        }
        let (venue, identities) = self
            .hosted_identity
            .as_ref()
            .ok_or_else(|| hosted_error(SignError::Unsupported))?;
        let input: safe_cex_venues::hosted_signing::Input =
            serde_json::from_str(input).map_err(|_| hosted_error(SignError::InvalidRequest))?;
        let (protocol_now, revision) = self.inner.signing_clock(now);
        let mut timeout = safe_cex_engine::signing_capture::MAX_TIMEOUT_MS;
        if let safe_cex_venues::hosted_signing::Input::Http {
            stamp_ms,
            revision: captured,
            ..
        } = &input
        {
            let age = protocol_now - *stamp_ms;
            if captured != &revision.to_string() || !age.is_finite() || !(0. ..5000.).contains(&age)
            {
                return Err(hosted_error(SignError::InvalidRequest));
            }
            timeout = (5000. - age).ceil().clamp(1., 5000.) as u32;
        }
        let (work, kind) = safe_cex_venues::hosted_signing::prepare(venue, identities, input)
            .map_err(hosted_error)?;
        let prepared = self
            .hosted_captures
            .prepare_with_policy(
                (work, kind.header_policy()),
                kind,
                now,
                protocol_now,
                revision,
                timeout,
            )
            .map_err(hosted_error)?;
        // Fields in json!'s sorted-key order; `work` stays a sorted-key Value
        // so the bytes handed to the signer are unchanged.
        #[derive(serde::Serialize)]
        struct Prepared {
            #[serde(rename = "deadlineMs")]
            deadline_ms: f64,
            id: u32,
            work: serde_json::Value,
        }
        let work = serde_json::to_value(&prepared.work)
            .map_err(|_| hosted_error(SignError::InvalidRequest))?;
        serde_json::to_string(&Prepared {
            deadline_ms: prepared.deadline_ms,
            id: prepared.id.get(),
            work,
        })
        .map_err(|_| hosted_error(SignError::InvalidRequest))
    }

    /// Executes only this account's already prepared work, never caller refs.
    #[wasm_bindgen(js_name = localHostedSigning)]
    pub fn local_hosted_signing(&mut self, id: u32, now: f64) -> Result<String, js_sys::Error> {
        use safe_cex_engine::signing::SignError;
        let id = safe_cex_engine::signing_capture::CaptureId::from_raw(id)
            .ok_or_else(|| hosted_error(SignError::Unavailable))?;
        let provider = self.inner.signer();
        let (protocol_now, revision) = self.inner.signing_clock(now);
        let (kind, result) = self
            .hosted_captures
            .execute(id, now, protocol_now, revision, |work| {
                provider
                    .ok_or(SignError::Unavailable)?
                    .sign_ready(work)
                    .unwrap_or(Err(SignError::Unavailable))
            })
            .map_err(hosted_error)?;
        let output =
            safe_cex_venues::hosted_signing::finish(kind, &result).map_err(hosted_error)?;
        serde_json::to_string(&output).map_err(|_| hosted_error(SignError::InvalidResult))
    }

    #[wasm_bindgen(js_name = finishHostedSigning)]
    pub fn finish_hosted_signing(
        &mut self,
        id: u32,
        result: &str,
        now: f64,
    ) -> Result<String, js_sys::Error> {
        use safe_cex_engine::signing::{SignError, SignResult};
        let id = safe_cex_engine::signing_capture::CaptureId::from_raw(id)
            .ok_or_else(|| hosted_error(SignError::Unavailable))?;
        let result = if result.len() > 5 * 1024 * 1024 {
            Err(SignError::InvalidResult)
        } else {
            serde_json::from_str::<SignResult>(result).map_err(|_| SignError::InvalidResult)
        };
        let (protocol_now, revision) = self.inner.signing_clock(now);
        let (kind, result) = self
            .hosted_captures
            .finish(id, now, protocol_now, revision, result)
            .map_err(hosted_error)?;
        let output =
            safe_cex_venues::hosted_signing::finish(kind, &result).map_err(hosted_error)?;
        serde_json::to_string(&output).map_err(|_| hosted_error(SignError::InvalidResult))
    }

    #[wasm_bindgen(js_name = cancelHostedSigning)]
    pub fn cancel_hosted_signing(&mut self, id: u32) {
        if let Some(id) = safe_cex_engine::signing_capture::CaptureId::from_raw(id) {
            self.hosted_captures.cancel(id);
        }
    }
}

#[wasm_bindgen]
impl WasmEngine {
    #[cfg(any(
        feature = "coinbase",
        feature = "paradex",
        feature = "binance",
        feature = "apexomni",
        feature = "asterdex",
        feature = "breakoutprop",
        feature = "kucoin",
        feature = "kucoinspot",
        feature = "polymarket",
        feature = "blofin",
        feature = "bybit",
        feature = "woo",
        feature = "cryptocom",
        feature = "bingx",
        feature = "mexc",
        feature = "bitget",
        feature = "grvt",
        feature = "bitrue",
        feature = "bitunix",
        feature = "backpack",
        feature = "phemex",
        feature = "weex",
        feature = "hyperliquid",
        feature = "kraken",
        feature = "txflow",
        feature = "orderly",
        feature = "lighter",
        feature = "okx"
    ))]
    #[wasm_bindgen(constructor)]
    pub fn new(
        venue: &str,
        options_json: &str,
        signer_setup_json: Option<String>,
    ) -> Result<WasmEngine, js_sys::Error> {
        // Own the setup buffer before any parsing or feature/venue refusal.
        let signer_setup_json = signer_setup_json.map(zeroize::Zeroizing::new);
        let options: safe_cex_venues::EngineOptions = serde_json::from_str(options_json)
            .map_err(|_| js_sys::Error::new("Invalid engine options"))?;
        #[cfg(feature = "dispatch-signing")]
        let mut options = options;
        let dispatch_signing = options.construction(venue).bridge.dispatch_signing;
        // A false read-key result outranks any provider setup failure, as natively.
        safe_cex_venues::read_key_preflight(venue, &options).map_err(construction_error)?;
        let external_signer = options.signing.is_some() && signer_setup_json.is_none();
        #[cfg(feature = "dispatch-signing")]
        let signer = if let Some(mut input) = signer_setup_json {
            if !dispatch_signing {
                return Err(js_sys::Error::new(
                    "Local signer setup is not supported for this venue yet",
                ));
            }
            let (signer, identities) = safe_cex_venues::local_signer_from_capsule(
                venue,
                std::mem::take(&mut *input),
                &mut options,
            )
            .map_err(construction_error)?;
            options.signing = Some(identities);
            Some(signer)
        } else if external_signer {
            Some(std::sync::Arc::new(ExternalSigner) as safe_cex_engine::signing::Provider)
        } else if dispatch_signing {
            let (signer, identities) =
                safe_cex_venues::prepare_local_signer_for(venue, &mut options)
                    .map_err(construction_error)?;
            options.signing = Some(identities);
            Some(signer)
        } else {
            None
        };
        #[cfg(not(feature = "dispatch-signing"))]
        let signer = {
            if signer_setup_json.is_some() {
                return Err(js_sys::Error::new(
                    "Local signer setup is not supported for this venue yet",
                ));
            }
            None
        };
        #[cfg(feature = "hosted-signing")]
        let hosted_identity = (dispatch_signing
            && safe_cex_venues::hosted_signing::admitted(venue, &options))
        .then(|| {
            options
                .signing
                .clone()
                .map(|identities| (venue.to_owned(), identities))
        })
        .flatten();
        #[cfg(feature = "hosted-wallet")]
        let hosted_wallet = (dispatch_signing && safe_cex_venues::hosted_wallet::admitted(venue))
            .then(|| {
                options.signing.clone().map(|identities| {
                    let account = safe_cex_venues::hosted_wallet::Account {
                        key: options.key.clone(),
                        vault: options.vault.clone(),
                        testnet: options.testnet,
                        application_id: options.application_id.clone(),
                        // EngineOptions carries api_key_index only in Lighter builds.
                        #[cfg(feature = "lighter")]
                        api_key_index: options.api_key_index.clone(),
                        #[cfg(not(feature = "lighter"))]
                        api_key_index: None,
                        paradex_address: options.paradex_address.clone(),
                    };
                    (venue.to_owned(), identities, account)
                })
            })
            .flatten();
        let construction = serde_json::to_string(&options.construction(venue))
            .map_err(|error| js_sys::Error::new(&error.to_string()))?;
        let inner = safe_cex_venues::build_with_signer(
            venue,
            options,
            Some(production_entropy().map_err(construction_error)?),
            signer,
            Boxed,
        )
        .map_err(construction_error)?;
        Ok(Self {
            inner,
            construction,
            dispatch_signing,
            external_signer,
            #[cfg(feature = "hosted-signing")]
            hosted_identity,
            #[cfg(feature = "hosted-signing")]
            hosted_captures: Default::default(),
            #[cfg(feature = "hosted-wallet")]
            hosted_wallet,
            #[cfg(feature = "algos")]
            algos: algos::AlgoHost::new(venue),
        })
    }

    /// Device-side wallet answer for a keyless server's ws-auth request; the
    /// local provider signs synchronously. Returns the `0x` hex result.
    #[cfg(feature = "hosted-wallet")]
    #[wasm_bindgen(js_name = hostedWalletSigning)]
    pub fn hosted_wallet_signing(
        &self,
        to_sign: &str,
        output_format: &str,
        now: f64,
    ) -> Result<String, js_sys::Error> {
        use safe_cex_engine::signing::SignError;
        let error = |error: SignError| construction_error(error.venue_error());
        let (venue, identities, account) = self
            .hosted_wallet
            .as_ref()
            .ok_or_else(|| error(SignError::Unsupported))?;
        let provider = self
            .inner
            .signer()
            .ok_or_else(|| error(SignError::Unavailable))?;
        let (protocol_now, _) = self.inner.signing_clock(now);
        safe_cex_venues::hosted_wallet::sign(
            venue,
            identities,
            account,
            &provider,
            to_sign,
            output_format,
            protocol_now,
        )
        .map_err(error)
    }

    #[cfg(feature = "address-nonce")]
    #[wasm_bindgen(js_name = setAddressSetupSupport)]
    pub fn set_address_setup_support(&mut self, protocol: &str) -> Result<(), js_sys::Error> {
        if protocol != "publicAddressOwned" {
            return Err(construction_error(
                safe_cex_engine::signing::SignError::Unsupported.venue_error(),
            ));
        }
        self.inner
            .address_setup_support(true)
            .map_err(construction_error)
    }
    #[cfg(feature = "address-nonce")]
    #[wasm_bindgen(js_name = initializeAddressSetup)]
    pub fn initialize_address_setup(
        &mut self,
        now: f64,
        reinitialize: bool,
    ) -> Result<String, js_sys::Error> {
        self.inner
            .initialize_address_setup(now, reinitialize)
            .map_err(construction_error)?;
        Ok(self.flush())
    }
    #[cfg(feature = "address-nonce")]
    #[wasm_bindgen(js_name = addressSetupReady)]
    pub fn address_setup_ready(&self) -> bool {
        self.inner.address_setup_ready()
    }
    #[cfg(feature = "address-nonce")]
    #[wasm_bindgen(js_name = addressSetupRetired)]
    pub fn address_setup_retired(&mut self, token: &str, now: f64) -> String {
        self.inner.address_setup_retired(token, now);
        self.flush()
    }
    #[wasm_bindgen(js_name = setEntropy)]
    pub fn set_entropy(&mut self, source: js_sys::Function) -> Result<(), js_sys::Error> {
        self.inner
            .set_entropy(callback_entropy(source).map_err(construction_error)?, false)
            .map_err(construction_error)
    }
    #[wasm_bindgen(js_name = setSigningEntropy)]
    pub fn set_signing_entropy(&mut self, source: js_sys::Function) -> Result<(), js_sys::Error> {
        self.inner
            .set_entropy(callback_entropy(source).map_err(construction_error)?, true)
            .map_err(construction_error)
    }

    /// JS providers execute atomic batches outside the engine. Must be installed before start.
    #[wasm_bindgen(js_name = useExternalSigner)]
    /// `algorithms_json` declares what the JS provider can sign (a JSON array of
    /// SignAlgorithm); omitted means unknown, so capability admission fails closed.
    pub fn use_external_signer(
        &mut self,
        algorithms_json: Option<String>,
    ) -> Result<(), js_sys::Error> {
        if !self.dispatch_signing {
            return Err(construction_error(
                safe_cex_engine::VenueError::new(
                    "Injected signing is not supported for this venue yet",
                )
                .kind(safe_cex_engine::ErrorKind::Unsupported)
                .outcome(safe_cex_engine::Outcome::NotSent),
            ));
        }
        let algorithms = algorithms_json
            .map(|json| host_algorithms(&json))
            .transpose()
            .map_err(construction_error)?;
        self.inner
            .external_signer(algorithms)
            .map_err(construction_error)?;
        self.external_signer = true;
        Ok(())
    }
    #[wasm_bindgen(js_name=requiresExternalSigner)]
    pub fn requires_external_signer(&self) -> bool {
        self.external_signer
    }

    #[wasm_bindgen(js_name = signed)]
    pub fn signed(
        &mut self,
        attempt: f64,
        result_json: &str,
        now: f64,
    ) -> Result<String, js_sys::Error> {
        let attempt = host_resource_id(attempt)?;
        let result = serde_json::from_str::<safe_cex_engine::signing::SignResult>(result_json)
            .map_err(|_| safe_cex_engine::signing::SignError::InvalidResult);
        self.inner.handle(
            now,
            Event::Signed {
                attempt: RequestId(attempt),
                result,
            },
        );
        Ok(self.flush())
    }
    #[wasm_bindgen(js_name = signFailed)]
    pub fn sign_failed(
        &mut self,
        attempt: f64,
        code: &str,
        now: f64,
    ) -> Result<String, js_sys::Error> {
        let attempt = host_resource_id(attempt)?;
        use safe_cex_engine::signing::SignError;
        let error = match code {
            "timedOut" => SignError::TimedOut,
            "unavailable" => SignError::Unavailable,
            "invalidResult" => SignError::InvalidResult,
            _ => SignError::Refused,
        };
        self.inner.handle(
            now,
            Event::Signed {
                attempt: RequestId(attempt),
                result: Err(error),
            },
        );
        Ok(self.flush())
    }
    #[wasm_bindgen(js_name = prepareDispatch)]
    pub fn prepare_dispatch(&mut self, now: f64) -> String {
        self.inner.prepare_dispatch(now);
        self.flush()
    }

    /// Sticky fatal engine state; reading it grants no I/O authority.
    pub fn failed(&self) -> bool {
        self.inner.failed()
    }

    /// Resolved account choices; contains no credentials.
    pub fn construction(&self) -> String {
        self.construction.clone()
    }

    /// Whether `venue` can be built with `frameFed: true`; check before
    /// construction so an unconverted family keeps its existing path.
    #[wasm_bindgen(js_name = frameFedSupported)]
    pub fn frame_fed_supported(venue: &str) -> bool {
        safe_cex_venues::frame_fed_supported(venue)
    }

    /// Apply a data source's SCXF frame to a frame-fed engine; `snapshot`
    /// replaces the account sections. Returns the flushed actions, like `command`.
    /// The data source's clock minus this page's, in ms: 0 for the LOCAL
    /// worker, the measured server offset for COSIGNED/DELEGATED. Fences lift
    /// on one clock once set; until then they wait 30 s.
    #[wasm_bindgen(js_name = setSourceClockOffset)]
    pub fn set_source_clock_offset(&mut self, offset_ms: Option<f64>) {
        self.inner.set_source_clock_offset(offset_ms);
    }

    #[wasm_bindgen(js_name = applyFrame)]
    pub fn apply_frame(
        &mut self,
        frame: &[u8],
        snapshot: bool,
        now: f64,
    ) -> Result<String, JsError> {
        self.inner
            .apply_frame(now, frame, snapshot)
            .map_err(|error| JsError::new(&error))?;
        Ok(self.flush())
    }

    fn flush(&mut self) -> String {
        #[cfg(feature = "algos")]
        return encode(&self.algos.route(self.inner.drain()));
        #[cfg(not(feature = "algos"))]
        encode(&self.inner.drain())
    }

    /// Health counters as JSON (`safe_cex_engine::Health`, camelCase).
    pub fn health(&self) -> String {
        let h = self.inner.health();
        format!(
            r#"{{"wsOpened":{},"wsClosed":{},"reconnects":{},"authFailures":{},"httpFailures":{},"rateLimited":{},"snapshotsDiscarded":{},"rowsSkipped":{},"lastPrivateMessage":{}}}"#,
            h.ws_opened,
            h.ws_closed,
            h.reconnects,
            h.auth_failures,
            h.http_failures,
            h.rate_limited,
            h.snapshots_discarded,
            h.rows_skipped,
            h.last_private_message_ms
                .filter(|at| at.is_finite())
                .map_or_else(|| "null".to_owned(), |at| at.to_string())
        )
    }

    pub fn start(&mut self, now: f64) -> String {
        #[cfg(feature = "address-nonce")]
        if !self.inner.has_started()
            && let Err(error) = self.inner.initialize_address_setup(now, false)
        {
            return encode(&[Action::Notice(Notice::StartupFailed { error })]);
        }
        self.inner.start(now);
        self.flush()
    }

    #[wasm_bindgen(js_name = wsOpen)]
    pub fn ws_open(&mut self, socket: f64, now: f64) -> Result<String, js_sys::Error> {
        let socket = host_resource_id(socket)?;
        let socket = SocketId(socket);
        self.inner.handle(now, Event::WsOpened { socket });
        Ok(self.flush())
    }

    #[wasm_bindgen(js_name = wsMessage)]
    pub fn ws_message(
        &mut self,
        socket: f64,
        text: &str,
        now: f64,
    ) -> Result<String, js_sys::Error> {
        let socket = host_resource_id(socket)?;
        let socket = SocketId(socket);
        self.inner.handle(now, Event::WsMessage { socket, text });
        Ok(self.flush())
    }

    /// Like `wsMessage`, returning `undefined` when there are no host actions.
    /// Hosts can avoid decoding an empty UTF-8 string on market-data messages.
    #[cfg(feature = "text-ingress")]
    #[wasm_bindgen(js_name = wsMessageActions)]
    pub fn ws_message_actions(
        &mut self,
        socket: f64,
        text: &str,
        now: f64,
    ) -> Result<Option<String>, js_sys::Error> {
        host_resource_id(socket)?;
        let actions = self.ws_message(socket, text, now)?;
        Ok((!actions.is_empty()).then_some(actions))
    }

    /// A byte-input alternative that rejects invalid UTF-8 before handling any event.
    #[cfg(feature = "text-ingress")]
    #[wasm_bindgen(js_name = wsMessageBytes)]
    pub fn ws_message_bytes(
        &mut self,
        socket: f64,
        bytes: &[u8],
        now: f64,
    ) -> Result<Option<String>, js_sys::Error> {
        host_resource_id(socket)?;
        let Ok(text) = std::str::from_utf8(bytes) else {
            return Ok(None);
        };
        self.ws_message_actions(socket, text, now)
    }

    #[wasm_bindgen(js_name = wsBinary)]
    pub fn ws_binary(
        &mut self,
        socket: f64,
        data: &[u8],
        now: f64,
    ) -> Result<String, js_sys::Error> {
        let socket = host_resource_id(socket)?;
        self.inner.handle(
            now,
            Event::WsBinary {
                socket: SocketId(socket),
                data,
            },
        );
        Ok(self.flush())
    }

    #[wasm_bindgen(js_name = wsClose)]
    pub fn ws_close(&mut self, socket: f64, code: u16, now: f64) -> Result<String, js_sys::Error> {
        let socket = host_resource_id(socket)?;
        let socket = SocketId(socket);
        self.inner.handle(now, Event::WsClosed { socket, code });
        Ok(self.flush())
    }

    /// Host-only: a `rejectRoute` connect had no route left, so nothing was dialled.
    #[wasm_bindgen(js_name = wsRouteExhausted)]
    pub fn ws_route_exhausted(&mut self, socket: f64, now: f64) -> Result<String, js_sys::Error> {
        let socket = SocketId(host_resource_id(socket)?);
        self.inner.handle(now, Event::WsRouteExhausted { socket });
        Ok(self.flush())
    }

    /// A response begins; `headers` is `name: value` lines. The body follows
    /// as chunks so a large list never has to fit in memory at once.
    #[wasm_bindgen(js_name = httpStart)]
    pub fn http_start(
        &mut self,
        request: f64,
        status: u16,
        headers: &str,
        now: f64,
    ) -> Result<String, js_sys::Error> {
        let request = host_resource_id(request)?;
        let request = RequestId(request);
        self.inner.handle(
            now,
            Event::HttpStart {
                request,
                status,
                headers,
            },
        );
        Ok(self.flush())
    }

    #[wasm_bindgen(js_name = httpChunk)]
    pub fn http_chunk(
        &mut self,
        request: f64,
        data: &[u8],
        now: f64,
    ) -> Result<String, js_sys::Error> {
        let request = host_resource_id(request)?;
        let request = RequestId(request);
        self.inner.handle(now, Event::HttpChunk { request, data });
        Ok(self.flush())
    }

    #[wasm_bindgen(js_name = httpEnd)]
    pub fn http_end(&mut self, request: f64, now: f64) -> Result<String, js_sys::Error> {
        let request = host_resource_id(request)?;
        let request = RequestId(request);
        self.inner.handle(now, Event::HttpEnd { request });
        Ok(self.flush())
    }

    #[wasm_bindgen(js_name = httpFailed)]
    pub fn http_failed(
        &mut self,
        request: f64,
        error: &str,
        now: f64,
    ) -> Result<String, js_sys::Error> {
        let request = host_resource_id(request)?;
        let request = RequestId(request);
        self.inner.handle(now, Event::HttpFailed { request, error });
        Ok(self.flush())
    }

    pub fn timer(&mut self, timer: f64, now: f64) -> Result<String, js_sys::Error> {
        let timer = host_resource_id(timer)?;
        let timer = TimerId(timer);
        self.inner.handle(now, Event::TimerFired { timer });
        Ok(self.flush())
    }

    pub fn command(&mut self, command_json: &str, now: f64) -> Result<String, JsError> {
        #[cfg(feature = "cold-subscriptions")]
        {
            if let Some(command) = safe_cex_engine::command::decode_subscription(command_json)? {
                self.inner.handle(now, Event::Command(command));
                return Ok(self.flush());
            }
            self.other_command(command_json, now)
        }
        #[cfg(not(feature = "cold-subscriptions"))]
        {
            let event = match safe_cex_engine::command::decode_host_input(command_json)? {
                safe_cex_engine::command::HostInput::PublicFeedUpdate(update) => {
                    self.inner
                        .apply_public_feed(now, update)
                        .map_err(JsError::new)?;
                    return Ok(self.flush());
                }
                safe_cex_engine::command::HostInput::Command {
                    command,
                    timeout_ms,
                } => {
                    self.inner
                        .handle_command(now, command, timeout_ms)
                        .map_err(|error| JsError::new(&format!("{error:?}")))?;
                    return Ok(self.flush());
                }
                safe_cex_engine::command::HostInput::PollScaleChanged { scale, ceiling_ms } => {
                    self.inner
                        .handle(now, Event::PollCeilingChanged { ceiling_ms });
                    Event::PollScaleChanged { scale }
                }
            };
            self.inner.handle(now, event);
            Ok(self.flush())
        }
    }

    // Keep cold decoding and dispatch together: returning the large command enum
    // across this boundary would copy every variant into the startup path.
    #[cfg(feature = "cold-subscriptions")]
    #[inline(never)]
    fn other_command(&mut self, command_json: &str, now: f64) -> Result<String, JsError> {
        let event = match safe_cex_engine::command::decode_host_input(command_json)? {
            safe_cex_engine::command::HostInput::PublicFeedUpdate(update) => {
                self.inner
                    .apply_public_feed(now, update)
                    .map_err(JsError::new)?;
                return Ok(self.flush());
            }
            safe_cex_engine::command::HostInput::Command {
                command,
                timeout_ms,
            } => {
                self.inner
                    .handle_command(now, command, timeout_ms)
                    .map_err(|error| JsError::new(&format!("{error:?}")))?;
                return Ok(self.flush());
            }
            safe_cex_engine::command::HostInput::PollScaleChanged { scale, ceiling_ms } => {
                self.inner
                    .handle(now, Event::PollCeilingChanged { ceiling_ms });
                Event::PollScaleChanged { scale }
            }
        };
        self.inner.handle(now, event);
        Ok(self.flush())
    }

    /// The venue's capability record as JSON; fixed for the engine's lifetime.
    /// Snapshot frames carry the same record for consumers without the engine.
    pub fn capabilities(&self) -> Result<String, JsError> {
        Ok(serde_json::to_string(&self.inner.capabilities())?)
    }

    /// Everything that changed since the previous call as one flush frame
    /// (layout: docs/plans/RUST_REWRITE.md), or `undefined` when nothing did.
    /// The whole current state as one frame, for a consumer that starts late
    /// or skipped frames. Frames taken afterwards apply on top of it.
    #[wasm_bindgen(js_name = snapshotFrame)]
    pub fn snapshot_frame(&mut self, now: f64) -> js_sys::Uint8Array {
        js_sys::Uint8Array::from(self.inner.snapshot(now))
    }

    /// Full private sections; leaves pending deltas and public baselines intact.
    #[wasm_bindgen(js_name = accountSnapshotFrame)]
    pub fn account_snapshot_frame(&mut self, now: f64) -> js_sys::Uint8Array {
        js_sys::Uint8Array::from(self.inner.account_snapshot(now))
    }

    /// Transfer `frame.buffer` to the consumer: it is copied once, straight
    /// out of the engine's reused frame buffer.
    /// With `market_only`, just books, candles and fills (for the venue's
    /// book cadence); store sections wait for the next full frame.
    #[wasm_bindgen(js_name = takeFrame)]
    pub fn take_frame(&mut self, now: f64, market_only: bool) -> Option<js_sys::Uint8Array> {
        #[cfg(feature = "algos")]
        if self.algos.is_running() {
            return self.take_frame_with(now, market_only, |frame| js_sys::Uint8Array::from(frame));
        }
        self.inner
            .frame(now, market_only)
            .map(js_sys::Uint8Array::from)
    }
}

#[cfg(feature = "algos")]
mod algos;

#[cfg(feature = "algos")]
impl WasmEngine {
    /// A taken frame also reaches the algos: its fills and the store it left.
    fn take_frame_with<T>(
        &mut self,
        now: f64,
        market_only: bool,
        copy: impl FnOnce(&[u8]) -> T,
    ) -> Option<T> {
        let frame = self.inner.frame(now, market_only)?;
        let fills = safe_cex_engine::wire::fills(frame).unwrap_or_default();
        let out = copy(frame);
        self.algos.frame(self.inner.store(), fills);
        Some(out)
    }
}

#[cfg(all(test, feature = "algos"))]
impl WasmEngine {
    /// An engine over `inner`, for algo tests that drive a test venue.
    fn for_algo_tests(venue: &str, inner: Box<dyn AnyEngine>) -> Self {
        Self {
            dispatch_signing: false,
            external_signer: false,
            construction: String::new(),
            inner,
            #[cfg(feature = "hosted-signing")]
            hosted_identity: None,
            #[cfg(feature = "hosted-signing")]
            hosted_captures: Default::default(),
            #[cfg(feature = "hosted-wallet")]
            hosted_wallet: None,
            algos: algos::AlgoHost::new(venue),
        }
    }
}

fn is_false(value: &bool) -> bool {
    !*value
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "asterdex")]
    #[test]
    fn hosted_signing_identity_exists_only_for_aster_v1_accounts() {
        let engine = |options: &str, setup: &str| {
            let Ok(engine) = crate::WasmEngine::new("asterdex", options, Some(setup.into())) else {
                panic!("trusted Aster setup must construct");
            };
            engine
        };
        let v1 = engine(
            r#"{"asterdex":{"apiVersion":"v1"}}"#,
            r#"{"primary":{"key":"api-key","secret":"api-secret"}}"#,
        );
        assert!(v1.hosted_identity.is_some());
        let v3 = engine(
            r#"{"applicationId":"0x2222222222222222222222222222222222222222","asterdex":{"apiVersion":"v3","signerAddress":"0x1111111111111111111111111111111111111111"}}"#,
            &format!(
                r#"{{"primary":{{"key":"0x1111111111111111111111111111111111111111","secret":"{}","secretEncoding":"hex"}}}}"#,
                "12".repeat(32)
            ),
        );
        // A V3 wallet key must never be offered to the hosted HMAC capture,
        // including when no apiVersion accompanies setup-held V3 material.
        assert!(v3.hosted_identity.is_none());
        let unversioned = engine(
            r#"{"applicationId":"0x2222222222222222222222222222222222222222","asterdex":{"signerAddress":"0x1111111111111111111111111111111111111111"}}"#,
            &format!(
                r#"{{"primary":{{"key":"0x1111111111111111111111111111111111111111","secret":"{}","secretEncoding":"hex"}}}}"#,
                "12".repeat(32)
            ),
        );
        assert!(unversioned.hosted_identity.is_none());
    }

    /// The host build must run address setup at start: a V3 wallet's signing
    /// address is verified there, and every V3 signature depends on it.
    #[cfg(feature = "asterdex")]
    #[test]
    fn a_v3_wallet_account_signs_after_wasm_start() {
        use serde_json::Value;
        const NOW: f64 = 1_700_000_000_000.;
        let options = r#"{"applicationId":"0x63DD5aCC6b1aa0f563956C0e534DD30B6dcF7C4e","asterdex":{"apiVersion":"v3","signerAddress":"0x21cF8Ae13Bb72632562c6Fff438652Ba1a151bb0"}}"#;
        let setup = r#"{"primary":{"key":"0x21cF8Ae13Bb72632562c6Fff438652Ba1a151bb0","secret":"4fd0a42218f3eae43a6ce26d22544e986139a01e5b34a62db53757ffca81bae1","secretEncoding":"hex"}}"#;
        let Ok(mut engine) = crate::WasmEngine::new("asterdex", options, Some(setup.into())) else {
            panic!("trusted V3 setup must construct");
        };
        let decode = |text: String| -> Vec<Value> {
            if text.is_empty() {
                Vec::new()
            } else {
                serde_json::from_str(&text).unwrap()
            }
        };
        let mut queue: std::collections::VecDeque<Value> = decode(engine.start(NOW)).into();
        queue.extend(decode(
            engine
                .command(r#"{"op":"fetchStpMode","command":1}"#, NOW)
                .unwrap(),
        ));
        let mut released = Vec::new();
        let mut resource = 0;
        while let Some(action) = queue.pop_front() {
            if action["op"] != "managedHttp" {
                continue;
            }
            match action["phase"].as_str() {
                Some("prepare") => {
                    resource += 1;
                    let next = engine
                        .http_prepared(
                            action["preparation"].as_str().unwrap(),
                            &resource.to_string(),
                            None,
                            NOW,
                        )
                        .unwrap();
                    queue.extend(decode(next));
                }
                Some("bind") => {
                    let claim: Value = serde_json::from_str(
                        &engine.http_claim(action["permit"].as_str().unwrap(), NOW),
                    )
                    .unwrap();
                    if claim["status"] == "released" {
                        let packet = &action["packet"];
                        let url = packet["url"].as_str().unwrap().to_owned();
                        if url.ends_with("/time") {
                            let id = packet["request"].as_f64().unwrap();
                            queue.extend(decode(engine.http_start(id, 200, "", NOW).unwrap()));
                            let body = format!(r#"{{"serverTime":{NOW}}}"#);
                            queue.extend(decode(
                                engine.http_chunk(id, body.as_bytes(), NOW).unwrap(),
                            ));
                            queue.extend(decode(engine.http_end(id, NOW).unwrap()));
                        }
                        released.push(url);
                    }
                    queue.extend(decode(engine.prepare_dispatch(NOW)));
                }
                _ => {}
            }
        }
        assert!(
            released.iter().any(|url| url.contains("/stpMode")
                && url.contains("&signer=0x21cF8Ae13Bb72632562c6Fff438652Ba1a151bb0&")
                && url.contains("&nonce=")
                && url.contains("&signature=0x")),
            "a V3 signed read must release after start: {released:?}"
        );
    }

    #[cfg(feature = "bybit")]
    mod bybit_signer_construction {
        use crate::WasmEngine;
        use serde_json::{Value, json};

        const ALIASES: [&str; 5] = [
            "bybit",
            "bybitinverse",
            "bybitspot",
            "bybitdemo",
            "hyrotrader",
        ];
        const NOW: f64 = 1_700_000_000_000.;

        fn options(venue: &str) -> Value {
            if venue == "hyrotrader" {
                json!({"endpoints": {
                    "rest": "https://hyro.invalid",
                    "publicWs": "wss://hyro.invalid/public",
                    "privateWs": "wss://hyro.invalid/private"
                }})
            } else {
                json!({})
            }
        }

        fn header<'a>(action: &'a Value, name: &str) -> Option<&'a str> {
            action["headers"].as_array()?.iter().find_map(|entry| {
                (entry[0].as_str() == Some(name))
                    .then(|| entry[1].as_str())
                    .flatten()
            })
        }

        /// Drive managed reads through the host's prepare, bind and claim
        /// handoff; a released packet joins the output as its `http` action.
        fn managed(engine: &mut WasmEngine, actions: Vec<Value>) -> Vec<Value> {
            let mut queue: std::collections::VecDeque<Value> = actions.into();
            let mut out = Vec::new();
            let mut resource = 0;
            while let Some(action) = queue.pop_front() {
                if action["op"] != "managedHttp" {
                    out.push(action);
                    continue;
                }
                let next = match action["phase"].as_str() {
                    Some("prepare") => {
                        resource += 1;
                        engine
                            .http_prepared(
                                action["preparation"].as_str().unwrap(),
                                &resource.to_string(),
                                None,
                                NOW,
                            )
                            .unwrap()
                    }
                    Some("bind") => {
                        let claim: Value = serde_json::from_str(
                            &engine.http_claim(action["permit"].as_str().unwrap(), NOW),
                        )
                        .unwrap();
                        if claim["status"] == "released" {
                            let mut packet = action["packet"].clone();
                            packet["op"] = "http".into();
                            out.push(packet);
                        }
                        engine.prepare_dispatch(NOW)
                    }
                    _ => continue,
                };
                if !next.is_empty() {
                    queue.extend(serde_json::from_str::<Vec<Value>>(&next).unwrap());
                }
            }
            out
        }

        #[test]
        fn all_aliases_construct_with_setup_and_ordinary_startup_gets_are_signed() {
            for venue in ALIASES {
                let setup = json!({
                    "primary": {"key": "host-primary", "secret": "host-primary-secret"},
                    "secondary": {"key": "host-secondary", "secret": "host-secondary-secret"}
                });
                let Ok(mut engine) =
                    WasmEngine::new(venue, &options(venue).to_string(), Some(setup.to_string()))
                else {
                    panic!("{venue}: trusted provider construction must succeed");
                };
                assert!(engine.dispatch_signing, "{venue}");
                assert!(!engine.requires_external_signer(), "{venue}");
                let construction: Value = serde_json::from_str(&engine.construction()).unwrap();
                assert_eq!(construction["bridge"]["dispatchSigning"], true, "{venue}");
                let started = serde_json::from_str(&engine.start(NOW)).unwrap();
                let actions = managed(&mut engine, started);
                assert!(
                    !actions.iter().any(|action| action["op"] == "sign"),
                    "{venue}"
                );
                let signed: Vec<_> = actions
                    .iter()
                    .filter(|action| header(action, "X-BAPI-SIGN").is_some())
                    .collect();
                if venue == "hyrotrader" {
                    // Hyro defers private startup until its catalogue is usable.
                    assert!(signed.is_empty());
                    assert!(!actions.is_empty());
                    continue;
                }
                assert!(
                    !signed.is_empty(),
                    "{venue}: startup must release signed GETs"
                );
                for action in signed {
                    assert_eq!(action["op"], "http", "{venue}");
                    assert_eq!(action["method"], "GET", "{venue}");
                    assert_eq!(
                        header(action, "X-BAPI-API-KEY"),
                        Some("host-secondary"),
                        "{venue}"
                    );
                    assert_eq!(
                        header(action, "X-BAPI-TIMESTAMP"),
                        Some("1700000000000"),
                        "{venue}"
                    );
                    assert_eq!(header(action, "X-BAPI-SIGN").unwrap().len(), 64, "{venue}");
                }
            }
        }

        #[test]
        fn all_aliases_admit_opaque_external_signing_and_release_only_after_completion() {
            for venue in ALIASES {
                let mut input = options(venue);
                input["signing"] = json!({
                    "primary": {"credential": "primary.credential", "signing": "primary.signing"},
                    "secondary": {"credential": "secondary.credential", "signing": "secondary.signing"}
                });
                let Ok(mut engine) = WasmEngine::new(venue, &input.to_string(), None) else {
                    panic!("{venue}: opaque external provider construction must succeed");
                };
                assert!(engine.requires_external_signer(), "{venue}");
                // Declared and undeclared capability both admit ordinary signing.
                let declared = (venue != ALIASES[0]).then(|| r#"["hmacSha256"]"#.to_owned());
                assert!(engine.use_external_signer(declared).is_ok(), "{venue}");
                let started = serde_json::from_str(&engine.start(NOW)).unwrap();
                let actions = managed(&mut engine, started);
                assert!(
                    !actions
                        .iter()
                        .any(|action| header(action, "X-BAPI-SIGN").is_some()),
                    "{venue}: signing has not completed"
                );
                if venue == "hyrotrader" {
                    assert!(!actions.iter().any(|action| action["op"] == "sign"));
                    continue;
                }
                let sign = actions
                    .iter()
                    .find(|action| action["op"] == "sign")
                    .unwrap();
                let values: Vec<Vec<u8>> = sign["work"]["operations"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|operation| match operation["kind"].as_str().unwrap() {
                        "present" => {
                            assert_eq!(operation["key"], "secondary.credential", "{venue}");
                            b"external-secondary".to_vec()
                        }
                        "sign" => {
                            assert_eq!(operation["key"], "secondary.signing", "{venue}");
                            vec![0xab; 32]
                        }
                        kind => panic!("{venue}: unexpected signing operation {kind}"),
                    })
                    .collect();
                let completion = json!({"values": values, "receipt": sign["work"]["receipt"]});
                let completed = serde_json::from_str(
                    &engine
                        .signed(
                            sign["attempt"].as_f64().unwrap(),
                            &completion.to_string(),
                            NOW + 1.,
                        )
                        .unwrap(),
                )
                .unwrap();
                let released = managed(&mut engine, completed);
                assert!(released.iter().any(|action| {
                    action["op"] == "http"
                        && header(action, "X-BAPI-API-KEY") == Some("external-secondary")
                        && header(action, "X-BAPI-SIGN") == Some("abababababababababababababababababababababababababababababababab")
                }), "{venue}: completed provider values must reach the actual HTTP action");
            }
        }
    }

    #[cfg(all(feature = "apexomni", not(target_arch = "wasm32")))]
    #[test]
    fn apexomni_family_bootstraps_raw_and_explicit_local_signer_setup() {
        let corpus: serde_json::Value = serde_json::from_str(include_str!(
            "../../../crypto/tests/fixtures/zklink/npm-wasm.json"
        ))
        .unwrap();
        let seed = corpus["cases"][0]["seed"].as_str().unwrap();
        let raw = serde_json::json!({
            "key": "api-key", "secret": "bXlzZWNyZXQ=", "passphrase": "pass",
            "omniKeySeed": seed,
            "publicMode": {"requested": false},
        });
        let setup = serde_json::json!({"primary": {
            "key": "api-key", "secret": "bXlzZWNyZXQ=", "passphrase": "pass",
            "extra": {"zklinkSeed": {"value": seed, "encoding": "hex"}},
        }});
        for (options, setup) in [
            (raw.to_string(), None),
            (
                r#"{"publicMode":{"requested":false}}"#.into(),
                Some(setup.to_string()),
            ),
        ] {
            let Ok(mut engine) = super::WasmEngine::new("apexomni", &options, setup) else {
                panic!("ApeX local setup must construct the installed signer");
            };
            assert!(engine.dispatch_signing);
            assert!(engine.inner.signer().is_some());
            assert!(!engine.requires_external_signer());
            let construction: serde_json::Value =
                serde_json::from_str(&engine.construction()).unwrap();
            assert_eq!(construction["canSign"], true);
            assert_eq!(construction["publicDataOnly"], false);
            let actions: Vec<serde_json::Value> =
                serde_json::from_str(&engine.start(1000.)).unwrap();
            assert!(actions.iter().any(|action| action["op"] == "managedHttp"));
        }
    }

    #[cfg(all(feature = "mexc", not(target_arch = "wasm32")))]
    #[test]
    fn mexc_family_bootstraps_raw_and_explicit_local_signer_setup() {
        for (options, setup) in [
            (r#"{"key":"replay-key","secret":"fixture-secret"}"#, None),
            (
                "{}",
                Some(r#"{"primary":{"key":"replay-key","secret":"fixture-secret"}}"#.into()),
            ),
        ] {
            let Ok(mut engine) = super::WasmEngine::new("mexc", options, setup) else {
                panic!("MEXC local setup must construct the installed signer");
            };
            assert!(engine.dispatch_signing);
            assert!(!engine.requires_external_signer());
            let actions: Vec<serde_json::Value> =
                serde_json::from_str(&engine.start(1000.)).unwrap();
            assert!(actions.iter().any(|action| action["op"] == "managedHttp"));
        }
    }

    #[cfg(all(feature = "dispatch-signing", not(target_arch = "wasm32")))]
    mod shared_dispatch_provider {
        use super::super::ExternalSigner;
        use safe_cex_engine::signing::{
            Context, KeyRef, Operation, SignError, SignRequest, Signer,
        };

        #[test]
        fn host_algorithm_declarations_are_exact_sign_algorithm_names() {
            use safe_cex_engine::signing::Algorithm;
            assert_eq!(
                super::super::host_algorithms(r#"["hmacSha256","zkLinkContract"]"#).unwrap(),
                [Algorithm::HmacSha256, Algorithm::ZkLinkContract]
            );
            for invalid in [r#"["HMAC"]"#, r#""hmacSha256""#, "", r#"[null]"#] {
                assert!(super::super::host_algorithms(invalid).is_err(), "{invalid}");
            }
        }

        #[test]
        fn external_marker_never_materializes_credentials_or_signatures() {
            let signer = ExternalSigner;
            let request = SignRequest {
                receipt: Default::default(),
                context: Context::default(),
                operations: vec![Operation::Present {
                    key: KeyRef("opaque-test-credential".into()),
                }],
            };
            assert!(!signer.immediate());
            assert!(signer.sign_ready(&request).is_none());
            // The host fulfils Action::Sign. Accidentally polling the marker
            // must retain its existing closed failure, never expose material.
            let mut future = signer.sign(request);
            let mut context = std::task::Context::from_waker(std::task::Waker::noop());
            assert!(matches!(
                std::future::Future::poll(future.as_mut(), &mut context),
                std::task::Poll::Ready(Err(SignError::Unavailable))
            ));
        }

        // Provider-enabled MEXC discovery still has no signing authority.
        #[cfg(feature = "mexc")]
        #[test]
        fn shared_provider_feature_keeps_managed_public_mexc_unsigned() {
            let Ok(mut engine) =
                super::super::WasmEngine::new("mexc", r#"{"publicDataOnly":true}"#, None)
            else {
                panic!("public MEXC setup must remain native-safe and successful");
            };
            assert!(engine.dispatch_signing);
            assert!(!engine.requires_external_signer());
            #[derive(serde::Deserialize)]
            struct Packet {
                request: u64,
                url: String,
                method: String,
                headers: Vec<(String, String)>,
            }
            #[derive(serde::Deserialize)]
            struct Outbound {
                op: String,
                phase: Option<String>,
                intent: Option<String>,
                preparation: Option<String>,
                permit: Option<String>,
                packet: Option<Packet>,
            }
            let actions: Vec<Outbound> = serde_json::from_str(&engine.start(1000.)).unwrap();
            let preparation = actions
                .iter()
                .find(|action| {
                    action.op == "managedHttp"
                        && action.phase.as_deref() == Some("prepare")
                        && action
                            .packet
                            .as_ref()
                            .is_some_and(|packet| packet.url.contains("/api/v1/contract/"))
                })
                .expect("managed public discovery must prepare before release");
            assert_eq!(preparation.intent.as_deref(), Some("public"));
            assert!(actions.iter().all(|action| action.op != "http"));
            let token = preparation.preparation.as_deref().unwrap();
            assert!(engine.http_preparation_pending(token));
            let bound: Vec<Outbound> =
                serde_json::from_str(&engine.http_prepared(token, "1", None, 1000.).unwrap())
                    .unwrap();
            let binding = bound
                .iter()
                .find(|action| {
                    action.op == "managedHttp" && action.phase.as_deref() == Some("bind")
                })
                .expect("public transport must bind before claim");
            assert_eq!(binding.preparation.as_deref(), Some(token));
            let unsigned = preparation.packet.as_ref().unwrap();
            let released = binding.packet.as_ref().unwrap();
            assert_eq!(released.request, unsigned.request);
            assert_eq!(released.url, unsigned.url);
            assert_eq!(released.method, "GET");
            assert_eq!(released.headers, unsigned.headers);
            assert_eq!(
                engine.http_claim(binding.permit.as_deref().unwrap(), 1000.),
                r#"{"status":"released"}"#
            );
            assert!(bound.iter().all(|action| {
                !matches!(action.op.as_str(), "http" | "sign" | "signedOutbound")
            }));
            assert!(
                actions
                    .iter()
                    .all(|action| { !matches!(action.op.as_str(), "sign" | "signedOutbound") })
            );
            assert!(!engine.requires_external_signer());
        }
    }

    /// Each venue's record equals the name lists `rust.exchange.ts` used before
    /// the engine reported it.
    #[cfg(all(
        feature = "apexomni",
        feature = "asterdex",
        feature = "backpack",
        feature = "binance",
        feature = "bingx",
        feature = "bitget",
        feature = "bitrue",
        feature = "bitunix",
        feature = "blofin",
        feature = "breakoutprop",
        feature = "bybit",
        feature = "coinbase",
        feature = "cryptocom",
        feature = "grvt",
        feature = "hyperliquid",
        feature = "kraken",
        feature = "kucoin",
        feature = "kucoinspot",
        feature = "lighter",
        feature = "mexc",
        feature = "okx",
        feature = "orderly",
        feature = "paradex",
        feature = "phemex",
        feature = "polymarket",
        feature = "txflow",
        feature = "weex",
        feature = "woo"
    ))]
    #[test]
    fn capabilities_match_the_former_bridge_name_lists() {
        use safe_cex_engine::{
            AccountLookup, CancelAll, Capabilities, LeverageQuery, MaxLeverage, QuantityUnits,
        };
        const ORDERLY: [&str; 6] = [
            "tealstreet-dex",
            "kodiakfi",
            "mode",
            "salsadex",
            "what",
            "woofi",
        ];
        const OKX: [&str; 4] = ["okx", "okxeu", "okxdemo", "okxeudemo"];
        let venues = [
            "coinbase",
            "coinbasespot",
            "apexomni",
            "asterdex",
            "breakoutprop",
            "binance",
            "binancespot",
            "binanceinverse",
            "backpack",
            "binancepm",
            "bitget",
            "bitgetusdc",
            "bitgetinverse",
            "blofin",
            "bitrue",
            "bitunix",
            "grvt",
            "mexc",
            "woo",
            "bingx",
            "bybit",
            "bybitinverse",
            "bybitspot",
            "bybitdemo",
            "hyrotrader",
            "cryptocom",
            "kucoin",
            "kucoinspot",
            "polymarket",
            "hyperliquid",
            "hyperliquidspot",
            "paradex",
            "txflow",
            "kraken",
            "krakenspot",
            "lighter",
            "lighter-robinhood",
            "weex",
            "phemex",
        ]
        .into_iter()
        .chain(ORDERLY)
        .chain(OKX);
        let any = |names: &[&str], venue: &str| names.contains(&venue);
        let engine_options = |venue: &str, extra: &str| {
            let endpoints = if venue == "txflow" {
                r#","testnet":true"#
            } else if venue == "breakoutprop" {
                r#","key":"user","secret":"password""#
            } else if venue == "hyrotrader" {
                r#","endpoints":{"rest":"https://hyro.test","publicWs":"wss://hyro.test/public","privateWs":"wss://hyro.test/private"}"#
            } else {
                ""
            };
            format!(r#"{{"publicDataOnly":true{endpoints}{extra}}}"#)
        };
        for venue in venues {
            let no_history = any(
                &[
                    "breakoutprop",
                    "hyperliquidspot",
                    "krakenspot",
                    "binancespot",
                    "binanceinverse",
                    "polymarket",
                ],
                venue,
            );
            let no_orders_or_income =
                no_history || any(&["apexomni", "bitrue", "kucoinspot"], venue);
            let expected = Capabilities {
                account: if any(
                    &[
                        "apexomni",
                        "asterdex",
                        "backpack",
                        "binance",
                        "binancepm",
                        "bingx",
                        "bitget",
                        "bitgetinverse",
                        "bitgetusdc",
                        "bybit",
                        "bybitdemo",
                        "bybitinverse",
                        "bybitspot",
                        "cryptocom",
                        "grvt",
                        "kucoin",
                        "kucoinspot",
                        "lighter",
                        "lighter-robinhood",
                        "mexc",
                        "paradex",
                        "phemex",
                        "polymarket",
                        "weex",
                        "woo",
                    ],
                    venue,
                ) {
                    AccountLookup::Query
                } else if any(&["coinbase", "coinbasespot"], venue) || any(&ORDERLY, venue) {
                    AccountLookup::AfterLoad
                } else {
                    AccountLookup::Unsupported
                },
                client_order_lookup: any(
                    &[
                        "hyperliquid",
                        "grvt",
                        "paradex",
                        "bitrue",
                        "kucoinspot",
                        "phemex",
                        "binance",
                        "binancepm",
                    ],
                    venue,
                ) || any(&OKX, venue),
                fill_history: !no_history,
                order_history: !no_orders_or_income,
                income_history: !no_orders_or_income
                    && !any(
                        &["bybitspot", "hyrotrader", "coinbase", "coinbasespot"],
                        venue,
                    ),
                fills_by_order_id: venue == "grvt",
                cancel_all: if any(
                    &[
                        "polymarket",
                        "breakoutprop",
                        "woo",
                        "grvt",
                        "kucoin",
                        "kucoinspot",
                    ],
                    venue,
                ) || any(&ORDERLY, venue)
                {
                    CancelAll::Native
                } else if any(
                    &[
                        "phemex",
                        "asterdex",
                        "bitget",
                        "bitgetinverse",
                        "bitgetusdc",
                        "blofin",
                        "bitrue",
                    ],
                    venue,
                ) || any(&OKX, venue)
                {
                    CancelAll::EachPlacedSymbol
                } else {
                    CancelAll::EachOpenSymbol
                },
                leverage_query: if any(
                    &[
                        "bingx",
                        "weex",
                        "bybit",
                        "bybitinverse",
                        "bybitdemo",
                        "bitget",
                        "bitgetinverse",
                        "bitgetusdc",
                        "kucoin",
                    ],
                    venue,
                ) {
                    LeverageQuery::OnReadAndMetadata
                } else if any(
                    &[
                        "asterdex",
                        "blofin",
                        "bitrue",
                        "grvt",
                        "paradex",
                        "txflow",
                        "coinbase",
                        "coinbasespot",
                        "polymarket",
                    ],
                    venue,
                ) || any(&OKX, venue)
                    || any(&ORDERLY, venue)
                {
                    LeverageQuery::OnRead
                } else if any(&["mexc", "woo", "kraken"], venue) {
                    LeverageQuery::OnMetadata
                } else {
                    LeverageQuery::Settings
                },
                max_leverage: match venue {
                    "bitrue" => MaxLeverage::Query,
                    "polymarket" => MaxLeverage::Rails,
                    "breakoutprop" => MaxLeverage::Unsupported,
                    _ => MaxLeverage::Market,
                },
                account_leverage: any(&ORDERLY, venue),
                account_margin_type: any(&["blofin", "kucoinspot"], venue),
                cancel_with_symbols: any(&["asterdex", "kucoinspot"], venue),
                amend_with_symbol: venue == "kucoinspot",
                order_book_query: any(
                    &[
                        "asterdex",
                        "polymarket",
                        "mexc",
                        "backpack",
                        "cryptocom",
                        "grvt",
                        "paradex",
                        "weex",
                    ],
                    venue,
                ),
                options: false,
                market_refresh: any(
                    &[
                        "bybit",
                        "bybitdemo",
                        "bybitinverse",
                        "bybitspot",
                        "hyrotrader",
                    ],
                    venue,
                ),
                quantity_units: match venue {
                    "mexc" | "blofin" | "kucoin" | "coinbase" | "coinbasespot" => {
                        QuantityUnits::NativePlacement
                    }
                    "bitgetinverse" => QuantityUnits::InverseNative,
                    "okx" | "okxeu" | "okxdemo" | "okxeudemo" => QuantityUnits::ContractCount,
                    "binanceinverse" => QuantityUnits::InverseContractSized,
                    _ => QuantityUnits::Base,
                },
                account_tier: any(&["lighter", "lighter-robinhood"], venue),
                closed_positions: any(
                    &[
                        "bybit",
                        "bybitdemo",
                        "hyrotrader",
                        "bybitinverse",
                        "blofin",
                        "bitunix",
                    ],
                    venue,
                ) || any(&OKX, venue),
                balance_info: any(
                    &[
                        "bybit",
                        "bybitdemo",
                        "hyrotrader",
                        "bybitinverse",
                        "bybitspot",
                        "binanceinverse",
                    ],
                    venue,
                ),
                schedule_cancel: venue == "hyperliquid",
                position_history: match venue {
                    "kraken" | "bitget" | "bitgetusdc" | "bitgetinverse" => {
                        safe_cex_engine::PositionHistory::FillsAccount
                    }
                    "binance" | "hyperliquid" | "paradex" => {
                        safe_cex_engine::PositionHistory::FillsSymbol
                    }
                    "bybit" | "bybitdemo" | "hyrotrader" => safe_cex_engine::PositionHistory::Bybit,
                    "okx" | "okxeu" | "okxdemo" | "okxeudemo" => {
                        safe_cex_engine::PositionHistory::Okx
                    }
                    "blofin" => safe_cex_engine::PositionHistory::Blofin,
                    "woo" => safe_cex_engine::PositionHistory::WooOrders,
                    "bitunix" => safe_cex_engine::PositionHistory::Bitunix,
                    _ => safe_cex_engine::PositionHistory::Unsupported,
                },
                closed_position_format: match venue {
                    "bybit" | "bybitdemo" | "hyrotrader" | "bybitinverse" => {
                        safe_cex_engine::ClosedPositionFormat::Bybit
                    }
                    "okx" | "okxeu" | "okxdemo" | "okxeudemo" => {
                        safe_cex_engine::ClosedPositionFormat::Okx
                    }
                    "blofin" => safe_cex_engine::ClosedPositionFormat::Blofin,
                    "bitunix" => safe_cex_engine::ClosedPositionFormat::Bitunix,
                    _ => safe_cex_engine::ClosedPositionFormat::Unsupported,
                },
                fill_history_time: match venue {
                    "woo" => safe_cex_engine::FillHistoryTime::NonnegativeSecondsOrMilliseconds,
                    "apexomni" | "blofin" | "bitrue" | "grvt" | "hyperliquid" | "paradex"
                    | "lighter" | "lighter-robinhood" | "kraken" | "coinbase" | "coinbasespot" => {
                        safe_cex_engine::FillHistoryTime::SecondsOrMilliseconds
                    }
                    _ => safe_cex_engine::FillHistoryTime::AsProvided,
                },
                stp_controls: false,
                bybit_execution_controls: any(
                    &[
                        "bybit",
                        "bybitinverse",
                        "bybitspot",
                        "bybitdemo",
                        "hyrotrader",
                    ],
                    venue,
                ),
                reject_client_slippage: venue == "hyrotrader",
                finite_command_numbers: any(&["coinbase", "coinbasespot"], venue),
                finite_history_bounds: any(&["coinbase", "coinbasespot"], venue),
                reject_order_history_filters: any(&["coinbase", "coinbasespot", "txflow"], venue)
                    || any(&ORDERLY, venue),
                public_catalogue_validation: venue == "breakoutprop",
                isolated_book_subscriptions: venue == "asterdex",
                inverse_closed_positions: venue == "bybitinverse",
            };
            let Ok(engine) = super::WasmEngine::new(venue, &engine_options(venue, ""), None) else {
                panic!("{venue} engine constructs");
            };
            assert_eq!(engine.inner.capabilities(), expected, "{venue}");
        }
        for (venue, options) in [
            ("bybit", true),
            ("bybitinverse", false),
            ("hyrotrader", false),
        ] {
            let Ok(engine) = super::WasmEngine::new(
                venue,
                &engine_options(venue, r#","enableOptionMarkets":true"#),
                None,
            ) else {
                panic!("{venue} engine constructs");
            };
            assert_eq!(engine.inner.capabilities().options, options, "{venue}");
        }
    }

    #[test]
    fn account_auth_serializes_scoped_credential_evidence_before_following_actions() {
        use safe_cex_engine::{AccountReadScope, Action, AuthOutcome, CredentialSlot, Notice};
        let encoded = super::encode(&[
            Action::Notice(Notice::AccountAuth {
                credential: CredentialSlot::Secondary,
                scope: AccountReadScope::Orders,
                outcome: AuthOutcome::Rejected,
            }),
            Action::Notice(Notice::AccountAuth {
                credential: CredentialSlot::Primary,
                scope: AccountReadScope::Balance,
                outcome: AuthOutcome::Accepted,
            }),
            Action::Notice(Notice::CommandDone {
                command: 7,
                result: "[]".into(),
            }),
        ]);
        assert_eq!(
            encoded,
            r#"[{"op":"accountAuth","credential":"secondary","scope":"orders","outcome":"rejected"},{"op":"accountAuth","credential":"primary","scope":"balance","outcome":"accepted"},{"op":"done","command":7,"result":"[]"}]"#
        );
    }

    #[test]
    fn command_progress_serializes_native_identity_namespaces_in_action_order() {
        use safe_cex_engine::{Action, Notice, OrderIdentityKind, OrderIdentityReceipt};
        let receipts = [
            (OrderIdentityKind::ClientOrderId, "client"),
            (OrderIdentityKind::ClientOrderIndex, "9007199254740993"),
            (OrderIdentityKind::OrderCode, "code"),
        ]
        .into_iter()
        .map(|(kind, identity)| OrderIdentityReceipt {
            symbol: "BTC-USD".into(),
            kind,
            identity: identity.into(),
        })
        .collect::<Vec<_>>();
        let encoded = super::encode(&[
            Action::Notice(Notice::CommandProgress {
                command: 19,
                receipts,
            }),
            Action::Notice(Notice::CommandDone {
                command: 19,
                result: "[]".into(),
            }),
        ]);
        assert_eq!(
            encoded,
            r#"[{"op":"commandProgress","command":19,"receipts":[{"symbol":"BTC-USD","kind":"clientOrderId","identity":"client"},{"symbol":"BTC-USD","kind":"clientOrderIndex","identity":"9007199254740993"},{"symbol":"BTC-USD","kind":"orderCode","identity":"code"}]},{"op":"done","command":19,"result":"[]"}]"#
        );
    }

    #[test]
    fn attributed_command_errors_preserve_command_and_precede_result() {
        use safe_cex_engine::{Action, Notice};
        let encoded = super::encode(&[
            Action::Notice(Notice::CommandError {
                command: 19,
                message: "placement rejected".into(),
            }),
            Action::Notice(Notice::CommandFailed {
                command: 19,
                error: safe_cex_engine::VenueError::new("placement rejected"),
            }),
        ]);
        assert_eq!(
            encoded,
            r#"[{"op":"commandError","command":19,"message":"placement rejected"},{"op":"done","command":19,"error":"placement rejected","detail":{"message":"placement rejected"}}]"#
        );
    }

    #[test]
    fn every_selected_venue_build_accepts_polling_control() {
        use safe_cex_engine::command::{HostInput, decode_host_input};

        assert_eq!(
            decode_host_input(r#"{"op":"setPollScale","scale":2.5}"#).unwrap(),
            HostInput::PollScaleChanged {
                scale: 2.5,
                ceiling_ms: None
            }
        );
        assert_eq!(
            decode_host_input(r#"{"op":"setPollScale","scale":null}"#).unwrap(),
            HostInput::PollScaleChanged {
                scale: f64::INFINITY,
                ceiling_ms: None,
            }
        );
    }

    #[test]
    fn host_commands_dispatch_once_and_flush_once_with_original_clock() {
        use safe_cex_engine::command::HostInput;
        use std::cell::{Cell, RefCell};
        use std::rc::Rc;

        struct Commands {
            events: Rc<RefCell<Vec<(f64, HostInput)>>>,
            drains: Rc<Cell<usize>>,
        }
        impl AnyEngine for Commands {
            fn external_signer(
                &mut self,
                _: Option<Vec<safe_cex_engine::signing::Algorithm>>,
            ) -> Result<(), safe_cex_engine::VenueError> {
                Ok(())
            }
            fn prepare_dispatch(&mut self, _: f64) {}
            fn capabilities(&self) -> safe_cex_engine::Capabilities {
                safe_cex_engine::Capabilities::BASE
            }
            fn apply_public_feed(
                &mut self,
                now: f64,
                update: safe_cex_engine::PublicFeedUpdate,
            ) -> Result<(), &'static str> {
                self.events
                    .borrow_mut()
                    .push((now, HostInput::PublicFeedUpdate(update)));
                Ok(())
            }
            fn start(&mut self, _: f64) {}
            fn handle(&mut self, now: f64, event: Event<'_>) {
                let input = match event {
                    Event::Command(command) => HostInput::Command {
                        command,
                        timeout_ms: None,
                    },
                    Event::PollScaleChanged { scale } => HostInput::PollScaleChanged {
                        scale,
                        ceiling_ms: None,
                    },
                    Event::PollCeilingChanged { .. } => return,
                    _ => panic!("command boundary"),
                };
                self.events.borrow_mut().push((now, input));
            }
            fn drain(&mut self) -> Vec<Action> {
                self.drains.set(self.drains.get() + 1);
                Vec::new()
            }
            fn frame(&mut self, _: f64, _: bool) -> Option<&[u8]> {
                None
            }
            fn snapshot(&mut self, _: f64) -> &[u8] {
                &[]
            }
            fn account_snapshot(&mut self, _: f64) -> &[u8] {
                &[]
            }
        }
        let events = Rc::new(RefCell::new(Vec::new()));
        let drains = Rc::new(Cell::new(0));
        let mut engine = WasmEngine {
            dispatch_signing: false,
            #[cfg(feature = "hosted-signing")]
            hosted_identity: None,
            #[cfg(feature = "hosted-signing")]
            hosted_captures: Default::default(),
            #[cfg(feature = "hosted-wallet")]
            hosted_wallet: None,
            #[cfg(feature = "algos")]
            algos: Default::default(),
            external_signer: false,
            construction: "{}".into(),
            inner: Box::new(Commands {
                events: Rc::clone(&events),
                drains: Rc::clone(&drains),
            }),
        };
        let commands = [
            r#"{"op":"subscribeOrderBook","symbol":"BTCUSDT"}"#,
            r#"{"op":"unsubscribeOrderBook","symbol":"BTCUSDT"}"#,
            r#"{"op":"subscribeCandles","symbol":"BTCUSDT","interval":"1m"}"#,
            r#"{"op":"unsubscribeCandles","symbol":"BTCUSDT","interval":"1m"}"#,
            r#"{"op":"setTickerSymbols","symbols":["BTCUSDT"]}"#,
            r#"{"op":"setPollScale","scale":4}"#,
            r#"{"op":"setPollScale","scale":null}"#,
            r#"{"op":"publicFeedUpdate","tickers":[]}"#,
        ];
        let mut dispatched = 0;
        for (index, json) in commands.iter().enumerate() {
            let now = index as f64 + 0.5;
            match safe_cex_engine::command::decode_host_input(json) {
                Ok(expected) => {
                    assert_eq!(engine.command(json, now).unwrap(), "");
                    assert_eq!(events.borrow()[dispatched], (now, expected));
                    dispatched += 1;
                }
                Err(_) => {
                    // JsError construction invokes JS and cannot run on a native target.
                    // Native tests cover decoder rejection; WASM also checks the host ABI.
                    #[cfg(target_arch = "wasm32")]
                    assert!(engine.command(json, now).is_err());
                }
            }
            assert_eq!(drains.get(), dispatched);
            assert_eq!(events.borrow().len(), dispatched);
        }
    }

    #[test]
    fn account_command_aliases_share_the_engine_read() {
        for op in ["getAccount", "fetchAccount"] {
            let parsed = safe_cex_engine::command::decode_command(&format!(
                r#"{{"op":"{op}","command":77}}"#
            ));
            assert!(matches!(
                parsed.unwrap(),
                Command::FetchAccount { command: 77 }
            ));
        }
    }

    #[test]
    fn http_route_metadata_is_optional_and_has_stable_host_names() {
        let actions = [
            None,
            Some(safe_cex_engine::RequestRoute::Direct),
            Some(safe_cex_engine::RequestRoute::PublicCors),
            Some(safe_cex_engine::RequestRoute::Primary),
        ]
        .into_iter()
        .map(|route| {
            Action::Http(safe_cex_engine::HttpRequest {
                id: safe_cex_engine::RequestId(1),
                method: safe_cex_engine::Method::Get,
                route,
                timeout_ms: route.map(|_| 5_000),
                url: "https://venue.test/time".into(),
                headers: vec![],
                signer_headers: vec![],
                body: None,
            })
        })
        .collect::<Vec<_>>();
        let encoded = encode(&actions);
        let rows: Vec<serde_json::Value> = serde_json::from_str(&encoded).unwrap();
        assert!(rows[0].get("route").is_none());
        assert_eq!(rows[1]["route"], "direct");
        assert_eq!(rows[2]["route"], "publicCors");
        assert_eq!(rows[3]["route"], "primary");
        assert!(rows[0].get("timeoutMs").is_none());
        assert_eq!(rows[1]["timeoutMs"], 5_000);
        assert_eq!(rows[2]["timeoutMs"], 5_000);
        assert!(
            rows.iter()
                .all(|row| row["headers"] == serde_json::json!([]))
        );
    }

    #[test]
    fn structured_failures_keep_the_legacy_error_text_beside_their_detail() {
        let ids = ["a".to_owned()];
        let actions = [
            Action::Notice(Notice::CommandFailed {
                command: 1,
                error: safe_cex_engine::VenueError::new("leg 2 rejected").partial(&ids, 2, 1),
            }),
            Action::Notice(Notice::CommandFailed {
                command: 2,
                error: safe_cex_engine::VenueError::new("plain"),
            }),
            Action::Notice(Notice::CommandDone {
                command: 3,
                result: "[]".into(),
            }),
        ];
        assert_eq!(
            encode(&actions),
            concat!(
                r#"[{"op":"done","command":1,"#,
                r#""error":"{\"message\":\"leg 2 rejected\",\"partialOrderIds\":[\"a\"],\"partialOrderCount\":1,\"totalOrderCount\":2,\"failedOrderIndex\":1}","#,
                r#""detail":{"message":"leg 2 rejected","partial":{"orderIds":["a"],"total":2,"failedIndex":1}}},"#,
                r#"{"op":"done","command":2,"error":"plain","detail":{"message":"plain"}},"#,
                r#"{"op":"done","command":3,"result":"[]"}]"#,
            )
        );
    }

    use super::*;
    use safe_cex_engine::Command;
    #[cfg(any(feature = "asterdex", feature = "kucoinspot"))]
    use safe_cex_engine::model::CancelOrderRef;
    #[cfg(feature = "asterdex")]
    use safe_cex_engine::model::StpMode;

    #[test]
    fn amendment_symbol_context_is_preserved_for_spot_builds() {
        let wire = safe_cex_engine::command::decode_command(
            r#"{"op":"amendOrderWithSymbol","command":19,"id":"unobserved","symbol":"BTC-USDT","price":100}"#,
        );
        #[cfg(feature = "kucoinspot")]
        assert!(
            matches!(wire.unwrap(), Command::AmendOrderWithSymbol { command: 19, id, symbol, price: Some(100.0), amount: None }
            if id == "unobserved" && symbol == "BTC-USDT")
        );
        #[cfg(not(feature = "kucoinspot"))]
        assert!(wire.is_err());
        let legacy = safe_cex_engine::command::decode_command(
            r#"{"op":"amendOrder","command":19,"id":"legacy","symbol":"ignored","price":100}"#,
        )
        .unwrap();
        assert!(
            matches!(legacy, Command::AmendOrder { command: 19, id, price: Some(100.0), amount: None } if id == "legacy")
        );
    }

    #[cfg(feature = "text-ingress")]
    #[derive(Default)]
    struct Messages {
        actions: Vec<Action>,
    }

    #[cfg(feature = "text-ingress")]
    impl AnyEngine for Messages {
        fn external_signer(
            &mut self,
            _: Option<Vec<safe_cex_engine::signing::Algorithm>>,
        ) -> Result<(), safe_cex_engine::VenueError> {
            Ok(())
        }
        fn prepare_dispatch(&mut self, _: f64) {}
        fn capabilities(&self) -> safe_cex_engine::Capabilities {
            safe_cex_engine::Capabilities::BASE
        }

        fn apply_public_feed(
            &mut self,
            _now: f64,
            _update: safe_cex_engine::PublicFeedUpdate,
        ) -> Result<(), &'static str> {
            Err("message-only test engine")
        }
        fn start(&mut self, _now: f64) {}
        fn handle(&mut self, _now: f64, event: Event<'_>) {
            if let Event::WsMessage { socket, text } = event
                && text != "quiet"
            {
                for prefix in ["first", "second"] {
                    self.actions.push(Action::WsSend {
                        socket,
                        text: format!("{prefix}:{text}"),
                    });
                }
            }
        }
        fn drain(&mut self) -> Vec<Action> {
            std::mem::take(&mut self.actions)
        }
        fn frame(&mut self, _now: f64, _market_only: bool) -> Option<&[u8]> {
            None
        }
        fn snapshot(&mut self, _now: f64) -> &[u8] {
            &[]
        }
        fn account_snapshot(&mut self, _now: f64) -> &[u8] {
            &[]
        }
    }

    #[test]
    fn events_and_health_encode_as_the_documented_json() {
        let event = Action::Notice(Notice::Event(safe_cex_engine::LogEvent {
            level: safe_cex_engine::Level::Warn,
            code: "auth.rejected",
            message: "rejected \"key\"".into(),
            fields: vec![
                ("key", Value::Redacted),
                ("status", Value::Num(401.)),
                ("ratio", Value::Num(f64::NAN)),
                ("retry", Value::Bool(false)),
                ("venue", Value::Str("bybit".into())),
            ],
        }));
        assert_eq!(
            encode(&[event]),
            r#"[{"op":"event","level":"warn","code":"auth.rejected","message":"rejected \"key\"","fields":{"key":"[redacted]","status":401.0,"ratio":null,"retry":false,"venue":"bybit"}}]"#
        );

        struct Counted;
        impl AnyEngine for Counted {
            fn external_signer(
                &mut self,
                _: Option<Vec<safe_cex_engine::signing::Algorithm>>,
            ) -> Result<(), safe_cex_engine::VenueError> {
                Ok(())
            }
            fn prepare_dispatch(&mut self, _: f64) {}
            fn capabilities(&self) -> safe_cex_engine::Capabilities {
                safe_cex_engine::Capabilities::BASE
            }
            fn apply_public_feed(
                &mut self,
                _: f64,
                _: safe_cex_engine::PublicFeedUpdate,
            ) -> Result<(), &'static str> {
                Ok(())
            }
            fn start(&mut self, _: f64) {}
            fn handle(&mut self, _: f64, _: Event<'_>) {}
            fn drain(&mut self) -> Vec<Action> {
                Vec::new()
            }
            fn frame(&mut self, _: f64, _: bool) -> Option<&[u8]> {
                None
            }
            fn snapshot(&mut self, _: f64) -> &[u8] {
                &[]
            }
            fn account_snapshot(&mut self, _: f64) -> &[u8] {
                &[]
            }
            fn health(&self) -> Health {
                Health {
                    ws_opened: 1,
                    ws_closed: 2,
                    reconnects: 3,
                    auth_failures: 4,
                    http_failures: 5,
                    rate_limited: 6,
                    snapshots_discarded: 7,
                    rows_skipped: 8,
                    last_private_message_ms: Some(1_700_000_000_123.0),
                }
            }
        }
        let health: serde_json::Value = serde_json::from_str(
            &WasmEngine {
                dispatch_signing: false,
                #[cfg(feature = "hosted-signing")]
                hosted_identity: None,
                #[cfg(feature = "hosted-signing")]
                hosted_captures: Default::default(),
                #[cfg(feature = "hosted-wallet")]
                hosted_wallet: None,
                #[cfg(feature = "algos")]
                algos: Default::default(),
                external_signer: false,
                construction: "{}".into(),
                inner: Box::new(Counted),
            }
            .health(),
        )
        .unwrap();
        assert_eq!(
            health,
            serde_json::json!({"wsOpened":1,"wsClosed":2,"reconnects":3,"authFailures":4,
                "httpFailures":5,"rateLimited":6,"snapshotsDiscarded":7,"rowsSkipped":8,
                "lastPrivateMessage":1_700_000_000_123u64})
        );
    }

    #[cfg(feature = "text-ingress")]
    #[test]
    fn numeric_websocket_abi_round_trips_ids_above_u32_without_bigint() {
        for id in [
            u64::from(u32::MAX) + 1,
            safe_cex_engine::resource_ids::MAX_RESOURCE_ID,
        ] {
            let mut engine = WasmEngine {
                dispatch_signing: false,
                #[cfg(feature = "hosted-signing")]
                hosted_identity: None,
                #[cfg(feature = "hosted-signing")]
                hosted_captures: Default::default(),
                #[cfg(feature = "hosted-wallet")]
                hosted_wallet: None,
                #[cfg(feature = "algos")]
                algos: Default::default(),
                external_signer: false,
                construction: "{}".into(),
                inner: Box::<Messages>::default(),
            };
            let text = engine.ws_message(id as f64, "wide", 1.).unwrap();
            let rows: serde_json::Value = serde_json::from_str(&text).unwrap();
            assert_eq!(rows.as_array().unwrap().len(), 2);
            assert!(
                rows.as_array()
                    .unwrap()
                    .iter()
                    .all(|row| row["socket"].as_u64() == Some(id))
            );
        }
    }

    #[cfg(feature = "text-ingress")]
    #[test]
    fn optional_websocket_actions_preserve_legacy_output_order_and_drain_once() {
        let engine = || WasmEngine {
            dispatch_signing: false,
            #[cfg(feature = "hosted-signing")]
            hosted_identity: None,
            #[cfg(feature = "hosted-signing")]
            hosted_captures: Default::default(),
            #[cfg(feature = "hosted-wallet")]
            hosted_wallet: None,
            #[cfg(feature = "algos")]
            algos: Default::default(),
            external_signer: false,
            construction: "{}".into(),
            inner: Box::<Messages>::default(),
        };
        let mut legacy = engine();
        let mut optional = engine();
        assert_eq!(legacy.ws_message(7., "quiet", 1.).unwrap(), "");
        assert_eq!(optional.ws_message_actions(7., "quiet", 1.).unwrap(), None);
        let expected = legacy.ws_message(7., "payload", 2.).unwrap();
        assert_eq!(
            expected,
            r#"[{"op":"wsSend","socket":7,"text":"first:payload"},{"op":"wsSend","socket":7,"text":"second:payload"}]"#
        );
        assert_eq!(
            optional.ws_message_actions(7., "payload", 2.).unwrap(),
            Some(expected)
        );
        assert_eq!(legacy.ws_message(7., "quiet", 3.).unwrap(), "");
        assert_eq!(optional.ws_message_actions(7., "quiet", 3.).unwrap(), None);
    }

    #[cfg(feature = "text-ingress")]
    #[test]
    fn byte_websocket_actions_validate_utf8_before_handling_or_draining() {
        let mut legacy = WasmEngine {
            dispatch_signing: false,
            #[cfg(feature = "hosted-signing")]
            hosted_identity: None,
            #[cfg(feature = "hosted-signing")]
            hosted_captures: Default::default(),
            #[cfg(feature = "hosted-wallet")]
            hosted_wallet: None,
            #[cfg(feature = "algos")]
            algos: Default::default(),
            external_signer: false,
            construction: "{}".into(),
            inner: Box::<Messages>::default(),
        };
        let mut bytes = WasmEngine {
            dispatch_signing: false,
            #[cfg(feature = "hosted-signing")]
            hosted_identity: None,
            #[cfg(feature = "hosted-signing")]
            hosted_captures: Default::default(),
            #[cfg(feature = "hosted-wallet")]
            hosted_wallet: None,
            #[cfg(feature = "algos")]
            algos: Default::default(),
            external_signer: false,
            construction: "{}".into(),
            inner: Box::<Messages>::default(),
        };
        // A rejected byte message must neither handle an event nor drain existing actions.
        bytes.inner.handle(
            1.,
            Event::WsMessage {
                socket: SocketId(7),
                text: "pending",
            },
        );
        for invalid in [
            &[0xff][..],
            &[0xc0, 0xaf],
            &[0xed, 0xa0, 0x80],
            &[0xf0, 0x9f],
        ] {
            assert_eq!(bytes.ws_message_bytes(7., invalid, 2.).unwrap(), None);
        }
        assert_eq!(
            bytes.ws_message_bytes(7., b"quiet", 3.).unwrap(),
            Some(legacy.ws_message(7., "pending", 1.).unwrap())
        );
        for text in ["", "ASCII", "é漢🦀\u{fffd}", "a\0b"] {
            assert_eq!(
                bytes.ws_message_bytes(7., text.as_bytes(), 4.).unwrap(),
                Some(legacy.ws_message(7., text, 4.).unwrap())
            );
            assert_eq!(bytes.ws_message_bytes(7., b"quiet", 5.).unwrap(), None);
        }
    }

    #[cfg(any(feature = "asterdex", feature = "kucoinspot"))]
    #[test]
    fn cancellation_with_symbols_preserves_caller_identity() {
        let wire = safe_cex_engine::command::decode_command(
            r#"{"op":"cancelOrdersWithSymbols","command":19,"orders":[{"id":"22542179","symbol":"BTCUSDT"}]}"#,
        ).unwrap();
        assert!(
            matches!(wire, Command::CancelOrdersWithSymbols { command: 19, orders }
            if orders == vec![CancelOrderRef { id: "22542179".into(), symbol: "BTCUSDT".into() }])
        );
    }

    // Compile command parsers only for venues that expose each SDK method.
    #[test]
    fn aster_commands_follow_the_compiled_venue_feature() {
        let commands = [
            r#"{"op":"cancelOrdersWithSymbols","command":19,"orders":[{"id":"7","symbol":"BTCUSDT"}]}"#,
            r#"{"op":"fetchStpMode","command":19}"#,
            r#"{"op":"fetchAccount","command":19}"#,
            r#"{"op":"fetchOrderBook","command":19,"symbol":"BTCUSDT","limit":50}"#,
            r#"{"op":"setStpMode","command":19,"mode":"EXPIRE_MAKER"}"#,
        ];
        for (index, json) in commands.into_iter().enumerate() {
            if index == 0 {
                assert_eq!(
                    safe_cex_engine::command::decode_command(json).is_ok(),
                    cfg!(any(feature = "asterdex", feature = "kucoinspot"))
                );
                continue;
            }
            let parsed = safe_cex_engine::command::decode_command(json);
            // Account identity queries are shared across every venue since #1430.
            if index == 2 {
                assert!(matches!(parsed, Ok(Command::FetchAccount { command: 19 })));
                continue;
            }
            #[cfg(feature = "asterdex")]
            match (index, parsed.unwrap()) {
                (
                    0,
                    Command::CancelOrdersWithSymbols {
                        command: 19,
                        orders,
                    },
                ) => {
                    assert_eq!(
                        orders,
                        vec![CancelOrderRef {
                            id: "7".into(),
                            symbol: "BTCUSDT".into()
                        }]
                    );
                }
                (1, Command::FetchStpMode { command: 19 })
                | (2, Command::FetchAccount { command: 19 })
                | (
                    4,
                    Command::SetStpMode {
                        command: 19,
                        mode: StpMode::ExpireMaker,
                    },
                ) => {}
                (
                    3,
                    Command::FetchOrderBook {
                        command: 19,
                        symbol,
                        limit: Some(50),
                    },
                ) => {
                    assert_eq!(symbol, "BTCUSDT");
                }
                _ => panic!("Aster command {index} changed its wire mapping"),
            }
            #[cfg(not(feature = "asterdex"))]
            if cfg!(feature = "polymarket") && matches!(index, 2 | 3) {
                let command = parsed.unwrap();
                assert!(matches!(
                    (index, command),
                    (2, Command::FetchAccount { command: 19 })
                        | (
                            3,
                            Command::FetchOrderBook {
                                command: 19,
                                symbol: _,
                                limit: Some(50)
                            }
                        )
                ));
            } else {
                assert!(
                    parsed.is_err(),
                    "unsupported venue command {index} must reject"
                );
            }
        }
    }

    #[test]
    fn forwards_leverage_metadata_command() {
        let wire = safe_cex_engine::command::decode_command(
            r#"{"op":"fetchLeverage","command":17,"symbol":"BTCUSDT"}"#,
        )
        .unwrap();
        assert!(matches!(
            wire,
            Command::FetchLeverage { command: 17, symbol } if symbol == "BTCUSDT"
        ));
    }
}

#[cfg(all(test, feature = "address-nonce", feature = "asterdex"))]
#[path = "../../native/tests/support/address_start.rs"]
mod address_start_fixture;
#[cfg(all(test, feature = "address-nonce", feature = "asterdex"))]
mod address_start_tests;
