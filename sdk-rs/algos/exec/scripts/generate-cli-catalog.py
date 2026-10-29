#!/usr/bin/env python3
"""Generate hosted help metadata from the registered TS commands."""
import ast
import json
from pathlib import Path
import re
import sys

root = Path(__file__).resolve().parents[4]
commands = root / "packages/cli-core/src/commands"
registered = re.findall(r"new (\w+Command)\(\)", (commands / "CommandExecutor.ts").read_text())
found = {}
for path in commands.glob("**/*Command.ts"):
    source = path.read_text()
    for match in re.finditer(r"export class (\w+Command)\b", source):
        block = source[match.end():]
        next_class = re.search(r"export class \w+Command\b", block)
        if next_class:
            block = block[:next_class.start()]
        def literal(field):
            m = re.search(r"readonly " + field + r"(?:\s*:\s*[^=]+)?\s*=\s*('(?:\\.|[^'\\])*'|\"(?:\\.|[^\"\\])*\")", block)
            return ast.literal_eval(m.group(1)) if m else ""
        alias = re.search(r"readonly aliases(?:\s*:\s*[^=]+)?\s*=\s*\[([^\]]*)\]", block)
        aliases = re.findall(r"['\"]([^'\"]+)['\"]", alias.group(1)) if alias else []
        examples = re.search(r"readonly examples(?:\s*:\s*[^=]+)?\s*=\s*\[(.*?)\];", block, re.S)
        examples = [ast.literal_eval(m.group(0)) for m in re.finditer(r"'(?:\\.|[^'\\])*'|\"(?:\\.|[^\"\\])*\"", examples.group(1))] if examples else []
        found[match.group(1)] = {"name": literal("name"), "aliases": aliases, "description": literal("description"), "examples": examples}
rows = [found[name] for name in registered]
assert all(row["name"] for row in rows)
target = root / "sdk-rs/algos/exec/src/catalog.json"
content = json.dumps(rows, ensure_ascii=False, indent=2) + "\n"
if "--check" in sys.argv:
    if target.read_text() != content:
        raise SystemExit("CLI catalog drift: run algos/exec/scripts/generate-cli-catalog.py")
else:
    target.write_text(content)
