#!/usr/bin/env python3
import importlib.util
import pathlib
import unittest

spec = importlib.util.spec_from_file_location(
    'check_plan_declarations', pathlib.Path(__file__).with_name('check-plan-declarations.py'))
check = importlib.util.module_from_spec(spec)
spec.loader.exec_module(check)

P = pathlib.PurePosixPath
FALSE = 'impl Venue for V {\n    fn http_plans() -> bool {\n        false\n    }\n}\n'
OPENS = 'fn f(cx: &mut Cx<()>) { let _ = cx.begin_preflight(policy); }\n'


class PlanDeclarations(unittest.TestCase):
    def test_a_crate_root_declaration_covers_the_whole_crate(self):
        sources = {P('venues/x/src/lib.rs'): FALSE, P('venues/x/src/trading/protective.rs'): OPENS}
        found = check.violations({P('venues/x/src/lib.rs')}, sources, {})
        self.assertEqual(len(found), 1)
        self.assertIn('protective.rs:1', found[0])

    def test_a_module_declaration_covers_only_its_module(self):
        sources = {P('venues/x/src/spot/mod.rs'): FALSE, P('venues/x/src/trading.rs'): OPENS}
        self.assertEqual(check.violations({P('venues/x/src/spot/mod.rs')}, sources, {}), [])
        sources[P('venues/x/src/spot/trading.rs')] = OPENS.replace('begin_preflight', 'admit_serial_plan')
        self.assertEqual(len(check.violations({P('venues/x/src/spot/mod.rs')}, sources, {})), 1)

    def test_every_admission_api_counts(self):
        for api in ['begin_preflight_with_session', 'issue_preflight', 'admit_plan_with_terminals',
                    'admit_plan', 'begin_planned_execution']:
            sources = {P('venues/x/src/lib.rs'): FALSE + OPENS.replace('begin_preflight', api)}
            self.assertEqual(len(check.violations({P('venues/x/src/lib.rs')}, sources, {})), 1, api)

    def test_a_crate_root_declaration_refuses_a_plan_feature(self):
        sources = {P('venues/x/src/lib.rs'): FALSE}
        cargo = {P('venues/x'): 'safe-cex-engine = { features = ["order-locks", "serial-plans"] }\n'}
        self.assertEqual(len(check.violations({P('venues/x/src/lib.rs')}, sources, cargo)), 1)
        self.assertEqual(check.violations({P('venues/x/src/spot/mod.rs')},
                                          {P('venues/x/src/spot/mod.rs'): FALSE}, cargo), [])

    def test_only_a_false_declaration_is_detected(self):
        self.assertTrue(check.DECLARES_FALSE.search(FALSE))
        self.assertFalse(check.DECLARES_FALSE.search(FALSE.replace('false', 'true')))
        self.assertTrue(check.DECLARES_FALSE.search('fn http_plans() -> bool { false }'))

    def test_the_repository_is_clean(self):
        self.assertEqual(check.main(), 0)


if __name__ == '__main__':
    unittest.main()
