//! Where a host's sockets and requests go: the network, or a recording.
use crate::{Events, HostEvent};
use futures_util::{SinkExt, StreamExt};
use safe_cex_engine::{HttpRequest, Method, SocketId};
use std::io::Read;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::{
    Message,
    client::IntoClientRequest,
    http::{HeaderName, HeaderValue},
};

const MAX_BINARY_BYTES: usize = 16 * 1024 * 1024;

fn binary_text(bytes: &[u8]) -> Result<String, u16> {
    let mut decoded = Vec::new();
    let limit = MAX_BINARY_BYTES as u64 + 1;
    if bytes.starts_with(&[0x1f, 0x8b]) {
        flate2::read::GzDecoder::new(bytes)
            .take(limit)
            .read_to_end(&mut decoded)
            .map_err(|_| 1007u16)?;
    } else if bytes.first() == Some(&0x78)
        && bytes
            .get(1)
            .is_some_and(|b| (0x7800 + u16::from(*b)).is_multiple_of(31))
    {
        flate2::read::ZlibDecoder::new(bytes)
            .take(limit)
            .read_to_end(&mut decoded)
            .map_err(|_| 1007u16)?;
    } else {
        if bytes.len() > MAX_BINARY_BYTES {
            return Err(1009);
        }
        decoded.extend_from_slice(bytes);
    }
    if decoded.len() > MAX_BINARY_BYTES {
        return Err(1009);
    }
    String::from_utf8(decoded).map_err(|_| 1007)
}

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
                            let _ = events.send(HostEvent::WsMessage(socket, text.to_string()));
                        }
                        Some(Ok(Message::Binary(bytes))) => {
                            let text = match binary_text(&bytes) {
                                Ok(text) => text,
                                Err(code) => break code,
                            };
                            let _ = events.send(HostEvent::WsMessage(socket, text));
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

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::{
        Compression,
        write::{GzEncoder, ZlibEncoder},
    };
    use std::io::Write;

    fn gzip(bytes: &[u8]) -> Vec<u8> {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
        encoder.write_all(bytes).unwrap();
        encoder.finish().unwrap()
    }

    fn zlib(bytes: &[u8]) -> Vec<u8> {
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::fast());
        encoder.write_all(bytes).unwrap();
        encoder.finish().unwrap()
    }

    #[test]
    fn binary_frames_decode_plain_gzip_and_zlib_utf8() {
        let text = "{\"book\":\"€\"}";
        for bytes in [
            text.as_bytes().to_vec(),
            gzip(text.as_bytes()),
            zlib(text.as_bytes()),
        ] {
            assert_eq!(binary_text(&bytes).unwrap(), text);
        }
    }

    #[test]
    fn corrupt_compressed_frames_and_invalid_utf8_are_rejected_whole() {
        let mut corrupted = gzip(b"{\"book\":1}");
        let crc = corrupted.len() - 8;
        corrupted[crc] ^= 1;
        for bytes in [corrupted, vec![0x1f, 0x8b, 0], vec![0xff], zlib(&[0xff])] {
            assert_eq!(binary_text(&bytes), Err(1007));
        }
    }

    #[test]
    fn decoded_binary_limit_rejects_oversize_without_returning_a_prefix() {
        let bytes = vec![b'a'; MAX_BINARY_BYTES + 1];
        for encoded in [gzip(&bytes), zlib(&bytes)] {
            assert_eq!(binary_text(&encoded), Err(1009));
        }
        assert_eq!(binary_text(&bytes), Err(1009));
    }
}
