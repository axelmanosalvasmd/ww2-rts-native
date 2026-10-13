#!/usr/bin/env python3
"""Generate or check the unagreed startup-table proposal from pinned static source.

Requires Python 3.10+ and an initialized reference checkout. No game execution,
network access, Cargo, runtime state or runtime-source editing is involved.
"""

import argparse
import hashlib
import json
from pathlib import Path
import re
import struct
import subprocess

BASELINE = "d1f2fb5d31efefb4902576142b135ddab9078920"
REFERENCE = "e4e579a838aabae3b1d9dadbc17a0a87147bc52d"
CATALOG_PATH = "rust/crates/core/data/rules.json"
CATALOG_SHA256 = "96ddf08289375e2338d8efa5c9f8994a9213834691e516e023212bb3d27133df"
SPECIAL_NUMBERS = ["NaN", "Infinity", "-Infinity", "-0"]
CLIENT_STATIC = [
    "CELL", "TICK", "CFG", "MOVE", "COVER", "TRENCH", "TERRAIN", "UNITS",
    "UNIT_TYPES", "SUPPORT", "SUPPORT_TYPES", "FORTS", "ENTRENCH", "ENTRENCH_TYPES",
    "SHAPES", "SLOTS", "SLOT_SCALE", "GUN_SLOTS", "TECH", "TIER_NAMES", "WEATHER",
    "WEATHER_CHOICES", "RAIL_DIRS", "DEBRIS_LIMITS", "SECTION_MASS", "UNIT_ROW",
    "GATE_OPEN_FLAG", "AUTO_FLAG", "OPERATIVE_FLAG", "RIDING_FLAG", "CARGO_FLAG",
    "CARGO_SHIFT",
]
AUTHORING_STATIC = ["MAX_PLAYERS", "POINT_KINDS", "BASE_SURFACES", "MATERIALS", "STRUCTURE_LIMITS"]
NAME_ARRAYS = [
    "UNIT_TYPES", "SUPPORT_TYPES", "ENTRENCH_TYPES", "SHAPES", "WEATHER_KINDS",
    "WEATHER_CHOICES", "TIER_NAMES",
]


def git(repo, *args):
    return subprocess.check_output(["git", *args], cwd=repo)


def source(repo, revision, path):
    return git(repo, "show", f"{revision}:{path}").decode("utf-8")


def digest(data):
    return hashlib.sha256(data).hexdigest()


def json_bytes(value):
    return (json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":"),
                       allow_nan=False) + "\n").encode("utf-8")


def parse_literal(text):
    """Read this pin's simple static literals, never execute JavaScript."""
    text = re.sub(r'([{,]\s*)([A-Za-z_$][\w$]*)\s*:', r'\1"\2":', text)
    text = text.replace("'", '"')
    text = re.sub(r",\s*([}\]])", r"\1", text)
    return json.loads(text)


def literal(text, name):
    pattern = r"export const " + re.escape(name) + r" = (?:Object.freeze\()?(.+?)(?:\))?;"
    match = re.search(pattern, text, re.S)
    if not match:
        raise ValueError(f"static literal absent: {name}")
    return parse_literal(match[1])


