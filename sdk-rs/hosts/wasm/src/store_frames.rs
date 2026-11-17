//! LOCAL host native frame sections, including host-observed socket state.
use safe_cex_engine::{Action, SocketChannel, SocketId};
use safe_cex_store_frames::{Emitter, Inputs};
use std::{cell::RefCell, collections::HashMap, rc::Rc};

#[derive(Default)]
struct Stats {
    open: u32,
    initialized: u32,
    last: f64,
    close: Option<u16>,
    closed_at: f64,
    closes: u32,
    streak: u32,
}
struct Socket {
    groups: Vec<usize>,
    open: bool,
    initialized: bool,
}
#[derive(Default)]
pub(crate) struct LocalFrames {
    pub(crate) inputs: Rc<RefCell<Inputs>>,
    emitter: Emitter,
    sockets: HashMap<SocketId, Socket>,
    stats: [Stats; 3],
    now: f64,
}
impl LocalFrames {
    pub(crate) fn new(venue: &str) -> Self {
        let this = Self::default();
        this.inputs.borrow_mut().configure("", venue);
        this
    }
    pub(crate) fn actions(&mut self, actions: &[Action]) {
        for action in actions {
            match action {
                Action::WsConnect {
                    socket,
                    channel,
                    public_topics,
                    ..
                } => {
                    self.closed(*socket, None, self.now);
                    let role = match channel {
                        Some(SocketChannel::Private) => 2,
                        Some(SocketChannel::Business) => 1,
                        _ => 0,
                    };
                    let mut groups = vec![role];
                    if *public_topics && role != 0 {
                        groups.push(0);
                    }
                    self.sockets.insert(
                        *socket,
                        Socket {
                            groups,
                            open: false,
                            initialized: false,
                        },
                    );
                }
                Action::WsClose { socket } => self.closed(*socket, None, self.now),
                _ => {}
            }
        }
    }
    pub(crate) fn opened(&mut self, id: SocketId, now: f64) {
        self.now = now;
        if let Some(s) = self.sockets.get_mut(&id)
            && !s.open
        {
            s.open = true;
            for &g in &s.groups {
                self.stats[g].open += 1;
            }
        }
    }
    pub(crate) fn received(&mut self, id: SocketId, now: f64) {
        self.now = now;
        if let Some(s) = self.sockets.get_mut(&id) {
            for &g in &s.groups {
                let stat = &mut self.stats[g];
                stat.last = now;
                if s.open && !s.initialized {
                    stat.initialized += 1;
                    stat.streak = 0;
                }
            }
            s.initialized |= s.open;
        }
    }
    pub(crate) fn closed(&mut self, id: SocketId, code: Option<u16>, now: f64) {
        self.now = now;
        if let Some(s) = self.sockets.remove(&id) {
            for g in s.groups {
                let stat = &mut self.stats[g];
                stat.open -= u32::from(s.open);
                stat.initialized -= u32::from(s.initialized);
                if let Some(code) = code {
                    stat.close = Some(code);
                    stat.closed_at = now;
                    stat.closes += 1;
                    stat.streak += 1;
                }
            }
        }
    }
    pub(crate) fn frame(
        &mut self,
        frame: Vec<u8>,
        store: Option<&safe_cex_engine::Store>,
        snapshot: bool,
        now: f64,
    ) -> Vec<u8> {
        let Some(store) = store else {
            return frame;
        };
        self.now = now;
        let summary = |groups: &[usize]| {
            let initialized: Vec<_> = groups
                .iter()
                .map(|&g| &self.stats[g])
                .filter(|s| s.initialized > 0)
                .collect();
            let healthy = initialized.is_empty()
                || initialized
                    .iter()
                    .any(|s| s.open > 0 && s.last > 0.0 && now - s.last < 60_000.0);
            let last = initialized
                .iter()
                .map(|s| s.last)
                .reduce(f64::max)
                .filter(|t| *t > 0.0);
            let latest = groups
                .iter()
                .map(|&g| &self.stats[g])
                .filter(|s| s.close.is_some())
                .max_by(|a, b| a.closed_at.total_cmp(&b.closed_at));
            (
                healthy,
                !initialized.is_empty(),
                last,
                latest.and_then(|s| s.close),
                groups.iter().map(|&g| self.stats[g].closes).sum::<u32>(),
                groups.iter().map(|&g| self.stats[g].streak).sum::<u32>(),
            )
        };
        let public = summary(&[0, 1]);
        let private = summary(&[2]);
        let mut input = self.inputs.borrow_mut();
        input.socket_health(&serde_json::json!({
            "publicHealthy":public.0,"privateHealthy":private.0,"publicInitialized":public.1,"privateInitialized":private.1,
            "publicLastMessage":public.2,"privateLastMessage":private.2,"publicCloseCode":public.3,"privateCloseCode":private.3,
            "publicCloseCountTotal":public.4,"privateCloseCountTotal":private.4,"publicCloseStreak":public.5,"privateCloseStreak":private.5,
            "publicReconnectPaused":false,"privateReconnectPaused":false,
        }));
        self.emitter.frame(frame, store, &input, snapshot)
    }
}
