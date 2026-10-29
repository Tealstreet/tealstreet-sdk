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

metadata = (root / "packages/safe-cex/src/exchangeMetadata.ts").read_text()
blocks = {}
current = None
for line in metadata.splitlines():
    text = line.strip()
    if text.startswith("const ") and text.endswith(" = {"):
        current = text[6:].split(":")[0].split()[0]
        blocks[current] = []
    elif text == "};":
        current = None
    elif current:
        blocks[current].append(text)
def property_value(name, field):
    block = blocks[name]
    for line in block:
        if line.startswith(field + ": "):
            raw = line[len(field)+2:].rstrip(",")
            boolean = re.match(r"^(true|false)(?:,|\s|$)", raw)
            if boolean:
                return boolean.group(1) == "true"
            literal = re.match(r"'(?:\\.|[^'\\])*'|\"(?:\\.|[^\"\\])*\"",raw)
            if literal:
                return ast.literal_eval(literal.group(0))
            raise ValueError(f"nonliteral metadata {name}.{field}: {raw}")
    parent = next((line[3:].rstrip(",") for line in block if line.startswith("...")), None)
    return property_value(parent, field) if parent else None
mapping = metadata.split("export const exchangeMetadata:",1)[1].split("} = {",1)[1].split("};",1)[0]
venues = re.findall(r"^\s*['\"]?([a-z0-9-]+)['\"]?: (\w+),", mapping, re.M)
assert len(venues) >= 50
exchanges = []
for venue, name in venues:
    if not property_value(name, "enabled") or property_value(name,"hiddenFromNewAccountPicker"):
        continue
    required = ["apiKey"]
    if not property_value(name,"requiresNoSecret"):
        required.append("apiSecret")
    for key, fields in [("requiresPassphrase",["passphrase"]),("requiresApplicationId",["applicationId"]),("requiresApiKeyIndex",["apiKeyIndex"]),("requiresOmniKeySeed",["omniKeySeed"]),("requiresParadexAddress",["paradexAddress","subkeyAddress"])]:
        if property_value(name,key):
            required.extend(fields)
    if venue == "grvt":
        required.extend(["grvtSignerAddress","grvtTradingAccountId"])
    exchanges.append({"name":venue,"displayName":property_value(name,"prettyName"),"requiredFields":required})
prompt_source = (root / "packages/cli-core/src/utils/PromptFormatter.ts").read_text().split("export const PROMPT_PRESETS:",1)[1].split("= {",1)[1].split("\n};",1)[0]
presets = {}
for name, block in re.findall(r"^  (\w+): \{(.*?)^  \},", prompt_source, re.M | re.S):
    presets[name] = {}
    for key in ("format","prefix"):
        literal = re.search(key + r": ('(?:\\.|[^'\\])*'|\"(?:\\.|[^\"\\])*\")", block)
        presets[name][key] = ast.literal_eval(literal.group(1))
assert len(presets) == 5
for filename, data in [("catalog.json",rows),("exchanges.json",exchanges),("prompt-presets.json",presets)]:
    target = root / "sdk-rs/algos/exec/src" / filename
    content = json.dumps(data, ensure_ascii=False, indent=2) + "\n"
    if "--check" in sys.argv:
        if target.read_text() != content:
            raise SystemExit(f"CLI metadata drift in {filename}: run algos/exec/scripts/generate-cli-catalog.py")
    else:
        target.write_text(content)
