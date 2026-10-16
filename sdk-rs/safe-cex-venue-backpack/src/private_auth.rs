//! Private authentication bodies with eager factories and native UTF-16 message values.
use crate::public_control::{JsString, Message, json_string};
use std::{cell::RefCell, future::Future, rc::Rc};

pub type AuthSignature = Rc<RefCell<[JsString; 4]>>;
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthMessage {
    pub method: &'static str,
    pub params: Vec<JsString>,
    pub signature: [JsString; 4],
}
impl AuthMessage {
    pub fn snapshot(streams: &[JsString], signature: &AuthSignature) -> Self {
        Self {
            method: "SUBSCRIBE",
            params: streams.to_vec(),
            signature: signature.borrow().clone(),
        }
    }
    pub fn to_json(&self) -> JsString {
        let mut out = Message {
            method: self.method,
            params: Some(self.params.iter().cloned().map(Some).collect()),
        }
        .to_json();
        out.pop();
        out.extend(",\"signature\":[".encode_utf16());
        for (index, value) in self.signature.iter().enumerate() {
            if index != 0 {
                out.push(44);
            }
            json_string(value, &mut out);
        }
        out.extend("]}".encode_utf16());
        out
    }
}
pub trait AuthHost {
    type Error;
    type Signer;
    type Emitter;
    type SignatureFuture: Future<Output = Result<AuthSignature, Self::Error>>;
    fn has_subscription(&mut self, stream: &[u16]) -> Result<bool, Self::Error>;
    fn add_subscription(&mut self, stream: JsString) -> Result<(), Self::Error>;
    fn signature_function(&mut self) -> Result<Self::Signer, Self::Error>;
    fn key(&mut self) -> Result<JsString, Self::Error>;
    fn secret(&mut self) -> Result<JsString, Self::Error>;
    fn generate_signature(
        &mut self,
        signer: Self::Signer,
        key: JsString,
        secret: JsString,
        window: f64,
    ) -> Result<Self::SignatureFuture, Self::Error>;
    /// Resolves send, then snapshots/serializes the supplied values and invokes that method.
    fn send_subscription(
        &mut self,
        streams: &[JsString],
        signature: &AuthSignature,
    ) -> Result<(), Self::Error>;
    fn error_text(&mut self, error: &Self::Error) -> Result<JsString, Self::Error>;
    fn error_emitter(&mut self) -> Result<Self::Emitter, Self::Error>;
    fn emit_error(&mut self, emitter: Self::Emitter, message: JsString) -> Result<(), Self::Error>;
}
fn recover<H: AuthHost>(host: &mut H, prefix: &str, error: &H::Error) -> Result<(), H::Error> {
    let emitter = host.error_emitter()?;
    let mut message: JsString = prefix.encode_utf16().collect();
    message.extend(host.error_text(error)?);
    host.emit_error(emitter, message)
}
enum Start<F, E> {
    Done(Result<(), E>),
    Await(F),
}
fn subscribe<H: AuthHost>(
    streams: Vec<JsString>,
    eligible: Result<bool, H::Error>,
    mut host: H,
    prefix: &'static str,
) -> impl Future<Output = Result<(), H::Error>> {
    let start = match eligible {
        Err(error) => Start::Done(Err(error)),
        Ok(false) => Start::Done(Ok(())),
        Ok(true) => {
            let signature = (|| {
                let signer = host.signature_function()?;
                let key = host.key()?;
                let secret = host.secret()?;
                host.generate_signature(signer, key, secret, 5000.0)
            })();
            match signature {
                Ok(future) => Start::Await(future),
                Err(error) => Start::Done(recover(&mut host, prefix, &error)),
            }
        }
    };
    async move {
        match start {
            Start::Done(result) => result,
            Start::Await(future) => match future.await {
                Err(error) => recover(&mut host, prefix, &error),
                Ok(signature) => {
                    if let Err(error) = host.send_subscription(&streams, &signature) {
                        return recover(&mut host, prefix, &error);
                    }
                    for stream in streams {
                        if let Err(error) = host.add_subscription(stream) {
                            return recover(&mut host, prefix, &error);
                        }
                    }
                    Ok(())
                }
            },
        }
    }
}
// ts: BackpackPrivateWebSocket#authenticate
pub fn authenticate<H: AuthHost>(host: H) -> impl Future<Output = Result<(), H::Error>> {
    subscribe(
        vec![
            "account.orderUpdate".encode_utf16().collect(),
            "account.positionUpdate".encode_utf16().collect(),
        ],
        Ok(true),
        host,
        "Failed to authenticate WebSocket: ",
    )
}
// ts: BackpackPrivateWebSocket.subscribeToOrderUpdates
pub fn subscribe_to_order_updates<H: AuthHost>(
    symbol: Option<&[u16]>,
    mut host: H,
) -> impl Future<Output = Result<(), H::Error>> + use<H> {
    let mut stream: JsString = "account.orderUpdate".encode_utf16().collect();
    if let Some(symbol) = symbol.filter(|s| !s.is_empty()) {
        stream.push(46);
        stream.extend_from_slice(symbol);
    }
    let eligible = host.has_subscription(&stream).map(|present| !present);
    subscribe(
        vec![stream],
        eligible,
        host,
        "Failed to subscribe to order updates: ",
    )
}
// ts: BackpackPrivateWebSocket.subscribeToPositionUpdates
pub fn subscribe_to_position_updates<H: AuthHost>(
    mut host: H,
) -> impl Future<Output = Result<(), H::Error>> {
    let stream: JsString = "account.positionUpdate".encode_utf16().collect();
    let eligible = host.has_subscription(&stream).map(|present| !present);
    subscribe(
        vec![stream],
        eligible,
        host,
        "Failed to subscribe to position updates: ",
    )
}
