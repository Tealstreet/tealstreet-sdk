//! Full-equality lookup only; execution handles retain all observer authority.
use super::*;
use crate::CredentialSlot;
use crate::engine::ExecutionHandle;
use crate::request_context::{Operation, RetryPolicy, RoutingIntent, plan::RetryWindow};
use crate::signing::Identity;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CredentialSubject {
    ConfiguredPrimary(Identity),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeWork {
    pub kind: &'static str,
    pub endpoints: &'static [&'static str],
    pub step_waits_ms: &'static [u32],
    pub calls_per_step: &'static [usize],
    pub root_timeout_ms: u32,
    pub call_timeout_ms: u32,
    pub attempts: u16,
    pub retry_window: RetryWindow,
    pub retry: RetryPolicy,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkKey {
    pub symbol: String,
    pub subject: CredentialSubject,
    pub slot: CredentialSlot,
    pub identity: Identity,
    pub intent: RoutingIntent,
    pub operation: Operation,
    pub native: String,
    pub work: NativeWork,
    pub publish: bool,
    pub fallback: Fallback,
}

pub(super) struct Index<H>(Vec<(WorkKey, Token, H)>);
impl<H> Default for Index<H> {
    fn default() -> Self {
        Self(Vec::new())
    }
}
impl<H> Index<H> {
    fn space(&self) -> bool {
        self.0.len() < MAX_ATTEMPTS
    }
    fn reserve(&mut self) -> Result<(), VenueError> {
        if !self.space() {
            return Err(invalid("metadata work capacity exceeded"));
        }
        self.0
            .try_reserve(1)
            .map_err(|_| invalid("metadata work capacity exceeded"))
    }
    fn lookup(&self, key: &WorkKey, current: impl Fn(&Token) -> bool) -> Option<&H> {
        self.0
            .iter()
            .find(|(other, token, _)| other == key && current(token))
            .map(|(_, _, handle)| handle)
    }
    pub(super) fn remove(&mut self, token: &Token) {
        self.0.retain(|(_, existing, _)| existing != token);
    }
}

impl State {
    pub(crate) fn work_space(&self, symbol: &str) -> bool {
        self.work.space()
            && self.attempts.len() < MAX_ATTEMPTS
            && (self.revisions.contains_key(symbol) || self.revisions.len() < MAX_SYMBOLS)
    }
    pub(crate) fn lookup_work(&self, key: &WorkKey) -> Option<ExecutionHandle> {
        self.work.lookup(key, |token| self.current(token)).cloned()
    }
    pub(crate) fn work_handles(&self) -> Vec<(Token, ExecutionHandle)> {
        self.work
            .0
            .iter()
            .map(|(_, token, handle)| (token.clone(), handle.clone()))
            .collect()
    }
    pub(crate) fn bind_work(
        &mut self,
        key: WorkKey,
        handle: ExecutionHandle,
    ) -> Result<Token, VenueError> {
        if key.operation != Operation::Read || !self.work_space(&key.symbol) {
            return Err(invalid("metadata work capacity or operation is invalid"));
        }
        self.work.reserve()?;
        let token = self.begin(&key.symbol, key.publish, key.fallback)?;
        self.work.0.push((key, token.clone(), handle));
        Ok(token)
    }
    pub(crate) fn retire_work(&mut self, token: &Token) {
        if let Some(read) = self.attempts.remove(token)
            && self.current.get(&read.symbol) == Some(token)
        {
            self.current.remove(&read.symbol);
        }
        self.work.remove(token);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signing::KeyRef;
    fn key() -> WorkKey {
        let identity = Identity::new(
            KeyRef("ref:credential".into()),
            Some(KeyRef("ref:signing".into())),
        );
        WorkKey {
            symbol: "BTC".into(),
            subject: CredentialSubject::ConfiguredPrimary(identity.clone()),
            slot: CredentialSlot::Primary,
            identity,
            intent: RoutingIntent::Primary,
            operation: Operation::Read,
            native: "BTC-USDT".into(),
            work: NativeWork {
                kind: "settings",
                endpoints: &["/leverage", "/margin"],
                step_waits_ms: &[0, 0],
                calls_per_step: &[1, 1],
                root_timeout_ms: 80_000,
                call_timeout_ms: 20_000,
                attempts: 2,
                retry_window: RetryWindow::FreshAttempt,
                retry: RetryPolicy::ReadFailure,
            },
            publish: true,
            fallback: Fallback::IndependentRails,
        }
    }
    #[test]
    fn full_key_separates_each_work_policy_and_credential_component() {
        let original = key();
        let mut state = State::default();
        let token = state
            .begin("BTC", true, Fallback::IndependentRails)
            .unwrap();
        let mut index = Index::<u32>::default();
        index.reserve().unwrap();
        index.0.push((original.clone(), token.clone(), 7));
        assert_eq!(index.lookup(&original, |_| true), Some(&7));
        let mut alternatives = Vec::new();
        macro_rules! different {
            ($field:ident, $value:expr) => {{
                let mut other = original.clone();
                other.$field = $value;
                alternatives.push(other);
            }};
        }
        different!(symbol, "ETH".into());
        different!(native, "OTHER".into());
        different!(publish, false);
        different!(fallback, Fallback::CompleteSetting);
        different!(slot, CredentialSlot::Secondary);
        different!(intent, RoutingIntent::Secondary);
        different!(operation, Operation::Mutation);
        let mut other = original.clone();
        other.identity.set_signing(Some(KeyRef("ref:other".into())));
        alternatives.push(other);
        let mut other = original.clone();
        let CredentialSubject::ConfiguredPrimary(subject) = &mut other.subject;
        subject.credential = KeyRef("ref:other-primary".into());
        alternatives.push(other);
        let mut other = original.clone();
        other.work.endpoints = &["/different"];
        alternatives.push(other);
        let mut other = original.clone();
        other.work.call_timeout_ms += 1;
        alternatives.push(other);
        let mut other = original.clone();
        other.work.attempts += 1;
        alternatives.push(other);
        let mut other = original.clone();
        other.work.retry_window = RetryWindow::Shared;
        alternatives.push(other);
        let mut other = original.clone();
        other.work.retry = RetryPolicy::Never;
        alternatives.push(other);
        let mut other = original.clone();
        other.work.kind = "return-only";
        alternatives.push(other);
        for other in alternatives {
            assert_eq!(index.lookup(&other, |_| true), None);
        }
        assert_eq!(index.lookup(&original, |_| false), None);
    }
    #[test]
    fn bounded_index_still_finds_same_key_at_capacity_and_removal_frees_space() {
        let mut state = State::default();
        let mut index = Index::<usize>::default();
        let first = key();
        let mut first_token = None;
        for ordinal in 0..MAX_ATTEMPTS {
            let mut key = first.clone();
            if ordinal != 0 {
                key.symbol = ordinal.to_string();
            }
            let token = state.begin(&key.symbol, true, key.fallback).unwrap();
            if ordinal == 0 {
                first_token = Some(token.clone());
            }
            index.reserve().unwrap();
            index.0.push((key, token, ordinal));
        }
        assert!(!index.space());
        assert!(index.reserve().is_err());
        assert_eq!(index.lookup(&first, |_| true), Some(&0));
        index.remove(&first_token.unwrap());
        assert!(index.space());
        index.reserve().unwrap();
        assert_eq!(index.lookup(&first, |_| true), None);
    }
}
