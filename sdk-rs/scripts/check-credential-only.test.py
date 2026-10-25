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

    def test_owners_may_use_the_restricted_accessors(self):
        text = 'fn f(i: &mut Identity) { i.set_signing_ref(None); i.validate_credential(); }\n'
        self.assertEqual(check.violations('engine/src/signing.rs', text), [])
        self.assertEqual(len(check.violations('registry/src/lib.rs', text)), 2)

    def test_a_gated_field_hides_only_that_field(self):
        text = ('struct S {\n    #[cfg(test)]\n    probe: u8,\n    real: u8,\n}\n'
                'impl S { fn f(i: &Identity) { i.signing_opt(); } }\n'
                'fn g() -> S { S {\n    #[cfg(test)]\n    probe: 0,\n    real: 1 } }\n')
        self.assertEqual(len(check.violations('venues/x/src/lib.rs', text)), 1)

    def test_raw_strings_do_not_unbalance_a_test_module(self):
        text = ('#[cfg(test)]\nmod tests {\n    const J: &str = r#"{\"a\":1}"#;\n}\n'
                'fn f(i: &Identity) { i.signing_opt(); }\n')
        self.assertEqual(len(check.violations('venues/x/src/lib.rs', text)), 1)


if __name__ == '__main__':
    unittest.main()
