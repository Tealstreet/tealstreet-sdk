//! Classification source: packages/safe-cex/src/health/errorPatterns.ts.
use crate::runtime::{ExchangeRuntime, ExchangeTask};
use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::Rc,
    sync::LazyLock,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCategory {
    AuthInvalid,
    AuthExpired,
    AuthPermissions,
    RateLimit,
    Network,
    ExchangeDown,
    TimeSync,
    OrderRejected,
    OrderWarning,
    UnsupportedBackend,
    Suppressed,
    Unknown,
}
impl ErrorCategory {
    fn label(self) -> String {
        serde_json::to_value(self)
            .expect("category")
            .as_str()
            .expect("string")
            .into()
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ErrorSeverity {
    Critical,
    Warning,
    Info,
}
#[derive(Clone)]
pub enum ErrorMatcher {
    Text(String),
    Regex(Regex),
}
#[derive(Clone)]
pub struct ErrorPattern {
    pub category: ErrorCategory,
    pub patterns: Vec<ErrorMatcher>,
    pub severity: ErrorSeverity,
}
#[derive(Deserialize)]
struct PatternData {
    category: ErrorCategory,
    severity: ErrorSeverity,
    patterns: Vec<MatcherData>,
}
#[derive(Deserialize)]
struct MatcherData {
    text: Option<String>,
    regex: Option<String>,
    flags: Option<String>,
}
#[derive(Deserialize)]
struct PatternFile {
    defaults: Vec<PatternData>,
    exchanges: BTreeMap<String, Vec<PatternData>>,
}
fn compile(data: Vec<PatternData>) -> Vec<ErrorPattern> {
    data.into_iter()
        .map(|p| ErrorPattern {
            category: p.category,
            severity: p.severity,
            patterns: p
                .patterns
                .into_iter()
                .map(|m| {
                    if let Some(text) = m.text {
                        ErrorMatcher::Text(text)
                    } else {
                        ErrorMatcher::Regex(
                            RegexBuilder::new(&js_regex_source(
                                &m.regex.expect("regex"),
                                m.flags.unwrap_or_default().contains('i'),
                            ))
                            .build()
                            .expect("generated regex"),
                        )
                    }
                })
                .collect(),
        })
        .collect()
}
pub static DEFAULT_ERROR_PATTERNS: LazyLock<Vec<ErrorPattern>> = LazyLock::new(|| {
    compile(
        serde_json::from_str::<PatternFile>(include_str!("health_patterns.json"))
            .expect("patterns")
            .defaults,
    )
});
pub static EXCHANGE_SPECIFIC_PATTERNS: LazyLock<BTreeMap<String, Vec<ErrorPattern>>> =
    LazyLock::new(|| {
        serde_json::from_str::<PatternFile>(include_str!("health_patterns.json"))
            .expect("patterns")
            .exchanges
            .into_iter()
            .map(|(k, v)| (k, compile(v)))
            .collect()
    });
// ts-bug: mergePatterns keeps the first severity, despite its priority comment.
pub fn merge_patterns(lists: &[&[ErrorPattern]]) -> Vec<ErrorPattern> {
    let mut merged: Vec<ErrorPattern> = vec![];
    for list in lists {
        for p in *list {
            if let Some(existing) = merged.iter_mut().find(|x| x.category == p.category) {
                existing.patterns.extend(p.patterns.clone());
            } else {
                merged.push(p.clone());
            }
        }
    }
    merged
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Error,
}
#[derive(Debug, Clone, Serialize)]
pub struct ErrorEntry {
    pub category: ErrorCategory,
    pub message: String,
    pub timestamp: f64,
    pub severity: ErrorSeverity,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthState {
    pub status: HealthStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<ErrorEntry>,
    pub recent_errors: Vec<ErrorEntry>,
    #[serde(serialize_with = "serialize_counts")]
    pub error_counts: Vec<(ErrorCategory, u64)>,
    pub last_updated: f64,
}
#[derive(Default)]
pub struct ExchangeHealthMonitorOptions {
    pub max_recent_errors: Option<usize>,
    pub custom_patterns: Vec<ErrorPattern>,
    pub recovery_window_ms: Option<u64>,
}
struct State {
    health: HealthState,
    timer: Option<Box<dyn ExchangeTask>>,
    generation: u64,
    recovery_deadline: Option<f64>,
    recovery_orphaned: bool,
}
// Only abandoned tasks use lazy recovery; live timers retain host scheduling.
struct RecoveryGuard {
    state: std::rc::Weak<RefCell<State>>,
    generation: u64,
    completed: Rc<Cell<bool>>,
}
impl RecoveryGuard {
    fn finish(&mut self, now: Option<f64>) {
        if self.completed.replace(true) {
            return;
        }
        let Some(state) = self.state.upgrade() else {
            return;
        };
        let task = {
            let mut state = state.borrow_mut();
            if state.generation != self.generation {
                return;
            }
            if let Some(now) = now {
                if state.health.status == HealthStatus::Degraded {
                    state.health.status = HealthStatus::Healthy;
                    state.health.last_updated = now;
                }
                state.recovery_deadline = None;
                state.recovery_orphaned = false;
            } else {
                state.recovery_orphaned = true;
            }
            state.timer.take()
        };
        drop(task);
    }
}
impl Drop for RecoveryGuard {
    fn drop(&mut self) {
        self.finish(None);
    }
}
pub struct ExchangeHealthMonitor {
    state: Rc<RefCell<State>>,
    patterns: Vec<ErrorPattern>,
    max_recent_errors: usize,
    recovery_window_ms: u64,
    runtime: Rc<dyn ExchangeRuntime>,
}
impl ExchangeHealthMonitor {
    pub fn new(
        exchange: &str,
        options: ExchangeHealthMonitorOptions,
        runtime: Rc<dyn ExchangeRuntime>,
    ) -> Self {
        let exchange_patterns = EXCHANGE_SPECIFIC_PATTERNS
            .get(&exchange.to_lowercase())
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        Self {
            state: Rc::new(RefCell::new(State {
                health: HealthState {
                    status: HealthStatus::Healthy,
                    last_error: None,
                    recent_errors: vec![],
                    error_counts: vec![],
                    last_updated: runtime.now_ms(),
                },
                timer: None,
                generation: 0,
                recovery_deadline: None,
                recovery_orphaned: false,
            })),
            patterns: merge_patterns(&[
                &DEFAULT_ERROR_PATTERNS,
                exchange_patterns,
                &options.custom_patterns,
            ]),
            max_recent_errors: options.max_recent_errors.unwrap_or(10),
            recovery_window_ms: options.recovery_window_ms.unwrap_or(60000),
            runtime,
        }
    }
    pub fn process_message(&self, message: &str, is_error: bool) -> Option<ErrorCategory> {
        if message.is_empty() {
            return None;
        }
        let lower = message.to_lowercase();
        let hit = self
            .patterns
            .iter()
            .find(|p| {
                p.patterns.iter().any(|m| match m {
                    ErrorMatcher::Text(t) => lower.contains(&t.to_lowercase()),
                    ErrorMatcher::Regex(r) => r.is_match(message),
                })
            })
            .map(|p| (p.category, p.severity))
            .or_else(|| is_error.then_some((ErrorCategory::Unknown, ErrorSeverity::Warning)));
        if let Some((category, severity)) = hit {
            self.record_error(category, message, severity);
        }
        hit.map(|(c, _)| c)
    }
    fn record_error(&self, category: ErrorCategory, message: &str, severity: ErrorSeverity) {
        self.reconcile_orphaned_recovery();
        let now = self.runtime.now_ms();
        let entry = ErrorEntry {
            category,
            message: message.into(),
            timestamp: now,
            severity,
        };
        let mut state = self.state.borrow_mut();
        state.health.recent_errors.push(entry.clone());
        if state.health.recent_errors.len() > self.max_recent_errors {
            state.health.recent_errors.remove(0);
        }
        state.health.last_error = Some(entry);
        state.health.last_updated = self.runtime.now_ms();
        if let Some((_, count)) = state
            .health
            .error_counts
            .iter_mut()
            .find(|(c, _)| *c == category)
        {
            *count += 1;
        } else {
            state.health.error_counts.push((category, 1));
        }
        let recover =
            severity == ErrorSeverity::Warning && state.health.status == HealthStatus::Healthy;
        if severity == ErrorSeverity::Critical {
            state.health.status = HealthStatus::Error;
        } else if recover {
            state.health.status = HealthStatus::Degraded;
        }
        drop(state);
        if severity == ErrorSeverity::Critical {
            self.clear_recovery_timer();
        } else if recover {
            self.schedule_recovery();
        }
    }
    fn schedule_recovery(&self) {
        self.clear_recovery_timer();
        let generation = {
            let mut state = self.state.borrow_mut();
            state.recovery_deadline = Some(self.runtime.now_ms() + self.recovery_window_ms as f64);
            state.generation
        };
        let completed = Rc::new(Cell::new(false));
        let mut guard = RecoveryGuard {
            state: Rc::downgrade(&self.state),
            generation,
            completed: completed.clone(),
        };
        let runtime = self.runtime.clone();
        let sleep = runtime.sleep(self.recovery_window_ms);
        let task = self.runtime.spawn_classified(
            crate::runtime::RuntimeTaskKind::Timer,
            Box::pin(async move {
                let now = sleep.await.is_ok().then(|| runtime.now_ms());
                guard.finish(now);
            }),
        );
        let mut state = self.state.borrow_mut();
        if state.generation == generation && !completed.get() {
            state.timer = Some(task);
        } else {
            drop(state);
            task.cancel();
        }
    }
    fn reconcile_orphaned_recovery(&self) {
        let mut state = self.state.borrow_mut();
        if state.recovery_orphaned
            && state
                .recovery_deadline
                .is_some_and(|at| self.runtime.now_ms() >= at)
        {
            state.recovery_orphaned = false;
            state.recovery_deadline = None;
            if state.health.status == HealthStatus::Degraded {
                state.health.status = HealthStatus::Healthy;
                state.health.last_updated = self.runtime.now_ms();
            }
        }
    }
    fn clear_recovery_timer(&self) {
        let mut state = self.state.borrow_mut();
        state.generation += 1;
        state.recovery_deadline = None;
        state.recovery_orphaned = false;
        let task = state.timer.take();
        drop(state);
        if let Some(task) = task {
            task.cancel();
        }
    }
    pub fn get_state(&self) -> HealthState {
        self.reconcile_orphaned_recovery();
        self.state.borrow().health.clone()
    }
    pub fn is_healthy(&self) -> bool {
        self.reconcile_orphaned_recovery();
        self.state.borrow().health.status == HealthStatus::Healthy
    }
    pub fn has_auth_error(&self) -> bool {
        [
            ErrorCategory::AuthInvalid,
            ErrorCategory::AuthExpired,
            ErrorCategory::AuthPermissions,
        ]
        .into_iter()
        .any(|c| self.has_error(c))
    }
    pub fn has_error(&self, category: ErrorCategory) -> bool {
        self.get_error_count(category) > 0
    }
    pub fn get_error_count(&self, category: ErrorCategory) -> u64 {
        self.state
            .borrow()
            .health
            .error_counts
            .iter()
            .find(|(c, _)| *c == category)
            .map(|(_, n)| *n)
            .unwrap_or(0)
    }
    pub fn get_last_error(&self) -> Option<ErrorEntry> {
        self.state.borrow().health.last_error.clone()
    }
    pub fn get_errors_by_category(&self, category: ErrorCategory) -> Vec<ErrorEntry> {
        self.state
            .borrow()
            .health
            .recent_errors
            .iter()
            .filter(|e| e.category == category)
            .cloned()
            .collect()
    }
    pub fn reset(&self) {
        self.clear_recovery_timer();
        self.state.borrow_mut().health = HealthState {
            status: HealthStatus::Healthy,
            last_error: None,
            recent_errors: vec![],
            error_counts: vec![],
            last_updated: self.runtime.now_ms(),
        };
    }
    pub fn set_status(&self, status: HealthStatus) {
        let mut state = self.state.borrow_mut();
        state.health.status = status;
        state.health.last_updated = self.runtime.now_ms();
        drop(state);
        if status == HealthStatus::Degraded {
            self.schedule_recovery();
        }
    }
    pub fn dispose(&self) {
        self.clear_recovery_timer();
    }
    pub fn get_summary(&self) -> String {
        self.reconcile_orphaned_recovery();
        let state = self.state.borrow();
        let health = &state.health;
        if health.status == HealthStatus::Healthy {
            return "OK".into();
        }
        let status = if health.status == HealthStatus::Error {
            "ERROR"
        } else {
            "DEGRADED"
        };
        if let Some(error) = &health.last_error {
            let units: Vec<_> = error.message.encode_utf16().collect();
            format!(
                "{status} - {}: {}{}",
                error.category.label(),
                String::from_utf16_lossy(&units[..units.len().min(50)]),
                if units.len() > 50 { "..." } else { "" }
            )
        } else {
            format!(
                "{status} - {}",
                health
                    .error_counts
                    .iter()
                    .filter(|(_, n)| *n > 0)
                    .map(|(c, n)| format!("{}: {n}", c.label()))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        }
    }
}

fn serialize_counts<S: serde::Serializer>(
    counts: &[(ErrorCategory, u64)],
    serializer: S,
) -> Result<S::Ok, S::Error> {
    use serde::ser::SerializeMap;
    let mut map = serializer.serialize_map(Some(counts.len()))?;
    for (category, count) in counts {
        map.serialize_entry(&category.label(), count)?;
    }
    map.end()
}

// JS regexes without u fold ASCII only; Rust Unicode folding can misclassify auth.
fn js_regex_source(source: &str, insensitive: bool) -> String {
    let mut output = String::new();
    let mut characters = source.chars().peekable();
    let mut in_class = false;
    while let Some(character) = characters.next() {
        if character == '\\' {
            let escaped = characters.next().expect("valid generated regex escape");
            match escaped {
                'b' if !in_class => output.push_str("(?-u:\\b)"),
                'd' => {
                    if in_class {
                        output.push_str("0-9")
                    } else {
                        output.push_str("[0-9]")
                    }
                }
                'w' => {
                    if in_class {
                        output.push_str("A-Za-z0-9_")
                    } else {
                        output.push_str("[A-Za-z0-9_]")
                    }
                }
                's' => {
                    if !in_class {
                        output.push('[')
                    }
                    output.push_str("\\t\\n\\v\\f\\r \\u{00a0}\\u{1680}\\u{2000}-\\u{200a}\\u{2028}\\u{2029}\\u{202f}\\u{205f}\\u{3000}\\u{feff}");
                    if !in_class {
                        output.push(']')
                    }
                }
                _ => {
                    output.push('\\');
                    output.push(escaped);
                }
            }
        } else if character == '[' {
            in_class = true;
            output.push(character);
        } else if character == ']' {
            in_class = false;
            output.push(character);
        } else if character == '.' && !in_class {
            output.push_str("[^\\n\\r\\u{2028}\\u{2029}]");
        } else if insensitive && character.is_ascii_alphabetic() {
            assert!(
                !in_class || characters.peek() != Some(&'-'),
                "new JS character-class range needs a faithful converter"
            );
            if !in_class {
                output.push('[')
            }
            output.push(character.to_ascii_lowercase());
            output.push(character.to_ascii_uppercase());
            if !in_class {
                output.push(']')
            }
        } else {
            output.push(character)
        }
    }
    output
}
