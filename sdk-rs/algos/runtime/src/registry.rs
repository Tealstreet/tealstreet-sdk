use crate::AlgoCx;

/// One algo family's wire ops (`chaser:start`, `chaser:terminate`, ...).
pub trait AlgoFactory {
    fn ops(&self) -> &'static [&'static str];
    /// Handle one op with its JSON payload. An `Err` is reported to the client.
    fn handle(&self, op: &str, data: &str, cx: &AlgoCx) -> Result<(), String>;
}

/// The server's op lookup over every algo family.
pub struct Registry {
    factories: Vec<Box<dyn AlgoFactory>>,
}

impl Registry {
    /// Panics when two families claim the same op, which would shadow one.
    pub fn new(factories: Vec<Box<dyn AlgoFactory>>) -> Self {
        let mut ops: Vec<&str> = factories
            .iter()
            .flat_map(|factory| factory.ops().iter().copied())
            .collect();
        let count = ops.len();
        ops.sort_unstable();
        ops.dedup();
        assert_eq!(
            ops.len(),
            count,
            "algo ops must be disjoint across families"
        );
        Self { factories }
    }

    pub fn handles(&self, op: &str) -> bool {
        self.factory(op).is_some()
    }

    /// `None` when no family owns `op`.
    pub fn dispatch(&self, op: &str, data: &str, cx: &AlgoCx) -> Option<Result<(), String>> {
        self.factory(op).map(|factory| factory.handle(op, data, cx))
    }

    fn factory(&self, op: &str) -> Option<&dyn AlgoFactory> {
        self.factories
            .iter()
            .find(|factory| factory.ops().contains(&op))
            .map(|factory| &**factory)
    }
}
