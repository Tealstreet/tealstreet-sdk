#!/usr/bin/env python3
import importlib.util
import pathlib
import unittest

spec = importlib.util.spec_from_file_location(
    'check_credential_only', pathlib.Path(__file__).with_name('check-credential-only.py'))
check = importlib.util.module_from_spec(spec)
spec.loader.exec_module(check)


class CredentialOnlyBans(unittest.TestCase):
    def test_code_after_a_test_module_declaration_is_still_scanned(self):
        text = '#[cfg(test)]\nmod tests;\nfn f(i: &Identity) { i.signing_opt(); }\n'
        self.assertEqual(len(check.violations('venues/x/src/lib.rs', text)), 1)

    def test_code_after_an_inline_test_module_is_still_scanned(self):
        text = ('#[cfg(test)]\nmod tests {\n    fn t() { let _ = KeyRef(String::new()); }\n}\n'
                'fn f() -> KeyRef { KeyRef(String::default()) }\n')
        found = check.violations('venues/x/src/lib.rs', text)
        self.assertEqual(found, ['empty KeyRef outside setup validation metadata: venues/x/src/lib.rs:5'])

    def test_only_the_pinned_setup_lines_may_build_an_empty_ref(self):
        pinned = 'identity.passphrase = Some(KeyRef(String::new()));\n'
        self.assertEqual(check.violations('crypto/src/signer.rs', pinned), [])
        self.assertEqual(len(check.violations('crypto/src/signer.rs', 'let k = KeyRef("".into());\n')), 1)

    def test_only_trusted_local_setup_marks_a_scalar_only_primary(self):
        text = 'fn f(p: &CredentialSetupPolicy, i: &mut Identities) { p.install_scalar_only_primary(i); }\n'
        self.assertEqual(check.violations('crypto/src/signer.rs', text), [])
        self.assertEqual(len(check.violations('registry/src/lib.rs', text)), 1)

    def test_owners_may_use_the_restricted_accessors(self):
        text = 'fn f(i: &mut Identity) { i.set_signing_ref(None); i.validate_credential(); }\n'
        self.assertEqual(check.violations('engine/src/signing.rs', text), [])
        self.assertEqual(len(check.violations('registry/src/lib.rs', text)), 2)

    def test_gated_fields_statements_and_macros_are_still_scanned(self):
        for gated in ('struct S {\n    #[cfg(test)]\n    probe: u8,\n}\n',
                      'fn f() {\n    #[cfg(test)]\n    { let _x = 1; }\n}\n',
                      'fn f() -> u8 {\n    #[cfg(test)]\n    if let Some(x) = y() { return x; }\n    0\n}\n',
                      '#[cfg(test)]\nthread_local! { static T: u8 = 0; }\n'):
            text = gated + 'fn g(i: &Identity) { i.signing_opt(); }\n'
            self.assertEqual(len(check.violations('venues/x/src/lib.rs', text)), 1, gated)

    def test_raw_strings_do_not_unbalance_a_test_module(self):
        text = ('#[cfg(test)]\nmod tests {\n    const J: &str = r#"{\"a\":1}"#;\n}\n'
                'fn f(i: &Identity) { i.signing_opt(); }\n')
        self.assertEqual(len(check.violations('venues/x/src/lib.rs', text)), 1)

    def test_char_literals_do_not_unbalance_a_test_module(self):
        text = ("#[cfg(test)]\nmod tests {\n    fn t(s: &str) -> bool { s.starts_with('{') }\n}\n"
                "fn f<'a>(i: &'a Identity) { i.signing_opt(); }\n")
        self.assertEqual(len(check.violations('venues/x/src/lib.rs', text)), 1)


if __name__ == '__main__':
    unittest.main()
