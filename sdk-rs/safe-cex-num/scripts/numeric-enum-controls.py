#!/usr/bin/env python3
"""Compile distinguishing enum-dispatch and extra-tag controls, then restore."""
from pathlib import Path
import subprocess

workspace = Path(__file__).resolve().parents[2]
source = workspace / "safe-cex-num/src/numeric_ingress.rs"
original = source.read_bytes()
text = original.decode()
dispatch = '''            Node::Object(entries) => {
                if entries.len() != 1 {
                    return Err(de::Error::custom(
                        "expected a single externally tagged enum variant",
                    ));
                }
                de::value::MapAccessDeserializer::new(Object {
                    entries: entries.into_iter(),
                    value: None,
                })
                .deserialize_enum(name, variants, visitor)
            }'''
old_dispatch = '''            Node::Object(entries) => de::value::MapDeserializer::new(
                entries
                    .into_iter()
                    .map(|(key, value)| (key, JsNumberDeserializer(value.0))),
            )
            .deserialize_enum(name, variants, visitor),'''
guard = '''                if entries.len() != 1 {
                    return Err(de::Error::custom(
                        "expected a single externally tagged enum variant",
                    ));
                }
'''


def run(name=None):
    command = ["cargo", "test", "-p", "safe-cex-num", "--test", "enum_numeric_ingress"]
    if name:
        command.append(name)
    return subprocess.run(command, cwd=workspace, text=True, capture_output=True)


try:
    for label, before, after, test, expected in [
        ("old-map-dispatch", dispatch, old_dispatch,
         "external_tags_reach_enum_visitors_and_keep_native_number_bits",
         "invalid type: map, expected enum External"),
        ("missing-single-tag-guard", guard, "",
         "external_tag_maps_require_one_variant_and_keep_string_union_guards",
         "invalid external enum"),
    ]:
        assert text.count(before) == 1, f"review changed source before {label}"
        source.write_text(text.replace(before, after, 1))
        result = run(test)
        output = result.stdout + result.stderr
        assert result.returncode == 101, f"{label}: expected native assertion failure\n{output}"
        assert f"test {test} ... FAILED" in output, f"{label}: did not execute named test\n{output}"
        assert expected in output, f"{label}: unrelated failure\n{output}"
        print(f"PASS {label}: compiled, named native assertion failed")
        source.write_bytes(original)
finally:
    source.write_bytes(original)

assert source.read_bytes() == original, "source restoration changed bytes"
restored = run()
assert restored.returncode == 0, restored.stdout + restored.stderr
print("PASS exact source restoration: both enum regression tests pass")
