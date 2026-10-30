"""Offline safety contracts for the explicitly scoped v0.2.4 cleanup.

Every API response is synthetic, and any Git command is mocked. These tests do
not connect to GitHub, exercise a credential, or delete a local/remote reference.
"""
from __future__ import annotations

import copy
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
from types import SimpleNamespace
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))
import cleanup_v024_branches as cleanup
import cleanup_v022_branches as legacy

SOURCE = "a" * 40
MERGE = "b" * 40
OTHER = "c" * 40
RELEASE_ID = 24
PUBLICATION_ID = 240
CURRENT_RUN_ID = 9000


def ancestor(sha):
    return {"status": "identical" if sha == SOURCE else "behind", "ahead_by": 0,
            "behind_by": 0 if sha == SOURCE else 1, "merge_base_commit": {"sha": sha}}


class FakeAPI:
    def __init__(self):
        release = {"id": RELEASE_ID, "tag_name": cleanup.TAG, "draft": False,
                   "prerelease": False, "published_at": "2026-10-30T00:00:00Z",
                   "html_url": "https://github.com/francis-du/maris/releases/tag/v0.2.4"}
        run = {"id": PUBLICATION_ID, "repository": {"full_name": legacy.REPOSITORY},
               "head_repository": {"full_name": legacy.REPOSITORY},
               "name": "Publish stable release", "path": legacy.PUBLISH_PATH,
               "event": "push", "head_branch": "release-publish/v0.2.4",
               "head_sha": SOURCE, "status": "completed", "conclusion": "success",
               "run_attempt": 1}
        pull = {"number": 96, "state": "closed", "merged": True, "merge_commit_sha": MERGE,
                "base": {"ref": "main", "repo": {"full_name": legacy.REPOSITORY}},
                "head": {"ref": "fix/coreaudio-startup-20261030", "sha": MERGE,
                         "repo": {"full_name": legacy.REPOSITORY}}}
        self.rows = {
            "/releases/tags/v0.2.4": release,
            "/releases/latest": copy.deepcopy(release),
            "/commits/v0.2.4": {"sha": SOURCE},
            "/git/ref/heads/main": {"object": {"sha": SOURCE}},
            f"/actions/runs/{PUBLICATION_ID}": run,
            "/pulls/96": pull,
            f"/compare/{SOURCE}...{cleanup.REVIEWED_MAIN}?per_page=1":
                ancestor(cleanup.REVIEWED_MAIN),
            f"/compare/{SOURCE}...{MERGE}?per_page=1": ancestor(MERGE),
            f"/compare/{SOURCE}...{SOURCE}?per_page=1": ancestor(SOURCE),
            f"/compare/{SOURCE}...{OTHER}?per_page=1":
                {"status": "diverged", "ahead_by": 1, "behind_by": 1,
                 "merge_base_commit": {"sha": cleanup.REVIEWED_MAIN}},
        }
        names = ["maris-release.tsv", "SHA256SUMS"]
        for system in ("macos", "linux", "windows"):
            for arch in ("x86_64", "arm64"):
                extension = ".zip" if system == "windows" else ".tar.gz"
                names.append(f"Maris-0.2.4-{system}-{arch}{extension}")
        assets = [{"id": index + 1, "name": name, "state": "uploaded", "size": 100,
                   "updated_at": "2026-10-30T00:00:00Z"} for index, name in enumerate(names)]
        jobs = []
        for platform in sorted(legacy.PLATFORMS):
            for bootstrap in ("release", "public"):
                step = ("Install from the public Release with install.ps1"
                        if platform.startswith("windows-")
                        else "Install from the public Release with install.sh")
                jobs.append({"id": len(jobs) + 1,
                             "name": f"assets / public-install-smoke ({platform}, {bootstrap})",
                             "head_sha": SOURCE, "status": "completed", "conclusion": "success",
                             "labels": [platform], "steps": [{"name": step, "conclusion": "success"}],
                             "html_url": "https://github.com/francis-du/maris/actions/jobs/1"})
        jobs.append({"id": 99, "name": "assets / promote-stable", "head_sha": SOURCE,
                     "conclusion": "success",
                     "steps": [{"name": "Promote smoke-tested prerelease to stable",
                                "conclusion": "success"}]})
        self.collections = {
            f"/releases/{RELEASE_ID}/assets": assets,
            f"/actions/runs/{PUBLICATION_ID}/jobs?filter=latest": jobs,
            "/branches": [branch("main", SOURCE)],
        }
        self.requests = []

    def get(self, path):
        legacy.validate_api_path(path)
        self.requests.append(path)
        if path.startswith("/rules/branches/"):
            return copy.deepcopy(self.rows.get(path, []))
        return copy.deepcopy(self.rows[path])

    def collection(self, path, key=None):
        legacy.validate_api_path(path)
        self.requests.append(path)
        return copy.deepcopy(self.collections[path])


