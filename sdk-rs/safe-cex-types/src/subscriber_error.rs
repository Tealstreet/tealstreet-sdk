//! Error reported by a host callback, before any Store fanout policy is applied.
#[derive(Debug, Clone)]
pub struct StoreSubscriberError {
    message: String,
    response: Option<serde_json::Value>,
    cause: Option<std::rc::Rc<dyn std::error::Error>>,
}
impl StoreSubscriberError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            response: None,
            cause: None,
        }
    }
    pub fn with_response(message: impl Into<String>, response: serde_json::Value) -> Self {
        Self {
            message: message.into(),
            response: Some(response),
            cause: None,
        }
    }
    /// Retain the original callback error without copying or flattening its metadata.
    pub fn with_cause(mut self, cause: std::rc::Rc<dyn std::error::Error>) -> Self {
        self.cause = Some(cause);
        self
    }
    pub fn message(&self) -> &str {
        &self.message
    }
    pub fn response(&self) -> Option<&serde_json::Value> {
        self.response.as_ref()
    }
}
impl std::fmt::Display for StoreSubscriberError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl PartialEq for StoreSubscriberError {
    fn eq(&self, other: &Self) -> bool {
        self.message == other.message
            && self.response == other.response
            && match (&self.cause, &other.cause) {
                (None, None) => true,
                (Some(left), Some(right)) => std::rc::Rc::ptr_eq(left, right),
                _ => false,
            }
    }
}
impl Eq for StoreSubscriberError {}
impl std::error::Error for StoreSubscriberError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.cause.as_deref()
    }
}

/// Store callback with the source worker-origin flag.
pub type StoreSubscriber = Box<dyn Fn(&crate::StoreData, bool) -> Result<(), StoreSubscriberError>>;
/// Read-only public-data callback uses the same failure payload.
pub type PublicDataSubscriber = Box<dyn Fn(&crate::StoreData) -> Result<(), StoreSubscriberError>>;

#[cfg(test)]
mod tests {
    use super::StoreSubscriberError;
    use std::{error::Error, rc::Rc};

    #[derive(Debug)]
    struct ListenerFailure {
        code: &'static str,
    }
    impl std::fmt::Display for ListenerFailure {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("listener TypeError")
        }
    }
    impl Error for ListenerFailure {}

    #[test]
    fn original_listener_cause_survives_clone_and_downcast() {
        let cause: Rc<dyn Error> = Rc::new(ListenerFailure { code: "E_LISTENER" });
        let error = StoreSubscriberError::with_response(
            "listener TypeError",
            serde_json::json!({"status": 409}),
        )
        .with_cause(cause.clone());
        let cloned = error.clone();
        assert_eq!(error, cloned);
        assert!(
            std::ptr::eq(error.source().unwrap(), cloned.source().unwrap()),
            "subscriber original cause identity"
        );
        assert_eq!(
            cloned
                .source()
                .unwrap()
                .downcast_ref::<ListenerFailure>()
                .unwrap()
                .code,
            "E_LISTENER",
            "subscriber original cause metadata"
        );
        assert_eq!(cloned.response().unwrap()["status"], 409);
        let different = StoreSubscriberError::with_response(
            "listener TypeError",
            serde_json::json!({"status": 409}),
        )
        .with_cause(Rc::new(ListenerFailure { code: "E_LISTENER" }));
        assert_ne!(
            error, different,
            "distinct original causes must remain distinct"
        );
        assert!(StoreSubscriberError::new("no cause").source().is_none());
    }
}
