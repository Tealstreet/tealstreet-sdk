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
        assert_eq!(threads.accounts(), 0);
    }
}
