"""Exact equal-scalar candidates; includes parseable raw payloads, not a defect verdict."""
import collections, json, pathlib
root = pathlib.Path(__file__).resolve().parents[1] / "tests/fixtures"
counts = collections.Counter()
records = 0

def walk(value, name):
    global records
    if isinstance(value, dict):
        records += 1
        groups = collections.defaultdict(list)
        for key, child in value.items():
            if type(child) in (str, bool, int, float):
                groups[("number" if type(child) in (int, float) else type(child).__name__, float(child) if type(child) in (int, float) else child)].append(key)
            if key in ("raw", "positional") and isinstance(child, str):
                try:
                    walk(json.loads(child), name)
                except (ValueError, TypeError):
                    pass
            walk(child, name)
        counts[name] += sum(len(keys) > 1 for keys in groups.values())
    elif isinstance(value, list):
        for child in value:
            walk(child, name)

files = sorted(root.glob("*.json"))
for file in files:
    walk(json.loads(file.read_text()), file.name)
print(json.dumps({"files": len(files), "records": records, "equalScalarGroups": sum(counts.values()), "perFile": dict(counts)}, indent=2))
