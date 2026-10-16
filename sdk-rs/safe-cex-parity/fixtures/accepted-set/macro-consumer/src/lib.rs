// A dependency's binding deliberately resembles an official registry crate.
// The production control inserts calls only after verifying the empty baseline.
pub fn ordinary_expression() {
    let _ = vec![1, 2];
}
