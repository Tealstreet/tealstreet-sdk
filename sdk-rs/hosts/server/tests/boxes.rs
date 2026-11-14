//! Two boxes in one process: two servers on two localhost ports sharing nothing,
//! each on its own thread and runtime so one can be killed outright. Accounts
//! are test accounts on a test venue (no venue I/O); pages are real WebSockets
//! with unsigned proxy tokens. What a single box can and cannot promise about
//! one account on two boxes is pinned in `a_reconnect_race_*` below.
mod support;

use safe_cex_exchange_server::accounts::testing::{self, Held};
use safe_cex_exchange_server::auth::{Verifier, unsigned_token};
use safe_cex_exchange_server::envelope::InitPayload;
use safe_cex_exchange_server::http::App;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::sync::{Arc, Weak};
use std::time::Duration;
use support::{Ending, Page, health, post};

type Task = Box<dyn FnOnce(&Arc<App>) + Send>;

/// One box: a server and everything it runs, on a thread of its own.
struct TestBox {
    address: String,
    tasks: tokio::sync::mpsc::UnboundedSender<Task>,
    kill: Option<tokio::sync::oneshot::Sender<()>>,
    thread: Option<std::thread::JoinHandle<Gone>>,
}

/// What a killed box left behind: nothing may still hold its state.
struct Gone {
    app: Weak<App>,
    accounts: Vec<Weak<safe_cex_exchange_server::accounts::Account>>,
}

impl TestBox {
    fn start(resume_grace: Duration) -> Self {
        let (ready, address) = std::sync::mpsc::channel();
        let (tasks, mut inbox) = tokio::sync::mpsc::unbounded_channel::<Task>();
        let (kill, killed) = tokio::sync::oneshot::channel::<()>();
        let thread = std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            let local = tokio::task::LocalSet::new();
            let gone = local.block_on(&runtime, async move {
                let app = support::app(support::config(resume_grace), Verifier::unsigned());
                ready.send(support::serve(app.clone()).await).unwrap();
                tokio::pin!(killed);
                loop {
                    tokio::select! {
                        _ = &mut killed => break,
                        Some(task) = inbox.recv() => task(&app),
                    }
                }
                let accounts = testing::accounts(&app.registry)
                    .iter()
                    .map(Arc::downgrade)
                    .collect();
                Gone {
                    app: Arc::downgrade(&app),
                    accounts,
                }
            });
            // The process dies: every socket, session, algo host and engine with it.
            drop(local);
            drop(runtime);
            gone
        });
        Self {
            address: address.recv().unwrap(),
            tasks,
            kill: Some(kill),
            thread: Some(thread),
        }
    }

    /// Runs `task` on the box's own thread, inside its `LocalSet`.
    async fn on_box<T: Send + 'static>(
        &self,
        task: impl FnOnce(&Arc<App>) -> T + Send + 'static,
    ) -> T {
        let (done, result) = tokio::sync::oneshot::channel();
        let task: Task = Box::new(move |app| {
            let _ = done.send(task(app));
        });
        assert!(self.tasks.send(task).is_ok(), "the box is gone");
        result.await.unwrap()
    }

    /// `user`'s account for `init`, as the box would build it from the venue.
    async fn account(&self, user: &str, init: &InitPayload) {
        let (user, init) = (user.to_owned(), init.clone());
        self.on_box(move |app| {
            testing::trading_account(&app.registry, &user, &init);
        })
        .await;
    }

    async fn holdings(&self) -> BTreeMap<(String, String), Held> {
        self.on_box(|app| testing::holdings(&app.registry)).await
    }

    /// The box is killed mid-flight: no drain, no close frames, nobody told.
    fn kill(&mut self) -> Gone {
        let _ = self.kill.take().unwrap().send(());
        self.thread.take().unwrap().join().unwrap()
    }
}

impl Drop for TestBox {
    fn drop(&mut self) {
        if let Some(kill) = self.kill.take() {
            let _ = kill.send(());
        }
    }
}

/// Invariant 1 across boxes: no (user, account) held or parked on two at once.
/// Returns the accounts on more than one box.
async fn on_two_boxes(boxes: &[&TestBox]) -> Vec<(String, String)> {
    let mut seen: BTreeMap<(String, String), usize> = BTreeMap::new();
    for one in boxes {
        for key in one.holdings().await.into_keys() {
            *seen.entry(key).or_default() += 1;
        }
    }
    seen.into_iter()
        .filter(|(_, boxes)| *boxes > 1)
        .map(|(key, _)| key)
        .collect()
}

