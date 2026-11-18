//! Concrete string joins. `[T]::join` monomorphizes `join_generic_copy` again in every
//! crate (and at every crate's opt-level); these keep one copy per element type.

/// `parts.join(separator)` for slices of `String` or `&str`, compiled once each.
pub trait JoinWith {
    fn join_with(&self, separator: &str) -> String;
}

impl JoinWith for [String] {
    #[inline(never)]
    fn join_with(&self, separator: &str) -> String {
        self.join(separator)
    }
}

impl JoinWith for [&str] {
    #[inline(never)]
    fn join_with(&self, separator: &str) -> String {
        self.join(separator)
    }
}

#[cfg(test)]
mod tests {
    use super::JoinWith;

    #[test]
    fn joins_exactly_as_the_std_join() {
        let owned: Vec<String> = ["a", "", "ü€", "b"].map(String::from).to_vec();
        let borrowed: Vec<&str> = owned.iter().map(String::as_str).collect();
        for separator in ["", ",", ", ", "; ", "&", "→"] {
            assert_eq!(owned.join_with(separator), owned.join(separator));
            assert_eq!(borrowed.join_with(separator), borrowed.join(separator));
        }
        assert_eq!(Vec::<String>::new().join_with(", "), "");
        assert_eq!(["only"].join_with(", "), "only");
    }
}
