//! Account threads: engines are `!Send`, so each account lives on one OS
//! thread's current-thread runtime and `LocalSet`, reached through `Send` handles.
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::sync::mpsc;

type Job = Box<dyn FnOnce() + Send>;

struct Lane {
    jobs: mpsc::UnboundedSender<Job>,
    accounts: AtomicUsize,
}

#[derive(Clone)]
pub struct EngineThreads {
    lanes: Arc<Vec<Lane>>,
    panics: Arc<AtomicUsize>,
}

/// Which lane an account occupies; release it when the account task ends.
pub struct Placement {
    threads: EngineThreads,
    lane: usize,
}

impl Drop for Placement {
    fn drop(&mut self) {
        self.threads.lanes[self.lane]
            .accounts
            .fetch_sub(1, Ordering::Relaxed);
    }
}

impl EngineThreads {
    pub fn start(count: usize) -> std::io::Result<Self> {
        let mut lanes = Vec::with_capacity(count);
        let panics = Arc::new(AtomicUsize::new(0));
        for index in 0..count {
            let (jobs, mut inbox) = mpsc::unbounded_channel::<Job>();
            let job_panics = panics.clone();
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?;
            std::thread::Builder::new()
                .name(format!("engine-{index}"))
                .spawn(move || {
                    let local = tokio::task::LocalSet::new();
                    local.block_on(&runtime, async move {
                        while let Some(job) = inbox.recv().await {
                            // A panic here would end the lane; its accounts' tasks
                            // are caught by the runtime, so only a job can do that.
                            if std::panic::catch_unwind(std::panic::AssertUnwindSafe(job)).is_err()
                            {
                                job_panics.fetch_add(1, Ordering::Relaxed);
                                crate::state::Log::warn(&format!(
                                    "engine-{index}: an account start panicked"
                                ));
                            }
                        }
                    });
                })?;
            lanes.push(Lane {
                jobs,
                accounts: AtomicUsize::new(0),
            });
        }
        Ok(Self {
            lanes: Arc::new(lanes),
            panics,
        })
    }

    pub fn thread_count(&self) -> usize {
        self.lanes.len()
    }

    /// Runs `job` inside the least-loaded thread's `LocalSet`, so it may
    /// `spawn_local`. The placement counts toward that thread until dropped.
    pub fn place(&self, job: impl FnOnce(Placement) + Send + 'static) -> bool {
        let lane = (0..self.lanes.len())
            .min_by_key(|lane| self.lanes[*lane].accounts.load(Ordering::Relaxed))
            .expect("at least one engine thread");
        self.lanes[lane].accounts.fetch_add(1, Ordering::Relaxed);
        let placement = Placement {
            threads: self.clone(),
            lane,
        };
        self.lanes[lane]
            .jobs
            .send(Box::new(move || job(placement)))
            .is_ok()
    }

    /// Account starts that panicked since the process began.
    pub fn panics(&self) -> usize {
        self.panics.load(Ordering::Relaxed)
    }