async fn assert_one_box(boxes: &[&TestBox]) {
    let both = on_two_boxes(boxes).await;
    assert!(both.is_empty(), "held or parked on two boxes: {both:?}");
}

fn sole_identity(holdings: &BTreeMap<(String, String), Held>, user: &str) -> (String, String) {
    assert_eq!(holdings.len(), 1, "one identity on this box");
    let key = holdings.keys().next().unwrap();
    assert_eq!(key.0, user);
    assert_eq!(key.1.len(), 64);
    assert!(
        key.1
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    );
    key.clone()
}

fn init() -> InitPayload {
    InitPayload {
        account_name: "acct".into(),
        exchange_name: "bybit".into(),
        key: "k".into(),
        ..InitPayload::default()
    }
}

fn token(user: &str) -> String {
    let in_an_hour = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 3600;
    unsigned_token(user, in_an_hour)
}

/// `chaseOrder.ts`: the order form's chase.
fn web_chase() -> Value {
    json!({"symbol":"BTCUSDT","size":0.01,"side":"buy","maxAttempts":1000,
        "reduceOnly":false,"delay":500,"origin":"web"})
}

fn chases(store: &Value) -> usize {
    store["data"]["activeChasers"]
        .as_array()
        .map_or(0, |rows| rows.len())
}

/// `user`'s page on `on`, joined to its account.
async fn joined_page(on: &TestBox, user: &str) -> Page {
    on.account(user, &init()).await;
    let mut page = Page::open(&on.address, &token(user), None).await;
    page.init(&init()).await;
    page.until("frame", |d| d["snapshot"] == true).await;
    page
}

/// `user`'s page on `on` with a chase running, and the chase's id.
async fn chasing_page(on: &TestBox, user: &str) -> (Page, Value) {
    let mut page = joined_page(on, user).await;
    page.op("acct", "chaser:start", web_chase()).await;
    let store = page.until("store", |d| chases(d) == 1).await;
    let id = store["data"]["activeChasers"][0]["id"].clone();
    (page, id)
}

async fn algos(address: &str) -> Value {
    health(address).await.1["algos"].clone()
}

