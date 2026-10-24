"""Offline regression coverage for bounded repository API requests."""
from pathlib import Path
import sys
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "scripts"))
import cleanup_v022_branches as cleanup


class APIPathTests(unittest.TestCase):
    def accepted(self, path):
        with patch.object(cleanup.urllib.request, "urlopen") as opened:
            opened.return_value.__enter__.return_value.read.return_value = b'{"ok": true}'
            self.assertEqual(cleanup.GitHub("test-token").get(path), {"ok": True})
            opened.assert_called_once()
            request = opened.call_args.args[0]
            self.assertEqual(request.full_url, "https://api.github.com/repos/francis-du/maris" + path)
            self.assertEqual(request.host, "api.github.com")
            self.assertEqual(opened.call_args.kwargs, {"timeout": 20})

    def rejected(self, path):
        with patch.object(cleanup.urllib.request, "urlopen") as opened:
            with self.assertRaises((ValueError, UnicodeError)):
                cleanup.GitHub("test-token").get(path)
            opened.assert_not_called()

    def test_exact_same_repo_compare_reaches_request_construction(self):
        self.accepted("/compare/" + "a" * 40 + "..." + "b" * 40 + "?per_page=1")

    def test_actual_cleanup_routes_remain_usable(self):
        for path in (
            "", "/releases/tags/v0.2.2", "/releases/tags/v0.2.3", "/releases/latest",
            "/commits/v0.2.3", "/git/ref/heads/main", "/branches?per_page=100&page=1",
            "/pulls?state=open&per_page=100&page=1",
            "/releases/422476583/assets?per_page=100&page=1",
            "/actions/runs/40055889860",
            "/actions/runs/40055889860/jobs?filter=latest&per_page=100&page=1",
            "/actions/runs?status=in_progress&per_page=100&page=1",
            "/rules/branches/release-validation%2Fv0.2.2-a21e0978cb56",
        ):
            with self.subTest(path=path):
                self.accepted(path)

    def test_raw_encoded_and_double_encoded_traversal_never_requests(self):
        for path in (
            "/../other", "/actions/../other", "/./actions", "/actions/./runs",
            "/%2e%2e/other", "/actions/%2E%2E/other", "/%252e%252e/other",
            "/actions%2F..%2Fother", "/%2f..%2fother", "/%252f..%252fother",
            "/rules/branches/feat%2F..%2Fother", "/%5c..%5cother",
            "/actions/%00/runs", "/actions/%09/runs", "/actions/%3Fruns",
            "/actions/%23runs", "/actions/%25runs", "/actions/%/runs",
        ):
            with self.subTest(path=path):
                self.rejected(path)

    def test_foreign_authority_fragments_controls_and_backslashes_never_request(self):
        for path in (
            "https://api.github.com/repos/other/repo", "https://evil.example/",
            "//evil.example/path", "/%2Fevil.example/path", "/actions//runs",
            "/actions#fragment", "/actions#", "/actions\\..\\other", "\n/actions",
            "/actions\r\nX-Test: yes", "/actions\t/runs", "/actions/with space",
            "/actions/\x7f", "actions/runs", None, 123, "/" + "x" * 1024,
        ):
            with self.subTest(path=path):
                self.rejected(path)

    def test_compare_refs_and_queries_are_bounded(self):
        for path in (
            "/compare/main...other", "/compare/" + "a" * 39 + "..." + "b" * 40,
            "/compare/" + "A" * 40 + "..." + "b" * 40,
            "/compare/" + "a" * 40 + "%2E%2E%2E" + "b" * 40,
            "/branches?per_page=101", "/branches?page=21", "/branches?page=0",
            "/branches?page=-1", "/branches?page=01", "/branches?per_page=1&per_page=2",
            "/branches?next=https://evil.example", "/branches?%70age=1",
            "/branches?page=%31", "/branches?", "/branches?filter=all",
        ):
            with self.subTest(path=path):
                self.rejected(path)


if __name__ == "__main__":
    unittest.main()
