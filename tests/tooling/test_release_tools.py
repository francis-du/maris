"""Documentation, packaging-policy and evidence-gate tests; no network or live audio."""
from pathlib import Path
import hashlib
import importlib.util
import json
import re
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))
import source_audit
import release_gate
from verify_rounds import SUITES, test_passed
from release_requirements import REQUIRED, MANIFEST
SPEC = importlib.util.spec_from_file_location("maris_site", ROOT / "scripts/site.py")
site = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(site)


class ReleaseTools(unittest.TestCase):
    def test_real_documentation_links_and_accessible_structure(self):
        files = site.check(ROOT / "docs")
        self.assertEqual(len(files), 101)
        self.assertEqual(sum(name.endswith('.html') for name in files), 77)
        for language in ('en', 'zh-CN', 'zh-TW', 'ja', 'de', 'es'):
            for page in ('index', 'guide', 'install', 'status'):
                self.assertIn(f'{language}/{page}.html', files)
        self.assertIn('assets/mark.svg', files)

    def test_site_rejects_project_base_escape_and_unexpected_files(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            (root / "index.html").write_text('<title>X</title><meta name="viewport"><main><a href="/install.html">X</a></main>')
            with self.assertRaises(ValueError):
                site.check(root)
            (root / "index.html").write_text('<title>X</title><meta name="viewport"><main id="main">X</main>')
            (root / "secret.txt").write_text("not part of a static site")
            with self.assertRaises(ValueError):
                site.check(root)

    def test_build_and_push_workflows_are_separate_and_actions_are_pinned(self):
        workflows = ROOT / ".github/workflows"
        checks = (workflows / "ci.yml").read_text()
        build = (workflows / "build.yml").read_text()
        pages = (workflows / "pages.yml").read_text()
        self.assertIn("  push:", checks)
        self.assertNotIn("cargo build --release", checks)
        self.assertNotIn("upload-artifact@", checks)
        self.assertNotIn("  push:", build)
        self.assertNotIn("contents: write", build)
        self.assertIn("workflow_dispatch:", build)
        self.assertIn("--rounds 200", build)
        self.assertIn("needs: build", pages)
        self.assertIn("pages: write", pages)
        self.assertIn("id-token: write", pages)
        for path in workflows.glob("*.yml"):
            for action in re.findall(r"uses:\s+(\S+)", path.read_text()):
                self.assertRegex(action, r"^[^@]+@[a-f0-9]{40}$")

    def test_report_rejects_insufficient_or_modified_records(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            data = b"running 1 test\ntest result: ok. 1 passed; 0 failed; 0 ignored;\n"
            records = []
            for i in range(1, 201):
                steps = []
                for suite in ["release_matrix", SUITES[(i - 1) % len(SUITES)]]:
                    name = f"round-{i:03d}-{suite}.log"
                    (root / name).write_bytes(data)
                    steps.append({"suite": suite, "log": name, "exit_code": 0,
                                  "passed": True, "log_sha256": hashlib.sha256(data).hexdigest()})
                records.append({"round": i, "seed": i, "passed": True, "steps": steps})
            report = {"source_sha256": "fixture", "source_unchanged": True, "passed": True,
                      "completed_rounds": 200, "requested_rounds": 200, "passed_rounds": 200,
                      "records": records}
            release_gate.check_report(report, "fixture", root)
            bad = dict(report, completed_rounds=199)
            with self.assertRaises(ValueError):
                release_gate.check_report(bad, "fixture", root)
            with self.assertRaises(ValueError):
                release_gate.check_report(report, "changed-source", root)
            (root / "round-001-release_matrix.log").write_text("modified")
            with self.assertRaises(ValueError):
                release_gate.check_report(report, "fixture", root)

    def test_final_bundle_digest_changes_with_payload_or_permission_changes(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            contents = root / "Maris.app/Contents"
            contents.mkdir(parents=True)
            payload = contents / "fixture"
            payload.write_bytes(b"first")
            first = release_gate.bundle_digest(root / "Maris.app")
            self.assertEqual(first, release_gate.bundle_digest(root / "Maris.app"))
            payload.write_bytes(b"second")
            self.assertNotEqual(first, release_gate.bundle_digest(root / "Maris.app"))
            with self.assertRaises(ValueError):
                release_gate.bundle_digest(root / "missing.app")

    def test_pass_parser_never_counts_empty_filtered_or_ignored_runs_as_success(self):
        self.assertTrue(test_passed(b"test result: ok. 10 passed; 0 failed; 0 ignored;", 0))
        self.assertFalse(test_passed(b"test result: ok. 0 passed; 0 failed; 0 ignored;", 0))
        self.assertFalse(test_passed(b"test result: ok. 10 passed; 0 failed; 1 ignored;", 0))
        self.assertFalse(test_passed(b"test result: ok. 10 passed; 0 failed; 0 ignored;", 1))
        self.assertFalse(test_passed(b"no test execution", 0))

    def test_acceptance_requires_reviewed_local_evidence_for_every_gate(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder).resolve()
            with self.assertRaises(ValueError):
                release_gate.check_approval({}, "fixture", root)
            evidence = root / "acceptance.txt"
            evidence.write_text("isolated test fixture, not product acceptance")
            item = {"accepted": True, "path": "acceptance.txt",
                    "sha256": hashlib.sha256(evidence.read_bytes()).hexdigest()}
            approval = {"platform": "macos", "source_sha256": "fixture", "reviewed_by": "Fixture reviewer",
                        "bundle_sha256": "a" * 64,
                        **{name: item for name in release_gate.EVIDENCE}}
            # A complete old-style five-gate packet no longer erases unfinished feature work.
            with self.assertRaises(ValueError):
                release_gate.check_approval(approval, "fixture", root)
            manifest = root / MANIFEST
            manifest.parent.mkdir(parents=True)
            records = [{'id': key, 'status': 'in_progress'} for key in REQUIRED]
            fixture = {'schema_version': 1, 'policy': 'all_requested_work_before_release', 'requirements': records}
            manifest.write_text(json.dumps(fixture))
            approval['requirements'] = {key: item for key in REQUIRED}
            with self.assertRaises(ValueError):
                release_gate.check_approval(approval, "fixture", root)
            for record in records:
                record['status'] = 'verified'
            manifest.write_text(json.dumps(fixture))
            release_gate.check_approval(approval, "fixture", root)
            approval["listening_validation"] = dict(item, accepted=False)
            with self.assertRaises(ValueError):
                release_gate.check_approval(approval, "fixture", root)

    def test_private_keys_and_tokens_are_detected_without_echoing_values(self):
        for value in ["ghp_" + "a" * 40, "-----BEGIN " + "OPENSSH PRIVATE KEY-----"]:
            self.assertTrue(any(p.search(value) for p in source_audit.SECRET_PATTERNS))
        files = source_audit.source_files()
        self.assertTrue(files)
        canonical_design = {ROOT / '.wcode/project.yaml'} | set((ROOT / '.wcode/design').glob('*.yaml'))
        self.assertTrue(all('.wcode' not in path.parts or path.resolve() in canonical_design for path in files))
        self.assertFalse(any(any(part in path.parts for part in ('target','dist','_site','.maris-review')) for path in files))


if __name__ == "__main__":
    unittest.main()
