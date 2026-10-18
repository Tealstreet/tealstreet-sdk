//! Mutation failures retain their origin; the Store never catches and logs a subscriber error.
use crate::model_patch::StorePatchError;
use safe_cex_types::StoreSubscriberError;

#[derive(Debug, Clone, PartialEq)]
pub struct StoreNotificationError {
    subscriber_id: u64,
    cause: StoreSubscriberError,
}
impl StoreNotificationError {
    /// Construct a failure using the subscriber identity assigned by this Store implementor.
    pub fn from_subscriber(subscriber_id: u64, cause: StoreSubscriberError) -> Self {
        Self {
            subscriber_id,
            cause,
        }
    }
    pub fn subscriber_id(&self) -> u64 {
        self.subscriber_id
    }
    pub fn cause(&self) -> &StoreSubscriberError {
        &self.cause
    }
    pub fn message(&self) -> &str {
        self.cause.message()
    }
}
impl std::fmt::Display for StoreNotificationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.cause.fmt(f)
    }
}
impl std::error::Error for StoreNotificationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.cause)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum StoreMutationError {
    Access(safe_cex_types::StoreReadError),
    Patch(StorePatchError),
    Notification(StoreNotificationError),
    InvalidOperation(&'static str),
}
impl StoreMutationError {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Patch(error) if error.is_type_error() => "TypeError",
            _ => "Error",
        }
    }
}
impl From<StorePatchError> for StoreMutationError {
    fn from(error: StorePatchError) -> Self {
        Self::Patch(error)
    }
}
impl From<safe_cex_types::StoreReadError> for StoreMutationError {
    fn from(error: safe_cex_types::StoreReadError) -> Self {
        Self::Access(error)
    }
}
impl From<StoreNotificationError> for StoreMutationError {
    fn from(error: StoreNotificationError) -> Self {
        Self::Notification(error)
    }
}
impl From<&'static str> for StoreMutationError {
    fn from(error: &'static str) -> Self {
        Self::InvalidOperation(error)
    }
}
impl std::fmt::Display for StoreMutationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Access(error) => error.fmt(f),
            Self::Patch(error) => error.fmt(f),
            Self::Notification(error) => error.fmt(f),
            Self::InvalidOperation(message) => f.write_str(message),
        }
    }
}
impl std::error::Error for StoreMutationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Access(error) => Some(error),
            Self::Patch(error) => Some(error),
            Self::Notification(error) => Some(error),
            Self::InvalidOperation(_) => None,
        }
    }
}