def branch(name, sha, protected=False):
    return {"name": name, "commit": {"sha": sha}, "protected": protected}


def inventory(branches, active=None, pulls=None):
    return {"branches": branches, "active_runs": active or [], "open_pulls": pulls or []}


def plan():
    return json.loads((ROOT / "scripts/cleanup_v024_branches.json").read_text())


def args():
    return SimpleNamespace(source_sha=SOURCE, release_id=RELEASE_ID,
                           publication_run_id=PUBLICATION_ID)


def current_cleanup(api):
    run = {"id": CURRENT_RUN_ID, "run_attempt": 2, "path": cleanup.CLEANUP_PATH,
           "event": "workflow_run", "head_branch": "main", "head_sha": SOURCE,
           "status": "in_progress", "repository": {"full_name": legacy.REPOSITORY},
           "head_repository": {"full_name": legacy.REPOSITORY}}
    api.rows[f"/actions/runs/{CURRENT_RUN_ID}"] = copy.deepcopy(run)
    return run


class CleanupV024Tests(unittest.TestCase):
    def test_plan_scope_cannot_expand_or_relabel_old_review(self):
        cleanup.validate_plan(plan())
        edits = [
            lambda item: item.update(tag_precondition="v0.2.2"),
            lambda item: item.update(required_merged_pr=95),
            lambda item: item["ancestor_refs"].append("feature/unreviewed"),
            lambda item: item["fixed_refs"][0].update(sha=OTHER),
        ]
        for edit in edits:
            selected = plan()
            edit(selected)
            with self.subTest(edit=edit), self.assertRaises(ValueError):
                cleanup.validate_plan(selected)

    def test_publication_requires_all_twelve_real_native_jobs_and_promotion(self):
        api = FakeAPI()
        result = cleanup.publication_gate(api, SOURCE, RELEASE_ID, PUBLICATION_ID)
        self.assertEqual(result["tag"], "v0.2.4")
        self.assertEqual(len(result["native_install_jobs"]), 12)
        api.collections[f"/actions/runs/{PUBLICATION_ID}/jobs?filter=latest"].pop(0)
        with self.assertRaises(ValueError):
            cleanup.publication_gate(api, SOURCE, RELEASE_ID, PUBLICATION_ID)

    def test_old_version_stale_main_and_automatic_asset_runs_are_rejected(self):
        cases = [
            ("/releases/tags/v0.2.4", "tag_name", "v0.2.3"),
            ("/releases/latest", "id", RELEASE_ID + 1),
            ("/commits/v0.2.4", "sha", OTHER),
            (f"/actions/runs/{PUBLICATION_ID}", "head_branch", "release-publish/v0.2.3"),
            (f"/actions/runs/{PUBLICATION_ID}", "path", legacy.RELEASE_PATH),
            (f"/actions/runs/{PUBLICATION_ID}", "event", "workflow_run"),
            (f"/actions/runs/{PUBLICATION_ID}", "conclusion", "failure"),
        ]
        for path, key, value in cases:
            api = FakeAPI()
            api.rows[path][key] = value
            with self.subTest(path=path, key=key), self.assertRaises(ValueError):
                cleanup.publication_gate(api, SOURCE, RELEASE_ID, PUBLICATION_ID)
        api = FakeAPI()
        api.rows["/git/ref/heads/main"]["object"]["sha"] = OTHER
        with self.assertRaises(ValueError):
            cleanup.publication_gate(api, SOURCE, RELEASE_ID, PUBLICATION_ID)

    def test_pr96_must_be_merged_into_the_published_source(self):
        api = FakeAPI()
        self.assertEqual(cleanup.review_gate(api, SOURCE)["required_pr"], 96)
        api.rows["/pulls/96"]["merged"] = False
        with self.assertRaises(ValueError):
            cleanup.review_gate(api, SOURCE)
        api = FakeAPI()
        api.rows["/pulls/96"]["merge_commit_sha"] = OTHER
        with self.assertRaises(ValueError):
            cleanup.review_gate(api, SOURCE)

    def test_named_dynamic_refs_are_observed_once_and_unpublished_work_is_held(self):
        api = FakeAPI()
        current = inventory([
            branch("main", SOURCE),
            branch("fix/coreaudio-startup-20261030", MERGE),
            branch("docs-refresh/v0.2.4", OTHER),
            branch("release-publish/v0.2.4", SOURCE),
            branch("feature/unreviewed", MERGE),
        ])
        frozen, held, proofs = cleanup.materialize_refs(api, plan(), current, SOURCE)
        selected = {item["name"]: item["sha"] for item in frozen["branches"]}
        self.assertEqual(selected["fix/coreaudio-startup-20261030"], MERGE)
        self.assertEqual(selected["release-publish/v0.2.4"], SOURCE)
        self.assertNotIn("feature/unreviewed", selected)
        self.assertNotIn("docs-refresh/v0.2.4", selected)
        self.assertEqual({item["name"] for item in held},
                         {"docs-refresh/v0.2.4", "release-validation/v0.2.4"})
        self.assertEqual(len(proofs), 3)
        current["branches"][1]["commit"]["sha"] = OTHER
        self.assertEqual(selected["fix/coreaudio-startup-20261030"], MERGE)

    def test_protection_rules_open_prs_and_active_sha_aliases_remain_gates(self):
        name, sha = next(iter(cleanup.FIXED_REFS.items()))
        frozen = {"branches": [{"name": name, "sha": sha,
                                "disposition": "candidate_after_successful_new_release",
                                "reason": "fixture reviewed exact head"}]}
        cases = [
            inventory([branch(name, sha, protected=True)]),
            inventory([branch(name, OTHER)]),
            inventory([branch(name, sha)],
                      active=[{"head_branch": "different-name", "head_sha": sha}]),
            inventory([branch(name, sha)], pulls=[
                {"head": {"ref": name, "sha": sha, "repo": {"full_name": legacy.REPOSITORY}},
                 "base": {"ref": "main", "sha": SOURCE,
                          "repo": {"full_name": legacy.REPOSITORY}}}]),
        ]
        for current in cases:
            eligible, held = legacy.select_refs(FakeAPI(), frozen, current)
            self.assertFalse(eligible)
            self.assertEqual(len(held), 1)
        api = FakeAPI()
        api.rows["/rules/branches/" + legacy.urllib.parse.quote(name, safe="")] = [
            {"type": "deletion"}
        ]
        eligible, held = legacy.select_refs(api, frozen, inventory([branch(name, sha)]))
        self.assertFalse(eligible)
        self.assertEqual(held[0]["reason"], "branch_deletion_rule")

    def test_changed_ref_after_dry_run_never_attempts_actual_push(self):
        name, sha = next(iter(cleanup.FIXED_REFS.items()))
        frozen = {"branches": [{"name": name, "sha": sha,
                                "disposition": "candidate_after_successful_new_release",
                                "reason": "fixture reviewed exact head"}]}
        eligible = [{"name": name, "expected_sha": sha}]
        report = {"deletion_attempted": False, "deleted_refs": []}
        api = FakeAPI()
        changed = inventory([branch(name, OTHER)])
        with mock.patch.object(legacy, "live_inventory", return_value=changed), \
                mock.patch.object(subprocess, "run",
                                  return_value=subprocess.CompletedProcess([], 0, "", "")) as run:
            with self.assertRaises(ValueError):
                cleanup.execute_cleanup(api, "offline-fixture", args(), frozen, eligible, report)
        pushes = [call.args[0] for call in run.call_args_list if call.args[0][:2] == ["git", "push"]]
        self.assertEqual(len(pushes), 1)
        self.assertIn("--dry-run", pushes[0])
        self.assertFalse(report["deletion_attempted"])

    def test_only_the_authenticated_current_run_is_removed_from_selection_view(self):
        api = FakeAPI()
        own = current_cleanup(api)
        other = {**own, "id": CURRENT_RUN_ID + 1, "path": ".github/workflows/ci.yml"}
        current = inventory([branch("release-publish/v0.2.4", SOURCE)], active=[own, other])
        frozen = {"branches": [{"name": "release-publish/v0.2.4", "sha": SOURCE,
                                "disposition": "candidate_after_successful_new_release",
                                "reason": "fixture published source"}]}
        with mock.patch.dict(os.environ, {"GITHUB_RUN_ID": str(CURRENT_RUN_ID),
                                          "GITHUB_RUN_ATTEMPT": "2"}), \
                mock.patch.object(cleanup, "validate_execution_event") as event:
            view, receipt = cleanup.selection_view(api, current, SOURCE, PUBLICATION_ID, True)
        event.assert_called_once_with(SOURCE, PUBLICATION_ID)
        self.assertEqual(receipt["excluded_cleanup_run_id"], CURRENT_RUN_ID)
        self.assertEqual(view["active_runs"], [other])
        self.assertEqual(current["active_runs"], [own, other])
        eligible, held = legacy.select_refs(api, frozen, view)
        self.assertFalse(eligible)
        self.assertEqual(held[0]["reason"], "active_workflow_or_same_commit_alias")

    def test_current_run_identity_mismatches_never_create_an_activity_exception(self):
        cases = [("id", CURRENT_RUN_ID + 1), ("run_attempt", 3),
                 ("path", ".github/workflows/cleanup-v022-branches.yml"),
                 ("head_sha", OTHER), ("head_branch", "release-publish/v0.2.4"),
                 ("event", "push"), ("status", "completed"),
                 ("repository", {"full_name": "other/repository"}),
                 ("head_repository", {"full_name": "other/repository"})]
        for key, value in cases:
            api = FakeAPI()
            own = current_cleanup(api)
            current = inventory([], active=[own])
            api.rows[f"/actions/runs/{CURRENT_RUN_ID}"][key] = value
            with self.subTest(key=key), \
                    mock.patch.dict(os.environ, {"GITHUB_RUN_ID": str(CURRENT_RUN_ID),
                                                 "GITHUB_RUN_ATTEMPT": "2"}), \
                    mock.patch.object(cleanup, "validate_execution_event"):
                view, receipt = cleanup.selection_view(api, current, SOURCE, PUBLICATION_ID, True)
            self.assertIs(view, current)
            self.assertIsNone(receipt["excluded_cleanup_run_id"])
            self.assertEqual(receipt["reason"], "current_cleanup_not_authenticated")

    def test_unvalidated_event_and_read_only_review_preserve_the_whole_inventory(self):
        api = FakeAPI()
        current = inventory([], active=[current_cleanup(api)])
        with mock.patch.object(cleanup, "validate_execution_event",
                               side_effect=ValueError("fixture wrong publisher event")):
            view, receipt = cleanup.selection_view(api, current, SOURCE, PUBLICATION_ID, True)
        self.assertIs(view, current)
        self.assertIsNone(receipt["excluded_cleanup_run_id"])
        view, receipt = cleanup.selection_view(api, current, SOURCE, PUBLICATION_ID, False)
        self.assertIs(view, current)
        self.assertEqual(receipt["reason"], "read_only_review")

    def test_current_execution_cannot_hide_an_open_pr_at_the_same_source_sha(self):
        api = FakeAPI()
        own = current_cleanup(api)
        current = inventory([branch("release-publish/v0.2.4", SOURCE)], active=[own], pulls=[
            {"head": {"ref": "new-work", "sha": OTHER,
                      "repo": {"full_name": legacy.REPOSITORY}},
             "base": {"ref": "main", "sha": SOURCE,
                      "repo": {"full_name": legacy.REPOSITORY}}}])
        frozen = {"branches": [{"name": "release-publish/v0.2.4", "sha": SOURCE,
                                "disposition": "candidate_after_successful_new_release",
                                "reason": "fixture published source"}]}
        with mock.patch.dict(os.environ, {"GITHUB_RUN_ID": str(CURRENT_RUN_ID),
                                          "GITHUB_RUN_ATTEMPT": "2"}), \
                mock.patch.object(cleanup, "validate_execution_event"):
            view, _ = cleanup.selection_view(api, current, SOURCE, PUBLICATION_ID, True)
        eligible, held = legacy.select_refs(api, frozen, view)
        self.assertFalse(eligible)
        self.assertEqual(held[0]["reason"], "open_pull_request_or_same_commit_alias")

    def test_current_run_is_reauthenticated_after_dry_run_before_any_deletion(self):
        api = FakeAPI()
        own = current_cleanup(api)
        current = inventory([branch("release-publish/v0.2.4", SOURCE)], active=[own])
        frozen = {"branches": [{"name": "release-publish/v0.2.4", "sha": SOURCE,
                                "disposition": "candidate_after_successful_new_release",
                                "reason": "fixture published source"}]}
        report = {"deletion_attempted": False, "deleted_refs": []}

        def run_command(command, **_kwargs):
            if "--dry-run" in command:
                api.rows[f"/actions/runs/{CURRENT_RUN_ID}"]["run_attempt"] = 3
            return subprocess.CompletedProcess(command, 0, "", "")

        with mock.patch.dict(os.environ, {"GITHUB_RUN_ID": str(CURRENT_RUN_ID),
                                          "GITHUB_RUN_ATTEMPT": "2"}), \
                mock.patch.object(cleanup, "validate_execution_event") as event, \
                mock.patch.object(legacy, "live_inventory", return_value=current), \
                mock.patch.object(subprocess, "run", side_effect=run_command) as run:
            view, _ = cleanup.selection_view(api, current, SOURCE, PUBLICATION_ID, True)
            eligible, _ = legacy.select_refs(api, frozen, view)
            self.assertEqual(len(eligible), 1)
            with self.assertRaises(ValueError):
                cleanup.execute_cleanup(api, "offline-fixture", args(), frozen, eligible, report)
        self.assertEqual(event.call_count, 2)
        pushes = [call.args[0] for call in run.call_args_list if call.args[0][:2] == ["git", "push"]]
        self.assertEqual(len(pushes), 1)
        self.assertIn("--dry-run", pushes[0])
        self.assertFalse(report["deletion_attempted"])
        self.assertEqual(report["live_inventory_immediately_before_push"], current)
        self.assertIsNone(report["final_selection_context"]["excluded_cleanup_run_id"])
        self.assertEqual(report["final_selection_context"]["reason"],
                         "current_cleanup_not_authenticated")
        self.assertFalse(report["final_eligible"])
        self.assertEqual(report["final_held"][0]["reason"],
                         "active_workflow_or_same_commit_alias")

    def test_successful_mock_execution_uses_atomic_frozen_sha_leases(self):
        name, sha = next(iter(cleanup.FIXED_REFS.items()))
        frozen = {"branches": [{"name": name, "sha": sha,
                                "disposition": "candidate_after_successful_new_release",
                                "reason": "fixture reviewed exact head"}]}
        eligible = [{"name": name, "expected_sha": sha}]
        report = {"deletion_attempted": False, "deleted_refs": []}
        api = FakeAPI()
        with mock.patch.object(legacy, "live_inventory",
                               return_value=inventory([branch(name, sha)])), \
                mock.patch.object(subprocess, "run",
                                  return_value=subprocess.CompletedProcess([], 0, "", "")) as run:
            cleanup.execute_cleanup(api, "offline-fixture", args(), frozen, eligible, report)
        pushes = [call.args[0] for call in run.call_args_list if call.args[0][:2] == ["git", "push"]]
        self.assertEqual(len(pushes), 2)
        self.assertIn("--dry-run", pushes[0])
        self.assertNotIn("--dry-run", pushes[1])
        for command in pushes:
            self.assertIn("--atomic", command)
            self.assertIn(f"--force-with-lease=refs/heads/{name}:{sha}", command)
            self.assertEqual(command[-1], ":refs/heads/" + name)
        self.assertEqual(report["result"], "atomic_leased_deletion_confirmed_absent")
        self.assertEqual(report["deleted_refs"], [{"name": name, "previous_sha": sha}])

    def test_workflow_integrity_pins_match_all_imported_cleanup_inputs(self):
        workflow = (ROOT / ".github/workflows/cleanup-v024-branches.yml").read_text()
        for key, path in (
            ("CLEANUP_SCRIPT_SHA256", "scripts/cleanup_v024_branches.py"),
            ("CLEANUP_PLAN_SHA256", "scripts/cleanup_v024_branches.json"),
            ("CLEANUP_LIBRARY_SHA256", "scripts/cleanup_v022_branches.py"),
        ):
            found = re.findall(key + r": '([0-9a-f]{64})'", workflow)
            self.assertEqual(len(found), 1)
            self.assertEqual(found[0], hashlib.sha256((ROOT / path).read_bytes()).hexdigest())


if __name__ == "__main__":
    unittest.main()
