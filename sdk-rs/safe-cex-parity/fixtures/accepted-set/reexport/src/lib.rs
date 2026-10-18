pub use accepted_set_control::Side as ExportedSide;
#[cfg(feature = "native")]
pub use serde_json_vendor::ValueLike as NativeVendor;
pub type ExternalJson = serde_json::Value;
pub type ClosedSide = accepted_set_control::Side;
pub type Callback = Box<dyn Fn(&str)>;
type PrivateVendorAlias = serde_json_vendor::ValueLike;
type PrivateExternalAlias = serde_json::Value;