/// `/health`'s algo counts, once the box has recounted (every 250ms).
async fn algos_become(address: &str, chases: usize, twaps: usize) {
    let expected = json!({"chases": chases, "twaps": twaps});
    for _ in 0..40 {
        if algos(address).await == expected {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(algos(address).await, expected);
}

/// (a) A drain moves the idle user at once and the chasing user only once the
/// chase ends, never stopping it; /health says draining and counts down to 0.
#[tokio::test]
async fn a_drain_moves_the_idle_user_at_once_and_the_chasing_user_after_the_chase() {
    let (a, b) = (
        TestBox::start(Duration::from_secs(60)),
        TestBox::start(Duration::from_secs(60)),
    );
    let (mut chasing, chase) = chasing_page(&a, "u1").await;
    let mut idle = joined_page(&a, "u2").await;
    algos_become(&a.address, 1, 0).await;
    assert_one_box(&[&a, &b]).await;

    let (status, body) = post(&a.address, "/admin/drain", "").await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        serde_json::from_str::<Value>(&body).unwrap(),
        json!({"drain": "waiting", "algos": {"chases": 1, "twaps": 0}})
    );
    assert_eq!(
        idle.ending(Duration::from_secs(2)).await,
        Ending::Closed(4100, "draining: reconnect".into())
    );
    assert_eq!(
        chasing.ending(Duration::from_secs(3)).await,
        Ending::StillOpen,
        "kept while its chase runs"
    );
    let (status, report) = health(&a.address).await;
    assert_eq!(status, 503);
    assert_eq!(report["draining"], true);
    assert_eq!(
        report["algos"],
        json!({"chases": 1, "twaps": 0}),
        "the drain stops nothing"
    );
    assert!(
        chasing.ops("error").is_empty() && chasing.ops("chaser:complete").is_empty(),
        "no stop notice, no end: {:?}",
        chasing.seen
    );
    assert_one_box(&[&a, &b]).await;

    // The user ends the chase from its Active Chasers row; only then does the page move.
    chasing.op("acct", "chaser:terminate", chase).await;
    let end = chasing.until("chaser:complete", |_| true).await;
    assert_eq!(
        end["data"]["stopReason"], "User cancelled",
        "ended by the user: {end}"
    );
    assert_eq!(
        chasing.ending(Duration::from_secs(5)).await,
        Ending::Closed(4100, "draining: reconnect".into())
    );
    algos_become(&a.address, 0, 0).await;
    // A moved socket releases its accounts: its page reconnects elsewhere.
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(a.holdings().await.is_empty(), "{:?}", a.holdings().await);
    assert_one_box(&[&a, &b]).await;
}

/// (b) Box A dies mid-chase. The page reconnects fresh to B, which reports no
/// chase for the user (the page's backstop must fire); A left nothing behind.
#[tokio::test]
async fn a_box_killed_mid_chase_leaves_nothing_and_the_other_box_reports_no_chase() {
    let (mut a, b) = (
        TestBox::start(Duration::from_secs(60)),
        TestBox::start(Duration::from_secs(60)),
    );
    let (mut page, _) = chasing_page(&a, "u1").await;
    algos_become(&a.address, 1, 0).await;
    assert_one_box(&[&a, &b]).await;

    let gone = a.kill();
    assert_eq!(
        page.ending(Duration::from_secs(5)).await,
        Ending::Dropped,
        "no close frame from a dead box"
    );
    assert!(
        page.ops("error").is_empty(),
        "nobody is told: {:?}",
        page.ops("error")
    );
    assert_eq!(gone.accounts.len(), 1);
    assert!(
        gone.app.upgrade().is_none() && gone.accounts.iter().all(|a| a.upgrade().is_none()),
        "a killed box holds nothing: no task kept its accounts or algo host alive"
    );

    // The page's fresh init on B (no resume: B never issued A's id).
    let again = joined_page(&b, "u1").await;
    let rows = again.ops("store");
    let rows: Vec<&&Value> = rows
        .iter()
        .filter(|d| d["data"].get("activeChasers").is_some())
        .collect();
    assert!(
        !rows.is_empty() && rows.iter().all(|d| chases(d) == 0),
        "B tells the page it runs no chase: {rows:?}"
    );
    assert_eq!(algos(&b.address).await, json!({"chases": 0, "twaps": 0}));
    let held = b.holdings().await;
    let mine = &held[&sole_identity(&held, "u1")];
    assert_eq!((mine.chases, mine.twaps), (0, 0));
    assert_eq!(mine.signing, mine.live.first().copied());
    assert_one_box(&[&b]).await;
}

/// (c) The reconnect race: A parks the account when the page's socket closes, and
/// the same user's next socket lands on B before A's grace ends. One box alone
/// cannot see the other, so for that window the account is on both: parked on A,
/// live on B. Keeping a user's live sockets on one box is the routing Worker's
/// job. What the server does guarantee: the parked copy has no browser, so no
/// socket signs for it and its signer link is suspended (a sign it asks for waits
/// out `SIGN_TIMEOUT` and fails; this test venue never signs, so that leg is the
/// remote signer's own tests), and grace expiry ends it, after which A holds nothing.
#[tokio::test]
async fn a_reconnect_race_parks_on_one_box_while_live_on_the_other_and_the_parked_copy_cannot_sign()
{
    let grace = Duration::from_secs(2);
    let (a, b) = (TestBox::start(grace), TestBox::start(grace));
    let (page, _) = chasing_page(&a, "u1").await;
    let key = sole_identity(&a.holdings().await, "u1");
    page.close().await;
    // The chase count is the algo host's 250ms recount, so wait for it rather than a fixed sleep.
    let mut on_a = None;
    for _ in 0..40 {
        on_a = a.holdings().await.remove(&key);
        if on_a
            .as_ref()
            .is_some_and(|held| held.parked && held.chases == 1)
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let on_a = on_a.expect("parked on A");
    assert_eq!(
        on_a,
        Held {
            live: Vec::new(),
            parked: true,
            signing: None,
            can_sign: false,
            chases: 1,
            twaps: 0,
        },
        "parked with no browser: its chase runs on but nothing can sign for it"
    );

    let _again = joined_page(&b, "u1").await;
    let on_b = b.holdings().await.remove(&key).expect("live on B");
    assert!(
        on_b.can_sign && on_b.signing.is_some() && !on_b.parked,
        "{on_b:?}"
    );
    assert_eq!(
        on_two_boxes(&[&a, &b]).await,
        std::slice::from_ref(&key),
        "the overlap only routing prevents"
    );

    // Grace expiry ends the parked copy; the invariant holds again.
    tokio::time::sleep(grace + Duration::from_secs(2)).await;
    assert!(a.holdings().await.is_empty(), "{:?}", a.holdings().await);
    algos_become(&a.address, 0, 0).await;
    assert_one_box(&[&a, &b]).await;
}