def additions(reference):
    extras, locations = {}, {}
    declarations = [
        ("sim.js", ["RAIL_DIRS", "POINT_KINDS", "UNIT_ROW"], "shared/sim.js"),
        ("debris-motion.js", ["DEBRIS_LIMITS", "SECTION_MASS"], "shared/debris-motion.js:2-3"),
        ("structures.js", ["STRUCTURE_LIMITS"], "shared/structures.js:3"),
        ("world-layers.js", ["WORLD_VERSION", "BASE_SURFACES", "OBJECT_TYPES", "BIOMES"],
         "shared/world-layers.js:2-15"),
    ]
    for filename, names, location in declarations:
        text = source(reference, REFERENCE, "shared/" + filename)
        for name in names:
            extras[name] = literal(text, name)
            locations[name] = location
    locations.update(RAIL_DIRS="shared/sim.js:1250", POINT_KINDS="shared/sim.js:624",
                     UNIT_ROW="shared/sim.js:5887")
    sim = source(reference, REFERENCE, "shared/sim.js")
    # Numeric literals include CARGO_FLAG in a shared declaration with RIDING_FLAG.
    for name in ["GATE_OPEN_FLAG", "OPERATIVE_FLAG", "AUTO_FLAG", "RIDING_FLAG",
                 "CARGO_FLAG", "CARGO_SHIFT"]:
        match = re.search(r"\b" + name + r"\s*=\s*(\d+)\b", sim)
        if not match:
            raise ValueError(f"flag literal absent: {name}")
        extras[name] = int(match[1])
        locations[name] = "shared/sim.js:5878-5882"
    layers = source(reference, REFERENCE, "shared/world-layers.js")
    match = re.search(r"export const MATERIALS = Object.freeze\((\{.*?\n\})\);", layers, re.S)
    if not match:
        raise ValueError("MATERIALS literal absent")
    extras["MATERIALS"] = parse_literal(re.sub(r"Object.freeze\((\{[^{}]*\})\)", r"\1", match[1]))
    locations["MATERIALS"] = "shared/world-layers.js:7-15"
    if len(extras) != 17:
        raise ValueError("expected 17 source additions")
    return extras, locations


def formation(repo, tables):
    text = source(repo, BASELINE, "rust/crates/core/src/formation.rs")
    names = [
        ("infantrySlot", "INFANTRY_SLOT"), ("vehicleSlot", "VEHICLE_SLOT"),
        ("slotMargin", "SLOT_MARGIN"), ("rankGap", "RANK_GAP"),
        ("lineRank", "LINE_RANK"), ("maxStretch", "MAX_STRETCH"),
        ("depthSteps", "DEPTH_STEPS"),
    ]
    shapes = re.search(r"pub const SHAPES: \[&str; 4\] = (\[.*?\]);", text)
    if not shapes or json.loads(shapes[1]) != tables["SHAPES"]:
        raise ValueError("native formation shape order differs from the catalog")
    numbers = {}
    for output_name, source_name in names:
        match = re.search(r"pub const " + source_name + r": f64 = ([\d.]+);", text)
        if not match:
            raise ValueError(f"formation constant absent: {source_name}")
        number = float(match[1])
        numbers[output_name] = int(number) if number.is_integer() else number
    order = re.search(r"pub const RULE_NUMBERS: \[f64; 7\] = \[(.*?)\];", text, re.S)
    if not order or re.findall(r"\b[A-Z][A-Z_]+\b", order[1]) != [name for _, name in names]:
        raise ValueError("formation hash number order changed")
    data = b"".join(name.encode() + b"\0" for name in tables["SHAPES"])
    data += b"".join(struct.pack("<d", number) for number in numbers.values())
    value = 0xcbf29ce484222325
    for byte in data:
        value = ((value ^ byte) * 0x100000001b3) & ((1 << 64) - 1)
    if f"{value:016x}" != "ed0ee23d32a621d1":
        raise ValueError("approved formation-only hash changed")
    return {"hash": f"{value:016x}", "scope": "formation-only", "shapes": tables["SHAPES"],
            "numbers": numbers}


def schema_shape(value):
    if value is None:
        return {"type": "null"}
    if isinstance(value, bool):
        return {"type": "boolean"}
    if isinstance(value, (float, int)):
        return {"type": "number"}
    if isinstance(value, str):
        return {"type": "string"}
    if isinstance(value, list):
        result = {"type": "array"}
        variants = {json.dumps(schema_shape(item), sort_keys=True): schema_shape(item) for item in value}
        if variants:
            result["items"] = next(iter(variants.values())) if len(variants) == 1 else {"anyOf": list(variants.values())}
        return result
    if set(value) == {"$number"}:
        return {"type": "object", "required": ["$number"], "additionalProperties": False,
                "properties": {"$number": {"enum": SPECIAL_NUMBERS}}}
    return {"type": "object", "required": list(value), "additionalProperties": False,
            "properties": {key: schema_shape(item) for key, item in value.items()}}


