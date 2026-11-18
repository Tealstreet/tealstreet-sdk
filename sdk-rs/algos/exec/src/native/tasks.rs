use super::Adapter;
use crate::AlgoReply;
struct Task {
    id: String,
    kind: &'static str,
    symbol: String,
    side: String,
    active: bool,
    progress: String,
    status: String,
}
impl Adapter {
    pub(super) fn task_control(&self, target: Option<&str>) -> Result<AlgoReply, String> {
        let mut tasks = Vec::new();
        if let Some(book) = &self.chasers {
            for c in book.active_chasers() {
                let remaining = book
                    .progress(&c.id)
                    .map(|p| p.remaining_size)
                    .unwrap_or(c.size);
                tasks.push(Task {
                    id: c.id,
                    kind: "chase",
                    symbol: c.symbol,
                    side: format!("{:?}", c.side).to_uppercase(),
                    active: !c.is_stopped,
                    progress: format!(
                        "{:.1}% filled",
                        if c.size > 0. {
                            (c.size - remaining) / c.size * 100.
                        } else {
                            0.
                        }
                    ),
                    status: if c.is_stopped {
                        c.stop_reason
                    } else {
                        "active".into()
                    },
                });
            }
        }
        if let Some(twap) = &self.twap {
            for row in twap.active_twaps(self.account_name.as_deref().unwrap_or("")) {
                let text = |key: &str| row[key].as_str().unwrap_or("").to_string();
                let executed = row["ordersExecuted"].as_f64().unwrap_or(0.);
                let count = row["ordersCount"].as_f64().unwrap_or(0.);
                tasks.push(Task {
                    id: text("id"),
                    kind: "twap",
                    symbol: text("symbol"),
                    side: text("side").to_uppercase(),
                    active: row["status"] != "completed",
                    progress: format!(
                        "{executed}/{count} slices ({:.1}%)",
                        if count > 0. {
                            executed / count * 100.
                        } else {
                            0.
                        }
                    ),
                    status: text("status"),
                });
            }
        }
        let message = if let Some(target) = target {
            let active = tasks.iter().filter(|t| t.active).collect::<Vec<_>>();
            let kill = |task: &Task| {
                if task.kind == "chase" {
                    self.chasers.as_ref().unwrap().remove(&task.id);
                    true
                } else {
                    self.twap.as_ref().unwrap().stop(&task.id)
                }
            };
            if target == "all" {
                if active.is_empty() {
                    "(no active tasks to kill)".into()
                } else {
                    format!(
                        "Killed {} task(s)",
                        active.iter().filter(|t| kill(t)).count()
                    )
                }
            } else {
                let task = if target.is_empty() || target.eq_ignore_ascii_case("last") {
                    active.last().copied().ok_or("No active tasks to kill")?
                } else {
                    active
                        .iter()
                        .copied()
                        .find(|t| t.id == target || t.id.starts_with(target))
                        .ok_or_else(|| format!("No active task matches \"{target}\""))?
                };
                if !kill(task) {
                    return Err(format!(
                        "Failed to kill {} {}",
                        task.kind,
                        &task.id[..task.id.len().min(8)]
                    ));
                }
                format!(
                    "Killed {} [{}] ({})",
                    task.kind,
                    &task.id[..task.id.len().min(8)],
                    task.symbol
                )
            }
        } else if tasks.is_empty() {
            "(no active tasks)".into()
        } else {
            let active = tasks.iter().filter(|t| t.active).count();
            let mut lines = tasks
                .iter()
                .map(|t| {
                    format!(
                        "[{}] {:5} {} {}  {}  {}",
                        &t.id[..t.id.len().min(8)],
                        t.kind,
                        t.symbol,
                        t.side,
                        t.progress,
                        if t.active {
                            t.status.clone()
                        } else {
                            format!("[{}]", t.status)
                        }
                    )
                })
                .collect::<Vec<_>>();
            lines.push(format!(
                "(active: {active}  completed: {})",
                tasks.len() - active
            ));
            lines.join("\n")
        };
        Ok(AlgoReply {
            message,
            ..Default::default()
        })
    }
}
