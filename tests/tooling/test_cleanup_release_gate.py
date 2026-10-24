"""Synthetic release fixtures reject evidence mixed across versions."""
from pathlib import Path
import sys
import unittest
from unittest.mock import Mock

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "scripts"))
import cleanup_v022_branches as cleanup

SOURCE = "1" * 40
STAMP = "2026-10-24T00:00:00Z"


def release_fixture():
    names = [
        "Maris-0.2.3-macos-x86_64.tar.gz", "Maris-0.2.3-macos-arm64.tar.gz",
        "Maris-0.2.3-linux-x86_64.tar.gz", "Maris-0.2.3-linux-arm64.tar.gz",
        "Maris-0.2.3-windows-x86_64.zip", "Maris-0.2.3-windows-arm64.zip",
        "maris-release.tsv", "SHA256SUMS",
    ]
    data = {
        "release": {"id": 23, "tag_name": "v0.2.3", "draft": False,
                    "prerelease": False, "published_at": STAMP,
                    "html_url": "https://example.invalid/release/v0.2.3"},
        "latest": {"id": 23}, "tag_commit": {"sha": SOURCE},
        "main": {"object": {"sha": SOURCE}},
        "assets": [{"id": i, "name": name, "state": "uploaded", "size": 1,
                    "updated_at": STAMP} for i, name in enumerate(names, 1)],
        "run": {"repository": {"full_name": "francis-du/maris"},
                "path": ".github/workflows/publish-release.yml", "event": "push",
                "head_branch": "release-publish/v0.2.3", "head_sha": SOURCE,
                "status": "completed", "conclusion": "success", "run_attempt": 1},
        "jobs": [],
    }
    for platform, installer in (
        ("macos-15", "install.sh"), ("macos-15-intel", "install.sh"),
        ("ubuntu-24.04", "install.sh"), ("ubuntu-24.04-arm", "install.sh"),
        ("windows-2025", "install.ps1"), ("windows-11-arm", "install.ps1"),
    ):
        for bootstrap in ("release", "public"):
            data["jobs"].append({
                "id": len(data["jobs"]) + 100,
                "name": f"assets / public-install-smoke ({platform}, {bootstrap})",
                "head_sha": SOURCE, "status": "completed", "conclusion": "success",
                "labels": [platform], "html_url": "https://example.invalid/job",
                "steps": [{"name": "Install from the public Release with " + installer,
                           "conclusion": "success"}],
            })
    data["jobs"].append({
        "id": 199, "name": "assets / promote-stable", "head_sha": SOURCE,
        "status": "completed", "conclusion": "success",
        "steps": [{"name": "Promote smoke-tested prerelease to stable",
                   "conclusion": "success"}],
    })
    get_routes = {
        "/releases/tags/v0.2.3": "release", "/releases/latest": "latest",
        "/commits/v0.2.3": "tag_commit", "/git/ref/heads/main": "main",
        "/actions/runs/2301": "run",
    }
    collections = {
        ("/releases/23/assets", None): "assets",
        ("/actions/runs/2301/jobs?filter=latest", "jobs"): "jobs",
    }
    api = Mock(spec=cleanup.GitHub)
    api.get.side_effect = lambda path: data[get_routes[path]]
    api.collection.side_effect = lambda path, key=None: data[collections[(path, key)]]
    return api, data


class ReleaseVersionGateTests(unittest.TestCase):
    def test_explicit_v023_uses_its_assets_run_and_complete_install_matrix(self):
        api, data = release_fixture()
        result = cleanup.release_gate(api, SOURCE, 23, 2301, tag="v0.2.3")
        self.assertEqual(result["tag"], "v0.2.3")
        self.assertEqual(result["source_sha"], SOURCE)
        self.assertEqual(len(result["native_install_jobs"]), 12)
        self.assertEqual(result["promotion_job"], 199)
        self.assertEqual({a["name"] for a in result["assets"]},
                         {a["name"] for a in data["assets"]})
        api.get.assert_any_call("/releases/tags/v0.2.3")
        api.get.assert_any_call("/commits/v0.2.3")
        api.collection.assert_any_call("/actions/runs/2301/jobs?filter=latest", "jobs")

    def test_cross_version_or_source_evidence_is_rejected(self):
        cases = (
            (("assets", 0, "name"), "Maris-0.2.2-macos-x86_64.tar.gz", "eight expected"),
            (("run", "head_branch"), "release-publish/v0.2.2", "Publication workflow"),
            (("tag_commit", "sha"), "2" * 40, "Release tag source changed"),
            (("latest", "id"), 22, "not latest"),
            (("release", "tag_name"), "v0.2.2", "Release identity differs"),
            (("main", "object", "sha"), "2" * 40, "Main changed"),
        )
        for path, value, message in cases:
            api, data = release_fixture()
            target = data
            for component in path[:-1]:
                target = target[component]
            target[path[-1]] = value
            with self.subTest(path=path), self.assertRaisesRegex(ValueError, message):
                cleanup.release_gate(api, SOURCE, 23, 2301, tag="v0.2.3")

    def test_invalid_or_unbounded_tag_is_rejected_before_any_request(self):
        for tag in (None, 23, "", "0.2.3", "v0.2.3/other", "v0.2.3?x=1",
                    "v0.2.3\n", "v" + "1" * 64 + ".2.3"):
            api, _ = release_fixture()
            with self.subTest(tag=tag), self.assertRaisesRegex(ValueError, "Invalid release tag"):
                cleanup.release_gate(api, SOURCE, 23, 2301, tag=tag)
            api.get.assert_not_called()
            api.collection.assert_not_called()


if __name__ == "__main__":
    unittest.main()
