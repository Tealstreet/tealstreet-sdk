//! Where a host's sockets and requests go: the network, or a recording.
use crate::{Events, HostEvent, now_ms};
use futures_util::{SinkExt, StreamExt};
use safe_cex_engine::{HttpRequest, Method, SocketId};
use std::io::Read;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::{
    Message,
    client::IntoClientRequest,
    http::{HeaderName, HeaderValue},
};

pub trait Transport {
    /// Open a socket; report it on `events` and return where to send text.
    /// Dropping the returned sender closes the socket.
    fn connect(
        &self,
        socket: SocketId,
        url: String,
        events: Events,
    ) -> mpsc::UnboundedSender<String>;
    /// Unsupported transports report a close instead of dropping authentication.
    fn connect_with_headers(
        &self,
        socket: SocketId,
        _url: String,
        _headers: Vec<(&'static str, String)>,
        events: Events,
    ) -> mpsc::UnboundedSender<String> {
        let _ = events.send(HostEvent::WsClosed(socket, 1006));
        mpsc::unbounded_channel().0
    }
    /// Run a request, reporting its start, body chunks and end on `events`.
    /// Bound its lifetime and report `HttpFailed` on timeout, including the body.
    fn request(&self, request: HttpRequest, events: Events);
}

/// Real sockets (tungstenite over rustls) and HTTP (reqwest), body streamed.
#[derive(Clone)]
pub struct NetworkTransport {
    http: reqwest::Client,
}

impl NetworkTransport {
    pub fn new() -> Self {
        // One process-wide rustls provider: ring, shared with tungstenite.
        let _ = rustls::crypto::ring::default_provider().install_default();
        Self {
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(20))
                .build()
                .expect("an HTTP client with the default configuration"),
        }
    }
}

impl Default for NetworkTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl Transport for NetworkTransport {
    fn connect(
        &self,
        socket: SocketId,
        url: String,
        events: Events,
    ) -> mpsc::UnboundedSender<String> {
        self.connect_with_headers(socket, url, vec![], events)
    }

    fn connect_with_headers(
        &self,
        socket: SocketId,
        url: String,
        headers: Vec<(&'static str, String)>,
        events: Events,
    ) -> mpsc::UnboundedSender<String> {
        let (outgoing, mut pending) = mpsc::unbounded_channel::<String>();
        tokio::spawn(async move {
            let request = (|| {
                let mut request = url.into_client_request().ok()?;
                for (name, value) in headers {
                    request.headers_mut().insert(
                        HeaderName::from_bytes(name.as_bytes()).ok()?,
                        HeaderValue::from_str(&value).ok()?,
                    );
                }
                Some(request)
            })();
            let Some(request) = request else {
                let _ = events.send(HostEvent::WsClosed(socket, 1006));
                return;
            };
            let mut ws = match tokio_tungstenite::connect_async(request).await {
                Ok((ws, _)) => ws,
                Err(_) => {
                    let _ = events.send(HostEvent::WsClosed(socket, 1006));
                    return;
                }
            };
            let _ = events.send(HostEvent::WsOpened(socket));
            let code = loop {
                tokio::select! {
                    message = ws.next() => match message {
                        Some(Ok(Message::Text(text))) => {
                            let received_at = now_ms();
                            if events.send_at(HostEvent::WsMessage(socket, text.to_string()), received_at).is_err() {
                                return;
                            }
                        }
                        Some(Ok(Message::Binary(bytes))) => {
                            let received_at = now_ms();
                            let mut decoded = Vec::new();
                            let result = if bytes.starts_with(&[0x1f, 0x8b]) {
                                flate2::read::GzDecoder::new(bytes.as_ref()).read_to_end(&mut decoded)
                            } else if bytes.first() == Some(&0x78) {
                                flate2::read::ZlibDecoder::new(bytes.as_ref()).read_to_end(&mut decoded)
                            } else {
                                decoded.extend_from_slice(&bytes);
                                Ok(decoded.len())
                            };
                            let Ok(text) = result.and_then(|_| String::from_utf8(decoded).map_err(std::io::Error::other)) else {break 1007};
                            if events.send_at(HostEvent::WsMessage(socket, text), received_at).is_err() {
                                return;
                            }
                        }
                        // Answering pings needs a flush on a quiet socket.
                        Some(Ok(Message::Ping(_))) => {
                            let _ = ws.flush().await;
                        }
                        Some(Ok(Message::Close(frame))) => break frame.map_or(1005, |f| u16::from(f.code)),
                        Some(Ok(_)) => {}
                        Some(Err(_)) | None => break 1006,
                    },
                    text = pending.recv() => match text {
                        Some(text) => {
                            if ws.send(Message::text(text)).await.is_err() {
                                break 1006;
                            }
                        }
                        None => {
                            let _ = ws.close(None).await;
                            return;
                        }
                    },
                }
            };
            let _ = events.send(HostEvent::WsClosed(socket, code));
        });
        outgoing
    }

    fn request(&self, request: HttpRequest, events: Events) {
        let http = self.http.clone();
        tokio::spawn(async move {
            let id = request.id;
            let method = match request.method {
                Method::Get => reqwest::Method::GET,
                Method::Post => reqwest::Method::POST,
                Method::Put => reqwest::Method::PUT,
                Method::Patch => reqwest::Method::PATCH,
                Method::Delete => reqwest::Method::DELETE,
            };
            let mut builder = http.request(method, &request.url);
            for (name, value) in &request.headers {
                builder = builder.header(*name, value);
            }
            if let Some(body) = request.body {
                builder = builder.body(body);
            }
            let response = match builder.send().await {
                Ok(response) => response,
                Err(error) => {
                    let _ = events.send(HostEvent::HttpFailed(id, error.to_string()));
                    return;
                }
            };
            let headers = response
                .headers()
                .iter()
                .filter_map(|(name, value)| Some(format!("{name}: {}", value.to_str().ok()?)))
                .collect::<Vec<_>>()
                .join("\n");
            let _ = events.send(HostEvent::HttpStart {
                request: id,
                status: response.status().as_u16(),
                headers,
            });
            let mut body = response.bytes_stream();
            while let Some(chunk) = body.next().await {
                match chunk {
                    Ok(chunk) => {
                        let _ = events.send(HostEvent::HttpChunk(id, chunk.to_vec()));
                    }
                    Err(error) => {
                        let _ = events.send(HostEvent::HttpFailed(id, error.to_string()));
                        return;
                    }
                }
            }
            let _ = events.send(HostEvent::HttpEnd(id));
        });
    }
}