def same_json(left, right):
    """JSON equality keeps booleans distinct from numbers, unlike Python ==."""
    if isinstance(left, bool) or isinstance(right, bool):
        return type(left) is type(right) and left == right
    if isinstance(left, (int, float)) and isinstance(right, (int, float)):
        return left == right
    if type(left) is not type(right):
        return False
    if isinstance(left, dict):
        return left.keys() == right.keys() and all(same_json(left[key], right[key]) for key in left)
    if isinstance(left, list):
        return len(left) == len(right) and all(same_json(a, b) for a, b in zip(left, right))
    return left == right


def validate_shape(schema, value):
    """Validate the generated structural subset without installing dependencies."""
    if "const" in schema and not same_json(value, schema["const"]):
        raise ValueError("schema const mismatch")
    if "enum" in schema and not any(same_json(value, choice) for choice in schema["enum"]):
        raise ValueError("schema enum mismatch")
    if "anyOf" in schema:
        for choice in schema["anyOf"]:
            try:
                validate_shape(choice, value)
                return
            except ValueError:
                pass
        raise ValueError("schema anyOf mismatch")
    kind = schema.get("type")
    expected = {"null": value is None, "boolean": isinstance(value, bool),
                "number": isinstance(value, (int, float)) and not isinstance(value, bool),
                "string": isinstance(value, str), "array": isinstance(value, list),
                "object": isinstance(value, dict)}
    if kind and not expected[kind]:
        raise ValueError(f"schema type mismatch: {kind}")
    if kind == "object":
        properties = schema.get("properties", {})
        if not set(schema.get("required", [])).issubset(value):
            raise ValueError("schema required field absent")
        if schema.get("additionalProperties") is False and set(value) - set(properties):
            raise ValueError("schema unknown field")
        for key, item in value.items():
            if key in properties:
                validate_shape(properties[key], item)
    if kind == "array" and "items" in schema:
        for item in value:
            validate_shape(schema["items"], item)


def field_presence(objects):
    keys = sorted(set().union(*(set(value) for value in objects.values())))
    return {key: [name for name, value in objects.items() if key in value] for key in keys}


def path_inventory(value):
    paths = []

    def walk(item, path):
        kind = ("null" if item is None else "boolean" if isinstance(item, bool) else
                "number" if isinstance(item, (int, float)) else "string" if isinstance(item, str) else
                "array" if isinstance(item, list) else "object")
        paths.append({"path": path, "type": kind})
        if isinstance(item, dict):
            for key, child in item.items():
                walk(child, path + "/" + key.replace("~", "~0").replace("/", "~1"))
        elif isinstance(item, list):
            for index, child in enumerate(item):
                walk(child, path + "/" + str(index))

    walk(value, "")
    return paths


