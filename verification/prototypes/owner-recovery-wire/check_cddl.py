#!/usr/bin/env python3
"""Check the ADR-0203 CDDL appendix against the same wire fixtures."""

from collections import defaultdict
import json
from pathlib import Path
import subprocess
import sys

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[2]
sys.path.insert(0, str(REPO / "verification/prototypes/v005-encoding/gen"))
import cddl_check as checks


def main(path, scratch):
    adr = (REPO / "decisions/ADR-0203-owner-recovery-carriage.md").read_text()
    schema = "\n".join(
        (REPO / f"spec/cddl/{name}.cddl").read_text()
        for name in ["common", "account", "recovery-notice"]
    )
    schema += "\n" + adr.split("```cddl\n", 1)[1].split("```", 1)[0]
    by_type = defaultdict(list)
    for vector in json.loads(Path(path).read_text())["vectors"]:
        by_type[vector["object"]].append(vector)
    total = 0
    for name, vectors in by_type.items():
        # Both tools check a CDDL root supplied as the first rule.
        work = scratch / name
        work.mkdir()
        spec = work / "spec.cddl"
        spec.write_text(f"root = {name}\n{schema}")
        files = []
        for index, vector in enumerate(vectors):
            fixture = work / f"{index}.cbor"
            fixture.write_bytes(bytes.fromhex(vector["hex"]))
            files.append(fixture)
        try:
            results = [("Rust", checks.rust_validate(spec, files)), ("Ruby", checks.ruby_validate(spec, files))]
        except subprocess.CalledProcessError as exc:
            raise SystemExit(exc.stderr) from exc
        for tool, result in results:
            for file, vector in zip(files, vectors):
                expected = vector["expect"] == "accept"
                assert result[file.name] == expected, f"{tool} CDDL verdict differs for {vector['id']}"
                total += 1
    print(f"ADR-0203 CDDL: {total} fixture verdicts match (Rust and Ruby)")


if __name__ == "__main__":
    main(sys.argv[1], Path(sys.argv[2]))
