"""Select real remote decoder fields, excluding public data and serialization views."""
import re

SCHEMAS = re.compile(r"(?ms)^struct (?P<owner>\w+)Wire \{\n(?P<body>.*?)^\}")
VECTOR = re.compile(r"    pub (\w+): Vec<([^>]+)>,")

def transform_schema(source, owner, transform):
    matches = [m for m in SCHEMAS.finditer(source) if m["owner"] == owner]
    assert len(matches) == 1, (owner, len(matches))
    match = matches[0]
    before = match["body"]
    after = transform(before)
    assert before != after, owner
    return source[:match.start("body")] + after + source[match.end("body"):]

def replace_decoder_field(source, old, new):
    matches = [m for m in SCHEMAS.finditer(source) if old in m["body"]]
    assert len(matches) == 1, (old, len(matches))
    owner = matches[0]["owner"]
    def replace(body):
        assert body.count(old) == 1, (owner, old)
        return body.replace(old, new)
    return transform_schema(source, owner, replace)

def decoder_vectors(source):
    return [field for schema in SCHEMAS.finditer(source)
            for field in VECTOR.finditer(source, schema.start("body"), schema.end("body"))]