def build(repo, reference):
    if git(reference, "rev-parse", "HEAD").decode().strip() != REFERENCE:
        raise ValueError("wrong reference checkout pin")
    git(reference, "diff", "--exit-code", REFERENCE, "--", "shared")
    raw = git(repo, "show", BASELINE + ":" + CATALOG_PATH)
    if digest(raw) != CATALOG_SHA256:
        raise ValueError("baseline catalog bytes changed")
    catalog = json.loads(raw)
    tables = catalog["tables"]
    if catalog["format"] != 1 or catalog["reference"] != REFERENCE or len(tables) != 47:
        raise ValueError("unexpected compiled catalog")
    extras, locations = additions(reference)
    if set(tables).intersection(extras):
        raise ValueError("addition duplicates a compiled table")
    formed = formation(repo, tables)
    content = {"schemaVersion": 1, "reference": REFERENCE, "formation": formed,
               "tables": dict(sorted((tables | extras).items()))}
    if not all(content["tables"][key] == value for key, value in tables.items()):
        raise ValueError("original compiled table was changed")
    schema = schema_shape(content)
    schema.update({"$schema": "https://json-schema.org/draft/2020-12/schema",
                   "$id": "urn:ww2-rts:startup-tables:proposal:1",
                   "title": "Startup static tables proposal 1 at reference e4e579a"})
    schema["properties"]["schemaVersion"] = {"const": 1}
    schema["properties"]["reference"] = {"const": REFERENCE}
    for name in NAME_ARRAYS:
        schema["properties"]["tables"]["properties"][name] = {"const": content["tables"][name]}
    schema["properties"]["formation"] = {"const": formed}
    validate_shape(schema, content)
    paths = path_inventory(content)
    units = tables["UNITS"]
    inventory = {
        "baselineNative": BASELINE, "reference": REFERENCE, "catalogFormatActual": 1,
        "catalogByteSha256": digest(raw), "catalogTables": sorted(tables), "catalogTableCount": len(tables),
        "proposedTableCount": len(content["tables"]), "unitCount": len(units),
        "unitDefinitionFieldPresence": field_presence(units),
        "weaponFieldPresence": field_presence({name: unit["w"] for name, unit in units.items() if isinstance(unit.get("w"), dict)}),
        "abilityFieldPresence": field_presence({name: unit["ab"] for name, unit in units.items() if isinstance(unit.get("ab"), dict)}),
        "requiredClientStaticNames": CLIENT_STATIC, "authoringOnlyStaticNames": AUTHORING_STATIC,
        "additionalMapFormatMetadata": ["WORLD_VERSION", "OBJECT_TYPES", "BIOMES"],
        "missingFromCompiledCatalog": extras, "extraSourceLocations": locations,
        "deferredStaticNames": ["OPERATIVE", "OPERATIVE_WEAPONS"], "deadImport": ["BUILDABLE"], "paths": paths,
    }
    outputs = {"proposed-content.json": json_bytes(content), "proposed-content.schema.json": json_bytes(schema),
               "comparison-inventory.json": json_bytes(inventory)}
    manifest = {
        "status": "content/schema v1 candidate agreed; transport unagreed, no transport implemented", "baselineNative": BASELINE,
        "reference": REFERENCE, "proposedSchemaVersion": 1, "schemaFile": "proposed-content.schema.json",
        "schemaSha256": digest(outputs["proposed-content.schema.json"]), "contentFile": "proposed-content.json",
        "contentSha256": digest(outputs["proposed-content.json"]),
        "contentEncoding": "UTF-8, compact JSON, sorted object keys, one final LF; this fixture generator only",
        "formationHash": formed["hash"], "formationHashScope": "formation-only; unchanged existing ABI return",
        "compiledCatalogPath": CATALOG_PATH, "compiledCatalogSha256": digest(raw),
        "comparisonFile": "comparison-inventory.json", "comparisonSha256": digest(outputs["comparison-inventory.json"]),
        "verifiedCounts": {"compiledTables": 47, "proposalTables": len(content["tables"]), "units": len(units),
                           "supports": len(tables["SUPPORT"]), "forts": len(tables["FORTS"]),
                           "entrenchments": len(tables["ENTRENCH"]), "weatherKinds": len(tables["WEATHER_KINDS"]),
                           "allPayloadPaths": len(paths)},
        "notIncluded": ["seed", "match identity", "recipients", "private state", "live actors", "authoring map instances",
                        "operative movement/weapon tables"],
        "agreementNeeded": ["verified single Rust producer", "transport mechanism and ownership/lifetime transition",
                            "producer exact field/value and platform byte/hash evidence",
                            "published Unreal parser/cache and actual RTS/HUD consumption evidence"],
    }
    outputs["manifest.json"] = json_bytes(manifest)
    return outputs


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reference", type=Path, help="pinned read-only ww2-rts checkout; default: repository submodule")
    parser.add_argument("--check", action="store_true", help="compare generated bytes without writing")
    args = parser.parse_args()
    destination = Path(__file__).resolve().parent
    repo = destination.parents[2]
    reference = args.reference.resolve() if args.reference else repo / "reference/ww2-rts"
    outputs = build(repo, reference)
    for name, data in outputs.items():
        path = destination / name
        if args.check:
            if not path.exists() or path.read_bytes() != data:
                raise SystemExit(f"proposal artifact differs: {name}")
        else:
            path.write_bytes(data)
    print(f"{'Checked' if args.check else 'Generated'} 47 unchanged tables + 17 static additions; schema structure valid")


if __name__ == "__main__":
    main()
