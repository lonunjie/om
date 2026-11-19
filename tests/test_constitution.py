"""Tests for tools/constitution.py (stdlib unittest): python3 -m unittest discover -s tests"""
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tools"))
import constitution as c  # noqa: E402


def run(*args):
    return subprocess.run([sys.executable, str(ROOT / "tools/constitution.py"), *args], capture_output=True, text=True, cwd=ROOT)


class ResolveTests(unittest.TestCase):
    def test_numbered_and_named_headings_resolve(self):
        self.assertIn("Purpose", c.resolve("PHIL", "1")["heading"])
        self.assertTrue(c.resolve("PHIL", "P-02")["anchor"].startswith("p-02"))
        self.assertIn("Data", c.resolve("LONG", "15 Data")["heading"])
        self.assertIn("Evolution", c.resolve("LONG", "15 Evolution")["heading"])

    def test_number_prefix_is_not_a_prefix_of_longer_number(self):
        self.assertIn("Goal", c.resolve("OPLESS", "1")["heading"])  # not "10. ..."

    def test_missing_heading_fails(self):
        with self.assertRaises(ValueError):
            c.resolve("NAME", "No such section")

    def test_ambiguous_heading_fails(self):
        with self.assertRaises(ValueError):
            c.resolve("LONG", "15")


class CliTests(unittest.TestCase):
    def test_check_passes_on_repo(self):
        r = run("check")
        self.assertEqual(r.returncode, 0, r.stderr)

    def test_find_by_tag_and_keyword(self):
        r = run("find", "recovery")
        self.assertEqual(r.returncode, 0)
        self.assertIn("recovery", r.stdout.lower())
        self.assertIn("docs/constitution/", r.stdout)
        self.assertEqual(run("find", "zzzznomatch").returncode, 1)

    def test_find_full_prints_original_text(self):
        r = run("find", "criticality", "--full", "-n", "1")
        self.assertIn("---", r.stdout)
        self.assertLess(len(r.stdout), 12000)

    def test_show_with_source(self):
        r = run("show", "C-01", "--source")
        self.assertEqual(r.returncode, 0)
        self.assertIn("C-01", r.stdout)
        self.assertIn("source: docs/constitution/", r.stdout)
        self.assertEqual(run("show", "C-999").returncode, 1)

    def test_adr_search(self):
        r = run("adr", "ADR-0130")
        self.assertIn("ADR-0130", r.stdout)
        self.assertIn("decisions/ADR-0130", r.stdout)

    def test_json_matches_digest(self):
        data = json.loads((ROOT / "machine/constitution_index.json").read_text())
        ids = [x["id"] for x in data["rules"]]
        self.assertEqual(len(ids), len(set(ids)))
        for rule in data["rules"]:
            self.assertTrue(rule["source"] and rule["tags"])


class DriftTests(unittest.TestCase):
    def test_duplicate_id_and_broken_source_are_reported(self):
        orig = c.DIGEST
        tmp = Path(tempfile.mkdtemp()) / "digest.md"
        try:
            tmp.write_text("## G {tags: x}\n\n- **C-01 A.** One. [NAME §0M]\n- **C-01 B.** Two. [NAME §Missing heading]\n")
            c.DIGEST = tmp
            _, errors = c.build_constitution()
            self.assertTrue(any("duplicate" in e for e in errors))
            self.assertTrue(any("no such heading" in e for e in errors))
        finally:
            c.DIGEST = orig
            tmp.unlink(missing_ok=True)
            tmp.parent.rmdir()


class BriefGateTests(unittest.TestCase):
    def gate(self, *a):
        return subprocess.run([sys.executable, str(ROOT / "tools/design_gate.py"), "needs", *a], capture_output=True, text=True, cwd=ROOT).stdout

    def test_brief_replaces_only_constitution_docs(self):
        full, brief = self.gate("infra/x"), self.gate("--brief", "infra/x")
        self.assertIn("docs/constitution/NAMING.md", full)
        self.assertNotIn("docs/constitution/NAMING.md", brief)
        self.assertIn("CONSTITUTION_DIGEST.md", brief)
        self.assertEqual(full.split("After reading")[1], brief.split("After reading")[1])


if __name__ == "__main__":
    unittest.main()
