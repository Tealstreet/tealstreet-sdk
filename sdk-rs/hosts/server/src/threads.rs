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
        for index in 0..count {
            let (jobs, mut inbox) = mpsc::unbounded_channel::<Job>();
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?;
            std::thread::Builder::new()
                .name(format!("engine-{index}"))
                .spawn(move || {
                    let local = tokio::task::LocalSet::new();
                    local.block_on(&runtime, async move {
                        while let Some(job) = inbox.recv().await {
                            job();
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
        })
    }

    pub fn len(&self) -> usize {
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

    pub fn accounts(&self) -> usize {
        self.lanes
            .iter()
            .map(|lane| lane.accounts.load(Ordering::Relaxed))
            .sum()
    }
}
