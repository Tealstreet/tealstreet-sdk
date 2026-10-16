mod runtime_selector {
    pub type WooV3MigrationRouteId = ();
}
mod shadow_compare {
    include!(concat!(env!("WOO_SOURCE_DIR"), "/shadow_compare.rs"));
}
#[cfg(positive)]
struct StructuralError;
#[cfg(positive)]
impl shadow_compare::ShadowErrorReference for StructuralError {
    type Value = ();
    type Failure = std::convert::Infallible;
    fn native(&self) -> &() {
        &()
    }
    fn name(&self) -> Result<Vec<u16>, Self::Failure> {
        Ok(vec![69])
    }
    fn message(&self) -> Result<Vec<u16>, Self::Failure> {
        Ok(vec![77])
    }
    fn stack(&self) -> Result<Option<Vec<u16>>, Self::Failure> {
        Ok(None)
    }
    fn cause(&self) -> Result<Option<()>, Self::Failure> {
        Ok(None)
    }
}
#[cfg(positive)]
fn main() {
    let _value = shadow_compare::WooV3FailedRead::new((), (), StructuralError);
}
#[cfg(not(positive))]
fn main() {
    let _value = shadow_compare::WooV3FailedRead::new((), (), 17_i32);
}
