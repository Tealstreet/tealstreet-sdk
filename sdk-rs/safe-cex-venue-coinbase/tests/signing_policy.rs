use safe_cex_exchange::exchange::{
    BaseSigningOptionsView, can_sign_with, is_public_data_only_with,
};
use std::cell::RefCell;
struct View {
    values: [bool; 4],
    expected: &'static [&'static str],
    trace: RefCell<Vec<&'static str>>,
}
impl View {
    fn read(&self, index: usize, name: &'static str) -> bool {
        let mut trace = self.trace.borrow_mut();
        assert_eq!(
            self.expected.get(trace.len()),
            Some(&name),
            "native signing getter reads only the next source operand"
        );
        trace.push(name);
        self.values[index]
    }
}
impl BaseSigningOptionsView for View {
    fn key_truthy(&self) -> bool {
        self.read(0, "key")
    }
    fn secret_truthy(&self) -> bool {
        self.read(1, "secret")
    }
    fn remote_signer_truthy(&self) -> bool {
        self.read(2, "remote")
    }
    fn payload_signer_truthy(&self) -> bool {
        self.read(3, "payload")
    }
}
#[test]
fn native_signing_policy_observes_lazy_source_operand_order() {
    for (values, expected, result) in [
        ([false, true, true, true], &["key"][..], false),
        ([true, true, true, true], &["key", "secret"][..], true),
        (
            [true, false, true, true],
            &["key", "secret", "remote"][..],
            true,
        ),
        (
            [true, false, false, true],
            &["key", "secret", "remote", "payload"][..],
            true,
        ),
        (
            [true, false, false, false],
            &["key", "secret", "remote", "payload"][..],
            false,
        ),
    ] {
        let view = View {
            values,
            expected,
            trace: RefCell::new(Vec::new()),
        };
        assert_eq!(can_sign_with(&view), result, "native signing policy result");
        assert_eq!(
            *view.trace.borrow(),
            expected,
            "native signing getter exact source trace"
        );
    }
}
#[test]
fn native_public_policy_preserves_lazy_virtual_signer_callback() {
    let trace = RefCell::new(Vec::new());
    assert!(is_public_data_only_with(
        || {
            trace.borrow_mut().push("public");
            true
        },
        || panic!("source short circuit must skip virtual canSign")
    ));
    assert_eq!(*trace.borrow(), ["public"]);
    for virtual_result in [false, true] {
        trace.borrow_mut().clear();
        let result = is_public_data_only_with(
            || {
                trace.borrow_mut().push("public");
                false
            },
            || {
                trace.borrow_mut().push("virtual canSign");
                virtual_result
            },
        );
        assert_eq!(
            result, !virtual_result,
            "native policy uses virtual canSign result"
        );
        assert_eq!(
            *trace.borrow(),
            ["public", "virtual canSign"],
            "native policy callback trace"
        );
    }
}
