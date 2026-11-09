use super::support::*;
use safe_cex_engine::request_context::RoutingIntent;
use safe_cex_engine::{Engine, HttpRequest, SocketChannel, SocketId, Venue};
use safe_cex_native::{
    BoundHttp, Events, Host, HostEvent, HttpPrepareError, HttpPrepareFuture, PreparedHttp,
    Transport,
};
use std::{fmt, time::Duration};
use tokio::sync::mpsc;

pub(super) enum Observed {
    Http(HttpRequest, Events),
    Socket(Option<SocketChannel>),
    Ws(String),
    Loaded(bool),
}
pub(super) struct Recorder(pub(super) mpsc::UnboundedSender<Observed>);
struct Prepared {
    request: HttpRequest,
    out: mpsc::UnboundedSender<Observed>,
}
struct Bound {
    request: HttpRequest,
    out: mpsc::UnboundedSender<Observed>,
}
impl fmt::Debug for Prepared {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Backpack prepared fixture")
    }
}
impl fmt::Debug for Bound {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Backpack bound fixture")
    }
}
impl PreparedHttp for Prepared {
    fn bind(
        self: Box<Self>,
        packet: HttpRequest,
    ) -> std::result::Result<Box<dyn BoundHttp>, HttpPrepareError> {
        assert_eq!(self.request.id, packet.id);
        assert_eq!(self.request.url, packet.url);
        assert_eq!(self.request.body, packet.body);
        Ok(Box::new(Bound {
            request: packet,
            out: self.out,
        }))
    }
}
impl BoundHttp for Bound {
    fn ready(&self) -> bool {
        true
    }
    fn dispatch(self: Box<Self>, events: Events) -> tokio::task::AbortHandle {
        assert!(self.out.send(Observed::Http(self.request, events)).is_ok());
        tokio::spawn(async {}).abort_handle()
    }
}
impl Transport for Recorder {
    fn prepare_http(&self, request: HttpRequest, _: RoutingIntent) -> HttpPrepareFuture {
        let out = self.0.clone();
        Box::pin(async move {
            let prepared: Box<dyn PreparedHttp> = Box::new(Prepared { request, out });
            Ok(prepared)
        })
    }
    fn request(&self, request: HttpRequest, events: Events) {
        assert!(self.0.send(Observed::Http(request, events)).is_ok());
    }
    fn connect(
        &self,
        socket: SocketId,
        url: String,
        events: Events,
    ) -> mpsc::UnboundedSender<String> {
        self.connect_channel(socket, url, vec![], None, events)
    }
    fn connect_channel(
        &self,
        socket: SocketId,
        _: String,
        headers: Vec<(&'static str, String)>,
        channel: Option<SocketChannel>,
        events: Events,
    ) -> mpsc::UnboundedSender<String> {
        assert!(headers.is_empty());
        assert!(self.0.send(Observed::Socket(channel)).is_ok());
        assert!(events.send(HostEvent::WsOpened(socket)).is_ok());
        let (sender, mut frames) = mpsc::unbounded_channel();
        let out = self.0.clone();
        tokio::spawn(async move {
            while let Some(text) = frames.recv().await {
                if out.send(Observed::Ws(text)).is_err() {
                    break;
                }
            }
        });
        sender
    }
}
struct Start {
    out: mpsc::UnboundedSender<Observed>,
}
impl safe_cex_venues::Visit for Start {
    type Output = (
        mpsc::UnboundedSender<safe_cex_engine::Command>,
        tokio::task::JoinHandle<()>,
    );
    fn visit<V: Venue + 'static>(self, engine: Engine<V>) -> Self::Output {
        let host = Host::new(engine, Recorder(self.out.clone()));
        let (commands, inbox) = mpsc::unbounded_channel();
        let task = tokio::task::spawn_local(async move {
            host.run_with_store(
                inbox,
                Duration::from_millis(5),
                move |_, store| {
                    let loaded = store.loaded();
                    let _ = self.out.send(Observed::Loaded(
                        loaded.markets
                            && loaded.tickers
                            && loaded.balance
                            && loaded.orders
                            && loaded.positions,
                    ));
                },
                |_| {},
            )
            .await;
        });
        (commands, task)
    }
}
pub async fn prove() -> Result {
    tokio::task::LocalSet::new().run_until(async {
        let mut device=Device::new()?;
        let (remote,options)=device.remote()?;
        let (out,mut observations)=mpsc::unbounded_channel();
        let (commands,task)=safe_cex_venues::build_with_signer("backpack",serde_json::from_str(&options)?,Some(safe_cex_engine::test_support::seeded_entropy(5)),Some(remote.provider),Start {out})?;
        let mut paths=std::collections::BTreeSet::new();
        let mut private=false;
        let mut ws=false;
        let mut loaded=false;
        tokio::time::timeout(Duration::from_secs(10),async {
            while !(loaded&&private&&ws&&paths.len()==4) {
                tokio::select! {
                    frame=device.frames.recv()=>device.answer(&frame.ok_or("device channel closed")?)?,
                    observation=observations.recv()=>match observation.ok_or("host channel closed")? {
                        Observed::Http(request,events)=>{
                            for path in ["/account","/collateral","/position","/orders"] {if request.url.ends_with(path) {assert_signature_shape(&request)?;paths.insert(path);}}
                            let body=startup_body(&request.url,"[]");
                            events.send(HostEvent::HttpStart {request:request.id,status:200,headers:String::new()})?;
                            events.send(HostEvent::HttpChunk(request.id,body.into_bytes()))?;
                            events.send(HostEvent::HttpEnd(request.id))?;
                        }
                        Observed::Socket(channel)=>private|=channel==Some(SocketChannel::Private),
                        Observed::Ws(text)=>{
                            let value:serde_json::Value=serde_json::from_str(&text)?;
                            if value.get("signature").is_some() {assert!(text.contains("account.orderUpdate"));ws=true;}
                        }
                        Observed::Loaded(value)=>loaded|=value,
                    }
                }
            }
            Ok::<(),Error>(())
        }).await??;
        assert!(device.payloads.iter().any(|payload|payload.starts_with("instruction=subscribe&")&&payload.ends_with("&window=5000")));
        drop(commands); task.await?;
        Ok(())
    }).await
}