    pub fn accounts(&self) -> usize {
        self.lanes
            .iter()
            .map(|lane| lane.accounts.load(Ordering::Relaxed))
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::router_lease::{Assertion, Config, Heartbeat, Permit, Users};
    use safe_cex_engine::{HttpRequest, Method, RequestId, request_context::RoutingIntent};
    use safe_cex_native::{HostEvent, NetworkTransport, Transport};
    use std::sync::Mutex;
    use std::time::Duration;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::sync::oneshot;

    fn grant_owner(users: &Users, epoch: u64) {
        let now = crate::accounts::now_ms();
        assert_eq!(
            users
                .accept(&Heartbeat {
                    r#box: "test-box".into(),
                    issued_at: now,
                    users: vec![Assertion {
                        user_id: "same-user".into(),
                        owner_epoch: epoch,
                        sequence: 1,
                        not_after: now + 45_000,
                    }],
                })
                .unwrap()
                .accepted,
            1
        );
    }

    fn queued_private_handoff(
        threads: &EngineThreads,
        permit: Permit,
        url: String,
    ) -> oneshot::Receiver<Result<(), String>> {
        let (finished, receive) = oneshot::channel();
        assert!(threads.place(move |placement| {
            let transport =
                NetworkTransport::new().with_io_guard(Arc::new(move || permit.allowed()));
            tokio::task::spawn_local(async move {
                let _placement = placement;
                let result = tokio::time::timeout(Duration::from_secs(10), async {
                    let request = HttpRequest {
                        id: RequestId(1),
                        method: Method::Post,
                        route: None,
                        timeout_ms: Some(5_000),
                        url,
                        headers: vec![],
                        signer_headers: vec![],
                        body: None,
                    };
                    let prepared = transport
                        .prepare_http(request.clone(), RoutingIntent::Primary)
                        .await
                        .map_err(|error| error.message().to_owned())?;
                    let bound = prepared
                        .bind(request)
                        .map_err(|error| error.message().to_owned())?;
                    let (events, mut replies) = mpsc::unbounded_channel();
                    bound.dispatch(events);
                    while let Some(event) = replies.recv().await {
                        match event {
                            HostEvent::HttpEnd(_) => return Ok(()),
                            HostEvent::HttpFailed(_, error) => return Err(error),
                            _ => {}
                        }
                    }
                    Err("private request ended without an outcome".into())
                })
                .await
                .expect("private request completes within its bound");
                let _ = finished.send(result);
            });
        }));
        receive
    }

    #[tokio::test]
    async fn queued_account_job_keeps_its_captured_owner_generation_at_private_handoff() {
        for replace_owner in [false, true] {
            let users = Users::new(Config {
                box_id: Some("test-box".into()),
                enforce: true,
                ..Default::default()
            });
            grant_owner(&users, 1);
            let user = users.get("same-user").unwrap();
            let owner_a = user.permit();
            let generation_a = user.status().generation;
            assert!(
                owner_a.allowed(),
                "A was authorized when its job was created"
            );

            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let base_url = format!("http://{}", listener.local_addr().unwrap());
            let requests = Arc::new(Mutex::new(Vec::new()));
            let received = requests.clone();
            let server = tokio::spawn(async move {
                loop {
                    let (mut socket, _) = listener.accept().await.unwrap();
                    let mut headers = Vec::new();
                    while !headers.ends_with(b"\r\n\r\n") {
                        headers.push(socket.read_u8().await.unwrap());
                    }
                    let path = std::str::from_utf8(&headers)
                        .unwrap()
                        .split_whitespace()
                        .nth(1)
                        .unwrap()
                        .to_owned();
                    received.lock().unwrap().push(path);
                    socket
                        .write_all(
                            b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok",
                        )
                        .await
                        .unwrap();
                }
            });

            let threads = EngineThreads::start(1).unwrap();
            let (release, blocked) = std::sync::mpsc::channel();
            let (entered, started) = oneshot::channel();
            assert!(threads.place(move |_| {
                entered.send(()).unwrap();
                blocked.recv_timeout(Duration::from_secs(10)).unwrap();
            }));
            tokio::time::timeout(Duration::from_secs(10), started)
                .await
                .unwrap()
                .unwrap();
            let mut a_done = queued_private_handoff(
                &threads,
                owner_a.clone(),
                format!("{base_url}/private/owner-A"),
            );
            assert_eq!(threads.accounts(), 2, "A really waits behind the held lane");
            assert!(matches!(
                a_done.try_recv(),
                Err(oneshot::error::TryRecvError::Empty)
            ));
            assert!(requests.lock().unwrap().is_empty());

            if replace_owner {
                grant_owner(&users, 2);
                assert_eq!(user.status().generation, generation_a + 1);
                assert!(!owner_a.allowed(), "the replacement retires A's authority");
            }
            let owner_b = user.permit();
            assert!(
                owner_b.allowed(),
                "a separately admitted B control is authorized"
            );
            let b_done =
                queued_private_handoff(&threads, owner_b, format!("{base_url}/private/owner-B"));
            assert_eq!(threads.accounts(), 3, "both jobs are queued before release");
            release.send(()).unwrap();

            let a_result = tokio::time::timeout(Duration::from_secs(10), a_done)
                .await
                .unwrap()
                .unwrap();
            let b_result = tokio::time::timeout(Duration::from_secs(10), b_done)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(b_result, Ok(()), "fresh B reaches the private handoff");
            if replace_owner {
                assert!(
                    a_result
                        .unwrap_err()
                        .contains("account IO authority expired")
                );
                assert_eq!(*requests.lock().unwrap(), vec!["/private/owner-B"]);
                assert!(
                    !owner_a.allowed(),
                    "executing A must not adopt B's generation"
                );
            } else {
                assert_eq!(a_result, Ok(()), "unchanged-owner control still sends A");
                let mut paths = requests.lock().unwrap().clone();
                paths.sort();
                assert_eq!(paths, vec!["/private/owner-A", "/private/owner-B"]);
            }
            server.abort();
            assert!(server.await.unwrap_err().is_cancelled());
            tokio::time::timeout(Duration::from_secs(10), async {
                while threads.accounts() != 0 {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .expect("all queued placements are released");
            assert_eq!(threads.panics(), 0);
        }
    }

    #[tokio::test]
    async fn a_panicking_account_start_leaves_its_lane_working_and_is_counted() {
        let threads = EngineThreads::start(1).unwrap();
        assert!(threads.place(|_| panic!("start failed")));
        let (done, finished) = tokio::sync::oneshot::channel();
        assert!(threads.place(move |_| {
            let _ = done.send(());
        }));
        tokio::time::timeout(std::time::Duration::from_secs(5), finished)
            .await
            .expect("the lane still runs jobs")
            .unwrap();
        assert_eq!(threads.panics(), 1);
        // The second job's placement drops only after its send, on the lane's thread.
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            while threads.accounts() != 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("both placements released");
    }
}
