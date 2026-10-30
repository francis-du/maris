#!/usr/bin/env python3
"""Review or execute only the explicitly scoped Maris v0.2.4 branch cleanup.

The default is read-only. Execution requires the exact successful stable-release
workflow event and keeps the legacy publication, rule and atomic SHA-lease gates.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import time

import cleanup_v022_branches as legacy

TAG = "v0.2.4"
REVIEWED_MAIN = "67d2ebcfcafdeb5584329525738fdc8e81f4ae7b"
REQUIRED_PR = 96
CLEANUP_PATH = ".github/workflows/cleanup-v024-branches.yml"
FIXED_REFS = {
    "refactor/macos-stable-device-uid": "a5d2af19142fad11e001fe64ebeed9d0c060d8cf",
    "rsi-preparation/v0.2.4-20261030": "8bce260cdbd9a6d05afae95aecb07406066b9977",
}
ANCESTOR_REFS = {
    "fix/coreaudio-startup-20261030",
    "docs-refresh/v0.2.4",
    "release-validation/v0.2.4",
    "release-publish/v0.2.4",
}


def validate_plan(plan):
    legacy.require(isinstance(plan, dict), "Expected a cleanup plan object")
    legacy.require(
        plan.get("schema_version") == 1
        and plan.get("repository") == legacy.REPOSITORY
        and plan.get("default_branch") == "main"
        and plan.get("main_sha") == REVIEWED_MAIN
        and plan.get("tag_precondition") == TAG
        and plan.get("required_merged_pr") == REQUIRED_PR,
        "The reviewed v0.2.4 plan identity changed",
    )
    fixed = plan.get("fixed_refs")
    names = plan.get("ancestor_refs")
    legacy.require(isinstance(fixed, list) and len(fixed) == len(FIXED_REFS),
                   "Unexpected fixed branch scope")
    legacy.require(all(isinstance(item, dict) for item in fixed),
                   "Invalid fixed branch entry")
    legacy.require({item.get("name"): item.get("sha") for item in fixed} == FIXED_REFS,
                   "A reviewed exact branch tip changed")
    legacy.require(all(isinstance(item.get("reason"), str) and item["reason"].strip()
                       for item in fixed), "Missing historical content-review evidence")
    legacy.require(isinstance(names, list) and len(names) == len(ANCESTOR_REFS)
                   and all(isinstance(name, str) for name in names)
                   and set(names) == ANCESTOR_REFS, "Unexpected dynamic branch scope")
    return plan


def read_plan(path, expected_digest):
    legacy.require(re.fullmatch(r"[0-9a-f]{64}", expected_digest),
                   "Invalid reviewed plan digest")
    legacy.require(path.is_file() and not path.is_symlink(), "Plan is missing or linked")
    data = path.read_bytes()
    legacy.require(len(data) <= 32768, "Oversized cleanup plan")
    legacy.require(hashlib.sha256(data).hexdigest() == expected_digest,
                   "Reviewed plan bytes changed")
    return validate_plan(json.loads(data))


def comparison(api, ancestor, descendant):
    legacy.require(isinstance(ancestor, str) and legacy.SHA.fullmatch(ancestor)
                   and isinstance(descendant, str) and legacy.SHA.fullmatch(descendant),
                   "Invalid comparison identity")
    result = api.get(f"/compare/{descendant}...{ancestor}?per_page=1")
    proven = (result["status"] in ("behind", "identical") and result["ahead_by"] == 0
              and result["merge_base_commit"]["sha"] == ancestor)
    evidence = {key: result[key] for key in ("status", "ahead_by", "behind_by")}
    return proven, evidence


def require_ancestor(api, ancestor, descendant):
    proven, evidence = comparison(api, ancestor, descendant)
    legacy.require(proven, "Reviewed commit is not an ancestor of the published source")
    return {"ancestor": ancestor, "descendant": descendant, **evidence}


def publication_gate(api, source_sha, release_id, publication_run_id):
    # Reuse the actual twelve native-installation and promotion verification;
    # never substitute an automatic asset workflow's six public smoke jobs.
    release = legacy.release_gate(api, source_sha, release_id, publication_run_id, tag=TAG)
    assets = api.collection(f"/releases/{release_id}/assets")
    legacy.require(len(assets) == 8 and all(
        type(asset.get("id")) is int and asset["id"] > 0
        and isinstance(asset.get("name"), str)
        and type(asset.get("size")) is int and asset["size"] > 0
        and isinstance(asset.get("updated_at"), str) and asset["updated_at"]
        and asset.get("state") == "uploaded"
        and isinstance(asset.get("digest"), str)
        and re.fullmatch(r"sha256:[0-9a-f]{64}", asset["digest"])
        for asset in assets
    ), "Release attachment identity or SHA-256 digest is missing")
    assets = sorted(
        [{key: asset[key] for key in ("id", "name", "size", "updated_at", "state", "digest")}
         for asset in assets], key=lambda asset: asset["name"])
    legacy.require(
        sorted(release["assets"], key=lambda asset: asset["name"])
        == [{key: asset[key] for key in ("id", "name", "size", "updated_at")} for asset in assets],
        "Release attachments changed while verifying publication",
    )
    run = api.get(f"/actions/runs/{publication_run_id}")
    expected = {"id": publication_run_id, "name": "Publish stable release",
                "path": legacy.PUBLISH_PATH, "event": "push",
                "head_branch": "release-publish/" + TAG, "head_sha": source_sha,
                "status": "completed", "conclusion": "success",
                "run_attempt": release["publication_run_attempt"]}
    legacy.require(type(run.get("run_attempt")) is int and run["run_attempt"] > 0
                   and all(run.get(key) == value for key, value in expected.items())
                   and run.get("repository", {}).get("full_name") == legacy.REPOSITORY
                   and run.get("head_repository", {}).get("full_name") == legacy.REPOSITORY,
                   "Publication caller or attempt changed while verifying native jobs")
    release["assets"] = assets
    release["native_install_jobs"] = sorted(release["native_install_jobs"],
                                             key=lambda job: job["name"])
    return release


def review_gate(api, source_sha):
    ancestor = require_ancestor(api, REVIEWED_MAIN, source_sha)
    pull = api.get(f"/pulls/{REQUIRED_PR}")
    legacy.require(
        pull.get("number") == REQUIRED_PR and pull.get("state") == "closed"
        and pull.get("merged") is True
        and pull.get("base", {}).get("ref") == "main"
        and pull.get("base", {}).get("repo", {}).get("full_name") == legacy.REPOSITORY
        and pull.get("head", {}).get("repo", {}).get("full_name") == legacy.REPOSITORY
        and pull.get("head", {}).get("ref") == "fix/coreaudio-startup-20261030",
        "PR #96 is not the merged reviewed CoreAudio change",
    )
    merged = pull.get("merge_commit_sha")
    merge_proof = require_ancestor(api, merged, source_sha)
    return {"reviewed_main": ancestor, "required_pr": REQUIRED_PR,
            "pr_head_sha": pull["head"]["sha"], "pr_merge": merge_proof}


def validate_execution_event(source_sha, publication_run_id):
    legacy.require(os.environ.get("GITHUB_ACTIONS") == "true"
                   and os.environ.get("GITHUB_REPOSITORY") == legacy.REPOSITORY
                   and os.environ.get("GITHUB_EVENT_NAME") == "workflow_run",
                   "Execution requires this repository's completed publication event")
    path = Path(os.environ.get("GITHUB_EVENT_PATH", ""))
    legacy.require(path.is_file() and not path.is_symlink(), "Missing workflow event")
    data = path.read_bytes()
    legacy.require(len(data) <= legacy.MAX_RESPONSE, "Oversized workflow event")
    event = json.loads(data)
    run = event.get("workflow_run", {})
    legacy.require(
        event.get("action") == "completed"
        and event.get("repository", {}).get("full_name") == legacy.REPOSITORY
        and run.get("id") == publication_run_id
        and type(run.get("run_attempt")) is int and run["run_attempt"] > 0
        and run.get("head_sha") == source_sha
        and run.get("path") == legacy.PUBLISH_PATH
        and run.get("name") == "Publish stable release"
        and run.get("head_branch") == "release-publish/" + TAG
        and run.get("event") == "push"
        and run.get("status") == "completed" and run.get("conclusion") == "success"
        and run.get("head_repository", {}).get("full_name") == legacy.REPOSITORY,
        "The triggering run is not the exact successful v0.2.4 publication",
    )
    actual = subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip()
    legacy.require(actual == source_sha, "Cleanup checkout is not the published source")
    return run["run_attempt"]


def wait_publication_idle(api, maximum_seconds, report):
    legacy.require(type(maximum_seconds) is int and 0 <= maximum_seconds <= 240,
                   "Invalid bounded publication wait")
    deadline = time.monotonic() + maximum_seconds
    observations = report.setdefault("publication_idle_observations", [])
    while True:
        inventory = legacy.workflow_inventory(api)
        active = [run for run in inventory["active_runs"]
                  if run["path"].split("@", 1)[0] in (legacy.PUBLISH_PATH, legacy.RELEASE_PATH)]
        observations.append({"at": legacy.now(), "active_publication_run_ids":
                             [run["id"] for run in active]})
        if not active:
            return
        remaining = deadline - time.monotonic()
        legacy.require(remaining > 0, "Publication workflows did not settle; no deletion attempted")
        time.sleep(min(10, remaining))


def materialize_refs(api, plan, inventory, source_sha):
    """Resolve only named ancestor refs once; later checks keep these SHA leases."""
    branches = {item["name"]: item for item in inventory["branches"]}
    resolved = [
        {"name": item["name"], "sha": item["sha"],
         "disposition": "candidate_after_successful_new_release",
         "reason": item["reason"], "candidate_basis": "reviewed_superseded_exact_tip"}
        for item in plan["fixed_refs"]
    ]
    held, proofs = [], []
    for name in sorted(plan["ancestor_refs"]):
        branch = branches.get(name)
        if branch is None:
            held.append({"name": name, "reason": "already_absent_no_delete_attempt"})
            continue
        sha = branch["commit"]["sha"]
        proven, evidence = comparison(api, sha, source_sha)
        proofs.append({"name": name, "observed_sha": sha, **evidence})
        if not proven:
            held.append({"name": name, "expected_sha": sha,
                         "reason": "named_ref_has_unpublished_commits"})
            continue
        resolved.append({"name": name, "sha": sha,
                         "disposition": "candidate_after_successful_new_release",
                         "reason": "Observed exact tip is an ancestor of the published source",
                         "candidate_basis": "runtime_exact_tip_ancestor_or_identical"})
    return {"branches": resolved}, held, proofs


def same_candidates(before, after):
    legacy.require([(item["name"], item["expected_sha"]) for item in before]
                   == [(item["name"], item["expected_sha"]) for item in after],
                   "Live cleanup eligibility changed; review without automatic retries")


def selection_view(api, inventory, source_sha, publication_run_id, execute=False):
    """Exclude only this authenticated cleanup execution, never a SHA or name group."""
    receipt = {"excluded_cleanup_run_id": None, "reason": "read_only_review"}
    if not execute:
        return inventory, receipt
    try:
        # The current cleanup run uses main; its triggering publisher uses the
        # release-publish branch. Authenticate both identities independently.
        validate_execution_event(source_sha, publication_run_id)
        run_id = os.environ.get("GITHUB_RUN_ID", "")
        attempt = os.environ.get("GITHUB_RUN_ATTEMPT", "")
        legacy.require(re.fullmatch(r"[1-9][0-9]*", run_id)
                       and re.fullmatch(r"[1-9][0-9]*", attempt),
                       "Missing current cleanup run identity")
        current = api.get("/actions/runs/" + run_id)
        expected = {"id": int(run_id), "run_attempt": int(attempt),
                    "path": CLEANUP_PATH, "event": "workflow_run",
                    "head_branch": "main", "head_sha": source_sha,
                    "status": "in_progress"}
        legacy.require(type(current.get("id")) is int
                       and type(current.get("run_attempt")) is int
                       and all(current.get(key) == value for key, value in expected.items())
                       and current.get("repository", {}).get("full_name") == legacy.REPOSITORY
                       and current.get("head_repository", {}).get("full_name")
                       == legacy.REPOSITORY,
                       "Current cleanup run does not match the authenticated execution")
        own = [run for run in inventory["active_runs"] if run.get("id") == int(run_id)]
        legacy.require(len(own) == 1
                       and all(own[0].get(key) == value for key, value in expected.items())
                       and own[0].get("repository", {}).get("full_name") == legacy.REPOSITORY
                       and own[0].get("head_repository", {}).get("full_name")
                       == legacy.REPOSITORY,
                       "Inventory does not contain the same current cleanup execution")
        view = {**inventory, "active_runs": [run for run in inventory["active_runs"]
                                           if run.get("id") != int(run_id)]}
        receipt.update({"excluded_cleanup_run_id": int(run_id),
                        "reason": "authenticated_current_cleanup_execution",
                        "current_run": expected})
        return view, receipt
    except Exception as error:
        # Authentication failure preserves the complete original inventory and
        # every legacy same-SHA activity guard; it never creates an exception.
        receipt["reason"] = "current_cleanup_not_authenticated"
        receipt["error"] = type(error).__name__ + ": " + str(error)
        return inventory, receipt


def execute_cleanup(api, token, args, frozen_plan, eligible, report):
    initial_release = report["release"]
    with tempfile.TemporaryDirectory(prefix="maris-v024-cleanup-") as directory:
        subprocess.run(["git", "init", "--bare", "--quiet", directory], check=True, timeout=20)
        subprocess.run(["git", "-C", directory, "remote", "add", "origin",
                        "https://github.com/" + legacy.REPOSITORY + ".git"],
                       check=True, timeout=20)
        environment = legacy.git_environment(token)
        probe = subprocess.run(legacy.push_arguments(eligible, dry_run=True), cwd=directory,
                               env=environment, capture_output=True, text=True, timeout=90)
        report["git_dry_run"] = {"exit_code": probe.returncode,
                                 "stdout": probe.stdout, "stderr": probe.stderr}
        legacy.require(probe.returncode == 0, "Git rejected the atomic leased deletion preview")
        report["release_before_push"] = publication_gate(
            api, args.source_sha, args.release_id, args.publication_run_id)
        legacy.require(report["release_before_push"] == initial_release,
                       "Release evidence changed after review; no deletion attempted")
        review_gate(api, args.source_sha)
        final = legacy.live_inventory(api)
        final_view, identity = selection_view(
            api, final, args.source_sha, args.publication_run_id, execute=True)
        final_eligible, final_held = legacy.select_refs(api, frozen_plan, final_view)
        report["live_inventory_immediately_before_push"] = final
        report["final_selection_context"] = identity
        report["final_eligible"] = final_eligible
        report["final_held"] = final_held
        same_candidates(eligible, final_eligible)
        report["deletion_attempted"] = True
        try:
            pushed = subprocess.run(legacy.push_arguments(eligible), cwd=directory,
                                    env=environment, capture_output=True, text=True, timeout=90)
            report["git_push"] = {"exit_code": pushed.returncode,
                                  "stdout": pushed.stdout, "stderr": pushed.stderr}
        except subprocess.TimeoutExpired:
            report["git_push"] = {"exit_code": None, "error": "timeout_outcome_unconfirmed"}
        after = api.collection("/branches")
        remaining = {item["name"]: item["commit"]["sha"] for item in after}
        report["branches_after"] = after
        report["readback"] = [{**item, "observed_sha": remaining.get(item["name"]),
                               "absent": item["name"] not in remaining} for item in eligible]
        report["observed_absent_refs"] = [
            {"name": item["name"], "previous_sha": item["expected_sha"]}
            for item in eligible if item["name"] not in remaining
        ]
        if report["git_push"]["exit_code"] == 0:
            report["deleted_refs"] = report["observed_absent_refs"]
        legacy.require(report["git_push"]["exit_code"] == 0
                       and len(report["deleted_refs"]) == len(eligible),
                       "Deletion is incomplete or uncertain; inspect readback, never retry blindly")
        legacy.require(remaining.get("main") == args.source_sha,
                       "Main changed during cleanup; inspect evidence")
        report["release_after"] = publication_gate(
            api, args.source_sha, args.release_id, args.publication_run_id)
        legacy.require(report["release_after"] == initial_release,
                       "Release evidence changed during deletion; inspect the confirmed ref readback")
        report["result"] = "atomic_leased_deletion_confirmed_absent"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--plan", type=Path, required=True,
                        help="Path to the reviewed v0.2.4 JSON plan; only its six exact "
                             "branch names are in scope.")
    parser.add_argument("--plan-sha256", required=True,
                        help="Expected SHA-256 of the reviewed plan bytes "
                             "(64 lowercase hexadecimal characters).")
    parser.add_argument("--source-sha", required=True,
                        help="Published v0.2.4 source commit (40 lowercase hexadecimal "
                             "characters); must match the tag, main and publication run.")
    parser.add_argument("--release-id", type=int, required=True,
                        help="Positive GitHub ID of the published stable v0.2.4 release; "
                             "it must also be the latest release.")
    parser.add_argument("--publication-run-id", type=int, required=True,
                        help="Positive GitHub run ID of the successful v0.2.4 Publish "
                             "stable release caller; all 12 native installation jobs "
                             "and stable promotion must have succeeded.")
    parser.add_argument("--wait-seconds", type=int, default=240,
                        help="Publication-idle polling budget in seconds, from 0 through "
                             "240 (default: 240); active publication workflows block deletion.")
    parser.add_argument("--output", type=Path, required=True,
                        help="Path for a new JSON evidence receipt; an existing destination "
                             "is never overwritten.")
    parser.add_argument("--execute", action="store_true",
                        help="Perform actual atomic SHA-leased branch deletions only in "
                             "the authenticated successful v0.2.4 workflow event after all "
                             "publication, review and live-ref gates pass. Omit for "
                             "read-only review.")
    args = parser.parse_args()
    report = {"schema_version": 1, "repository": legacy.REPOSITORY, "tag": TAG,
              "started_at": legacy.now(), "execute_requested": args.execute,
              "deletion_attempted": False, "deleted_refs": [], "observed_absent_refs": [],
              "error": None}
    try:
        legacy.require(not args.output.exists(), "Evidence destination already exists")
        plan = read_plan(args.plan, args.plan_sha256)
        legacy.require(legacy.SHA.fullmatch(args.source_sha), "Invalid release source SHA")
        legacy.require(args.release_id > 0 and args.publication_run_id > 0,
                       "Actual release and publication run IDs are required")
        if args.execute:
            report["trigger_publication_attempt"] = validate_execution_event(
                args.source_sha, args.publication_run_id)
        token = os.environ.get("GH_TOKEN") or os.environ.get("GITHUB_TOKEN")
        legacy.require(bool(token), "A normal repository token is required; it is never printed")
        api = legacy.GitHub(token)
        report["plan_sha256"] = args.plan_sha256
        report["release"] = publication_gate(
            api, args.source_sha, args.release_id, args.publication_run_id)
        if args.execute:
            legacy.require(report["release"]["publication_run_attempt"]
                           == report["trigger_publication_attempt"],
                           "Live publication attempt differs from the triggering completed event")
        report["review"] = review_gate(api, args.source_sha)
        wait_publication_idle(api, args.wait_seconds, report)
        inventory = legacy.live_inventory(api)
        frozen, held, proofs = materialize_refs(api, plan, inventory, args.source_sha)
        view, identity = selection_view(
            api, inventory, args.source_sha, args.publication_run_id, execute=args.execute)
        eligible, guarded = legacy.select_refs(api, frozen, view)
        report.update({"live_inventory_before": inventory, "frozen_plan": frozen,
                       "initial_selection_context": identity,
                       "ancestor_ref_proofs": proofs, "eligible": eligible,
                       "held": held + guarded})
        if not args.execute or not eligible:
            report["result"] = "review_only" if not args.execute else "nothing_eligible"
        else:
            execute_cleanup(api, token, args, frozen, eligible, report)
        return 0
    except Exception as error:
        report["error"] = type(error).__name__ + ": " + str(error)
        report["result"] = "blocked_or_requires_review"
        return 1
    finally:
        report["finished_at"] = legacy.now()
        if not args.output.exists():
            args.output.parent.mkdir(parents=True, exist_ok=True)
            with args.output.open("x", encoding="utf-8") as output:
                json.dump(report, output, ensure_ascii=False, indent=2)
                output.write("\n")
        print(json.dumps({"result": report.get("result"),
                          "deletion_attempted": report["deletion_attempted"],
                          "confirmed_absent_count": len(report["deleted_refs"]),
                          "error": report["error"]}))


if __name__ == "__main__":
    raise SystemExit(main())
