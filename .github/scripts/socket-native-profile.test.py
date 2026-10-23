#!/usr/bin/env python3
"""Offline negative controls; never builds or replays."""
import copy
import importlib.util
import json
from pathlib import Path
import tempfile
import subprocess
import sys
import time
import os
import unittest
from unittest.mock import patch

HERE = Path(__file__).resolve().parent
SPEC = importlib.util.spec_from_file_location("native_probe", HERE / "socket-native-profile.py")
PROBE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(PROBE)


class Controls(unittest.TestCase):
    def setUp(self):
        self.contract = json.loads((HERE / "socket-native-profile.contract.json").read_text())

    def test_frozen_contract_and_driver(self):
        self.assertEqual(PROBE.sha(HERE / "socket-native-profile.contract.json"), PROBE.CONTRACT_SHA)
        self.assertEqual(PROBE.sha(HERE / "socket-native-profile-driver.rs"), self.contract["driverSha256"])
        self.assertEqual(PROBE.sha(HERE / "socket-native-profile.Cargo.lock"), self.contract["lockSha256"])
        PROBE.validate_contract(self.contract)

    def test_reordered_or_incomplete_protocol_refused(self):
        for order in [PROBE.ORDER[:-1], list(reversed(PROBE.ORDER)), ["current"] * 8]:
            with self.subTest(order=order):
                bad = copy.deepcopy(self.contract)
                bad["protocol"]["order"] = order
                with self.assertRaises(AssertionError):
                    PROBE.validate_contract(bad)

    def test_shorter_or_extra_sampling_refused(self):
        for field, value in [("durationSeconds", 20), ("profileSeconds", 10), ("warmups", 2), ("frequencyHz", 100)]:
            with self.subTest(field=field):
                bad = copy.deepcopy(self.contract)
                bad["protocol"][field] = value
                with self.assertRaises(AssertionError):
                    PROBE.validate_contract(bad)

    def test_duplicate_or_mislabelled_sources_refused(self):
        for field, value in [("role", "landing"), ("head", self.contract["cases"][0]["head"]),
                             ("tree", "main"), ("workspaceLockSha256", "a" * 64)]:
            with self.subTest(field=field):
                bad = copy.deepcopy(self.contract)
                bad["cases"][1][field] = value
                with self.assertRaises(AssertionError):
                    PROBE.validate_contract(bad)

    def test_changed_contract_bytes_refused_before_git_build_or_replay(self):
        with tempfile.TemporaryDirectory() as directory:
            inputs = Path(directory)
            raw = (HERE / "socket-native-profile.contract.json").read_bytes()
            (inputs / "socket-native-profile.contract.json").write_bytes(raw + b" ")
            with patch.object(PROBE, "git", side_effect=AssertionError("must not touch source")) as git:
                with self.assertRaisesRegex(AssertionError, "changed diagnostic contract"):
                    PROBE.run(inputs, inputs, inputs, "never-run")
                git.assert_not_called()

    def test_dirty_or_different_head_tree_refused(self):
        case = self.contract["cases"][0]
        for outputs in [[" M source.rs"], ["", "a" * 40], ["", case["head"], "b" * 40]]:
            with self.subTest(outputs=outputs), patch.object(PROBE, "git", side_effect=outputs):
                with self.assertRaises(AssertionError):
                    PROBE.checked_source(HERE, case)

    def test_wrong_workspace_lock_refused(self):
        case = self.contract["cases"][0]
        with patch.object(PROBE, "git", side_effect=["", case["head"], case["tree"]]), \
             patch.object(PROBE, "sha", return_value="0" * 64):
            with self.assertRaises(AssertionError):
                PROBE.checked_source(HERE, case)

    def test_exact_facts_parse_without_evaluation(self):
        self.assertEqual(PROBE.facts("READY pid=1 rows=28703 result=" + repr(tuple(self.contract["facts"]))),
                         self.contract["facts"])
        for bad in ["", "DONE passes=1", "result=('partial',)", "result=(__import__('os').getpid(), 1, 2, 3)"]:
            with self.subTest(bad=bad), self.assertRaises((AssertionError, ValueError, SyntaxError)):
                PROBE.facts(bad)

    def test_optimized_python_refuses_before_any_inputs(self):
        result = subprocess.run([sys.executable, "-O", str(HERE / "socket-native-profile.py")],
                                capture_output=True, text=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("refuses optimized Python", result.stderr)

    def test_real_target_chain_and_self_samples_required(self):
        stacks = " 123/123 102.222222222: cpu-clock:u:\n 123abcd first+0x1 (/tmp/current-native)\n 123abce second (/tmp/current-native)\n\n"
        report = " 50.00% 10 [.] first\n 50.00% 10 [.] second\n"
        self.assertEqual(PROBE.profile_presence(stacks, report, 123), {
            "targetSamples": 1, "usableCallchains": 1, "selfReportSamples": 20,
        })
        for bad_stacks, bad_report in [
            ("# header only\n", report), (stacks.replace("123/123", "456/456"), report),
            (stacks.replace("first+0x1", "[unknown]").replace("second", "[unknown]"), report),
            (stacks, "# no samples\n"), (stacks, " 0.00% 0 [.] first\n"),
        ]:
            with self.subTest(stacks=bad_stacks, report=bad_report), self.assertRaises(AssertionError):
                PROBE.profile_presence(bad_stacks, bad_report, 123)

    def test_cleanup_preserves_exited_and_running_child_accounting(self):
        for code, exited in [("print('partial', flush=True)", True), ("import time;time.sleep(60)", False)]:
            with self.subTest(exited=exited):
                process = subprocess.Popen([sys.executable, "-c", code], stdout=subprocess.PIPE, text=True)
                if exited:
                    # Wait for EOF without Popen.poll/wait reaping its rusage.
                    self.assertEqual(process.stdout.read(), "partial\n")
                usage = PROBE.stop_owned_process(process)
                self.assertGreaterEqual(usage.ru_utime, 0)
                self.assertEqual(process.returncode, 0 if exited else -9)
                with self.assertRaises(ChildProcessError):
                    os.wait4(process.pid, os.WNOHANG)
                process.stdout.close()


if __name__ == "__main__":
    unittest.main()
