#!/usr/bin/env python3
"""Generate a sanitized draft envelope from accepted static bytes, not a runtime trace."""
import argparse
import hashlib
import json
from pathlib import Path

EXPECTED = "5de90a7990ec5e8a2903436ac4809302464be29b546d79591a2adc08c68a2efd"
MAX_CONTENT = 1_048_576
MAX_FRAME = 2_097_664

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    folder = Path(__file__).resolve().parent
    content = (folder.parent / "proposed-content.json").read_bytes()
    digest = hashlib.sha256(content).hexdigest()
    if digest != EXPECTED or not content.endswith(b"\n") or content.endswith(b"\n\n"):
        raise SystemExit("accepted content identity changed")
    envelope = {"t": "tables", "schemaVersion": 1, "contentSha256": digest, "json": content.decode("utf-8")}
    # This LF is example-file formatting, not an additional table-content byte.
    frame = json.dumps(envelope, ensure_ascii=False, sort_keys=True, separators=(",", ":")).encode("utf-8")
    if len(content) > MAX_CONTENT or len(frame) > MAX_FRAME:
        raise SystemExit("draft bounds exceeded")
    if json.loads(frame)["json"].encode("utf-8") != content:
        raise SystemExit("envelope changed the inner bytes")
    output = folder / "example-tables.json"
    data = frame + b"\n"
    if args.check:
        if output.read_bytes() != data:
            raise SystemExit("draft example differs")
    else:
        output.write_bytes(data)
    print(f"{'Checked' if args.check else 'Generated'} draft: {len(content)} inner bytes, {len(frame)} frame bytes, SHA256 {digest}")

if __name__ == "__main__":
    main()
