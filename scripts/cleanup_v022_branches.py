#!/usr/bin/env python3
"""Review or perform one bounded, SHA-leased cleanup after Maris v0.2.2.

Default behavior only reads GitHub and records a plan. --execute is restricted
to GitHub Actions in the named repository and still requires all live
release/ref guards.
"""
from __future__ import annotations

import argparse
import base64
from concurrent.futures import ThreadPoolExecutor
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import urllib.parse
import urllib.request

REPOSITORY = "francis-du/maris"
TAG = "v0.2.2"
PUBLISH_PATH = ".github/workflows/publish-release.yml"
RELEASE_PATH = ".github/workflows/release-assets.yml"
PLATFORMS = {
    "macos-15", "macos-15-intel", "ubuntu-24.04", "ubuntu-24.04-arm",
    "windows-2025", "windows-11-arm",
}
ACTIVE_STATES = ("requested", "waiting", "pending", "queued", "in_progress")
SHA = re.compile(r"[0-9a-f]{40}\Z")
SMOKE_NAME = re.compile(r"(?:^| / )public-install-smoke \(([^,]+), (release|public)\)\Z")
MAX_PAGES = 20
MAX_RESPONSE = 16 * 1024 * 1024


def now():
    return datetime.now(timezone.utc).isoformat()


def require(condition, message):
    if not condition:
        raise ValueError(message)


def validate_api_path(path):
    """Keep requests repository-relative; a compare delimiter is not traversal."""
    require(isinstance(path, str) and len(path) <= 1024
            and all(33 <= ord(char) <= 126 for char in path)
            and "\\" not in path and "#" not in path, "Invalid API path")
    parsed = urllib.parse.urlsplit(path)
    require(not parsed.scheme and not parsed.netloc and not parsed.fragment,
            "Invalid API path")
    resource = urllib.parse.unquote(parsed.path, errors="strict")
    require("%" not in resource and "\\" not in resource
            and (resource == "" or re.fullmatch(r"/[A-Za-z0-9_.-]+(?:/[A-Za-z0-9_.-]+)*",
                                                resource))
            and all(part not in (".", "..") for part in resource.split("/")),
            "Invalid API path")
    if resource.startswith("/compare/"):
        require(parsed.path == resource
                and re.fullmatch(r"/compare/[0-9a-f]{40}\.\.\.[0-9a-f]{40}", resource),
                "Invalid API compare path")
    require("?" not in path or bool(parsed.query), "Invalid API query")
    seen = set()
    for field in parsed.query.split("&") if parsed.query else ():
        key, separator, value = field.partition("=")
        require(separator and key not in seen, "Invalid API query")
        seen.add(key)
        if key in ("per_page", "page"):
            bound = 100 if key == "per_page" else MAX_PAGES
            require(re.fullmatch(r"[1-9][0-9]*", value) and int(value) <= bound,
                    "Invalid API pagination")
        else:
            allowed = {"filter": {"latest"}, "state": {"open"},
                       "status": set(ACTIVE_STATES) | {"completed"}}
            require(key in allowed and value in allowed[key], "Invalid API query")
    return path


class GitHub:
    def __init__(self, token):
        self.token = token

    def get(self, path):
        validate_api_path(path)
        request = urllib.request.Request(
            "https://api.github.com/repos/" + REPOSITORY + path,
            headers={
                "Accept": "application/vnd.github+json",
                "Authorization": "Bearer " + self.token,
                "X-GitHub-Api-Version": "2022-11-28",
                "User-Agent": "Maris-reviewed-branch-cleanup",
            },
        )
        with urllib.request.urlopen(request, timeout=20) as response:
            data = response.read(MAX_RESPONSE + 1)
        require(len(data) <= MAX_RESPONSE, "Oversized API response")
        return json.loads(data)

    def collection(self, path, key=None):
        result = []
        total = None
        for page in range(1, MAX_PAGES + 1):
            separator = "&" if "?" in path else "?"
            body = self.get(f"{path}{separator}per_page=100&page={page}")
            rows = body[key] if key else body
            require(isinstance(rows, list), "Expected an API collection")
            if key and "total_count" in body:
                require(total in (None, body["total_count"]), "Collection changed while paging")
                total = body["total_count"]
            result.extend(rows)
            if len(rows) < 100:
                require(total is None or total == len(result), "Incomplete API collection")
                return result
        raise ValueError("API collection exceeded the bounded pagination limit")


def release_gate(api, source_sha, release_id, publication_run_id, tag=TAG):
    require(re.fullmatch(r"v[0-9]+\.[0-9]+\.[0-9]+", tag), "Invalid release tag")
    release = api.get("/releases/tags/" + tag)
    require(release["id"] == release_id and release["tag_name"] == tag,
            "Release identity differs from the selected new release")
    require(not release["draft"] and not release["prerelease"] and release["published_at"],
            "The new release has not reached published stable state")
    require(api.get("/releases/latest")["id"] == release_id, "The selected release is not latest")
    require(api.get("/commits/" + tag)["sha"] == source_sha, "Release tag source changed")
    require(api.get("/git/ref/heads/main")["object"]["sha"] == source_sha,
            "Main changed after the selected publication; review before cleanup")
    assets = api.collection(f"/releases/{release_id}/assets")
    expected_assets = {"maris-release.tsv", "SHA256SUMS"}
    for system in ("macos", "linux", "windows"):
        for arch in ("x86_64", "arm64"):
            extension = ".zip" if system == "windows" else ".tar.gz"
            expected_assets.add(f"Maris-{tag[1:]}-{system}-{arch}{extension}")
    require(len(assets) == 8 and {a["name"] for a in assets} == expected_assets,
            "The new release does not contain the eight expected CLI assets")
    require(all(a["state"] == "uploaded" and a["size"] > 0 for a in assets),
            "A release attachment is not fully uploaded")
    run = api.get(f"/actions/runs/{publication_run_id}")
    require(run["repository"]["full_name"] == REPOSITORY
            and run["path"] == PUBLISH_PATH and run["event"] == "push"
            and run["head_branch"] == "release-publish/" + tag
            and run["head_sha"] == source_sha
            and run["status"] == "completed" and run["conclusion"] == "success",
            "Publication workflow did not succeed for the exact new release source")
    jobs = api.collection(f"/actions/runs/{publication_run_id}/jobs?filter=latest", "jobs")
    seen = set()
    native_jobs = []
    promoted = []
    for job in jobs:
        match = SMOKE_NAME.search(job["name"])
        if match:
            pair = match.groups()
            require(pair not in seen, "Duplicate public-install job identity")
            seen.add(pair)
            expected_step = ("Install from the public Release with install.ps1"
                             if pair[0].startswith("windows-")
                             else "Install from the public Release with install.sh")
            require(job["head_sha"] == source_sha and job["status"] == "completed"
                    and job["conclusion"] == "success" and pair[0] in job["labels"]
                    and any(s["name"] == expected_step and s["conclusion"] == "success"
                            for s in job["steps"]),
                    "A required native public installation has not succeeded")
            native_jobs.append({"id": job["id"], "name": job["name"], "url": job["html_url"]})
        if job["name"].split(" / ")[-1] == "promote-stable":
            require(job["head_sha"] == source_sha and job["conclusion"] == "success"
                    and any(s["name"] == "Promote smoke-tested prerelease to stable"
                            and s["conclusion"] == "success" for s in job["steps"]),
                    "Stable promotion has not succeeded")
            promoted.append(job["id"])
    require(seen == {(platform, bootstrap) for platform in PLATFORMS
                     for bootstrap in ("release", "public")} and len(promoted) == 1,
            "The publication lacks all twelve native install jobs and one promotion")
    return {"release_id": release_id, "tag": tag, "source_sha": source_sha,
            "url": release["html_url"], "published_at": release["published_at"],
            "publication_run_id": publication_run_id, "publication_run_attempt": run["run_attempt"],
            "native_install_jobs": native_jobs, "promotion_job": promoted[0],
            "assets": [{k: a[k] for k in ("id", "name", "size", "updated_at")} for a in assets]}


def workflow_inventory(api):
    runs = {}
    for state in ACTIVE_STATES:
        for run in api.collection("/actions/runs?status=" + state, "workflow_runs"):
            runs[run["id"]] = run
    # Runs can advance between status-filtered calls. Query a status-independent
    # recent window as well; this is a point-in-time guard, not an atomic lock on
    # workflow state. The branch SHA leases below are the atomic protection.
    recent = api.get("/actions/runs?per_page=100&page=1")
    require(isinstance(recent.get("workflow_runs"), list), "Invalid recent workflow collection")
    for run in recent["workflow_runs"]:
        runs[run["id"]] = run
    active = [r for r in runs.values() if r["status"] != "completed"]
    return {"active_runs": active,
            "active_state_query_order": list(ACTIVE_STATES),
            "recent_run_window": {"limit": 100, "total_count": recent.get("total_count"),
                                  "runs": [{k: run[k] for k in ("id", "status", "head_branch", "head_sha", "path", "created_at", "updated_at")}
                                           for run in recent["workflow_runs"]]}}


def live_inventory(api):
    repository = api.get("")
    require(repository["full_name"] == REPOSITORY and repository["default_branch"] == "main",
            "Repository or default branch identity changed")
    branches = api.collection("/branches")
    require(len({b["name"] for b in branches}) == len(branches), "Duplicate branch inventory")
    pulls = api.collection("/pulls?state=open")
    workflows = workflow_inventory(api)
    active = workflows["active_runs"]
    require(not any(r["path"].split("@", 1)[0] in (PUBLISH_PATH, RELEASE_PATH) for r in active),
            "A publication, installation, promotion or rollback workflow is still active")
    return {"at": now(), "branches": branches, "open_pulls": pulls, **workflows,
            "concurrency_limit": "PR and workflow checks are point-in-time; only exact branch-tip deletion is protected by atomic Git SHA leases."}


def select_refs(api, plan, inventory):
    branches = {b["name"]: b for b in inventory["branches"]}
    open_names, open_shas = set(), set()
    for pull in inventory["open_pulls"]:
        for side in ("head", "base"):
            item = pull[side]
            if item.get("repo") and item["repo"]["full_name"] == REPOSITORY:
                open_names.add(item["ref"])
                open_shas.add(item["sha"])
    active_names = {r["head_branch"] for r in inventory["active_runs"]}
    active_shas = {r["head_sha"] for r in inventory["active_runs"]}
    proposed, held = [], []
    for item in plan["branches"]:
        if item["disposition"] != "candidate_after_successful_new_release":
            continue
        name, expected = item["name"], item["sha"]
        require(name != "main" and SHA.fullmatch(expected), "Invalid candidate ref")
        require(re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9_./-]*", name), "Invalid candidate branch name")
        branch = branches.get(name)
        reason = None
        if branch is None:
            reason = "already_absent_no_delete_attempt"
        elif branch["commit"]["sha"] != expected:
            reason = "tip_changed_retain_for_review"
        elif branch["protected"]:
            reason = "protected_branch"
        elif name in open_names or expected in open_shas:
            reason = "open_pull_request_or_same_commit_alias"
        elif name in active_names or expected in active_shas:
            reason = "active_workflow_or_same_commit_alias"
        if reason:
            held.append({"name": name, "expected_sha": expected, "reason": reason})
        else:
            proposed.append({"name": name, "expected_sha": expected,
                             "basis": item.get("candidate_basis", item["reason"])})

    def rules(item):
        try:
            found = api.get("/rules/branches/" + urllib.parse.quote(item["name"], safe=""))
            require(isinstance(found, list), "Unexpected rules response")
            if any(rule["type"] == "deletion" for rule in found):
                return item, None, "branch_deletion_rule"
            return item, found, None
        except Exception as error:
            return item, None, "rules_not_confirmed: " + type(error).__name__

    eligible = []
    with ThreadPoolExecutor(max_workers=6) as pool:
        for item, found, reason in pool.map(rules, proposed):
            if reason:
                held.append({**item, "reason": reason})
            else:
                eligible.append({**item, "applicable_rules": found})
    return sorted(eligible, key=lambda x: x["name"]), held


def push_arguments(eligible, dry_run=False):
    require(eligible, "Empty cleanup must not invoke Git push")
    args = ["git", "push", "--atomic", "--porcelain"]
    if dry_run:
        args.append("--dry-run")
    for item in eligible:
        args.append("--force-with-lease=refs/heads/" + item["name"] + ":" + item["expected_sha"])
    args.append("origin")
    args.extend(":refs/heads/" + item["name"] for item in eligible)
    return args


def git_environment(token):
    environment = os.environ.copy()
    # Git trace output can include HTTP authorization headers. This operation
    # neither enables tracing nor inherits ambient trace/debug switches.
    for key in tuple(environment):
        if key.startswith("GIT_TRACE") or key == "GIT_CURL_VERBOSE":
            environment.pop(key)
    credential = base64.b64encode(("x-access-token:" + token).encode()).decode()
    environment.update({"GIT_TERMINAL_PROMPT": "0", "GIT_CONFIG_COUNT": "1",
                        "GIT_CONFIG_KEY_0": "http.https://github.com/.extraheader",
                        "GIT_CONFIG_VALUE_0": "AUTHORIZATION: basic " + credential})
    return environment


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--plan", type=Path, required=True)
    parser.add_argument("--plan-sha256", required=True)
    parser.add_argument("--source-sha", required=True)
    parser.add_argument("--release-id", type=int, required=True)
    parser.add_argument("--publication-run-id", type=int, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--execute", action="store_true")
    args = parser.parse_args()
    report = {"schema_version": 1, "repository": REPOSITORY, "started_at": now(),
              "execute_requested": args.execute, "deletion_attempted": False,
              "deleted_refs": [], "observed_absent_refs": [], "error": None}
    try:
        require(not args.output.exists(), "Evidence destination already exists")
        data = args.plan.read_bytes()
        require(hashlib.sha256(data).hexdigest() == args.plan_sha256, "Reviewed plan hash changed")
        plan = json.loads(data)
        require(plan["repository"] == REPOSITORY and plan["default_branch"] == "main"
                and SHA.fullmatch(plan["main_sha"]) and SHA.fullmatch(args.source_sha),
                "Plan or source identity is invalid")
        require(len({b["name"] for b in plan["branches"]}) == len(plan["branches"]),
                "Duplicate planned refs")
        require(args.release_id > 0 and args.publication_run_id > 0, "Actual release and run IDs are required")
        if args.execute:
            require(os.environ.get("GITHUB_ACTIONS") == "true"
                    and os.environ.get("GITHUB_REPOSITORY") == REPOSITORY,
                    "Execution is restricted to this repository's GitHub Actions runner")
        token = os.environ.get("GH_TOKEN") or os.environ.get("GITHUB_TOKEN")
        require(bool(token), "A normal repository token is required; it is never printed")
        api = GitHub(token)
        report["plan_sha256"] = args.plan_sha256
        report["release"] = release_gate(api, args.source_sha, args.release_id, args.publication_run_id)
        ancestry = api.get("/compare/" + args.source_sha + "..." + plan["main_sha"] + "?per_page=1")
        require(ancestry["status"] in ("behind", "identical") and ancestry["ahead_by"] == 0
                and ancestry["merge_base_commit"]["sha"] == plan["main_sha"],
                "Reviewed main is not an ancestor of the published source")
        report["ancestry"] = {k: ancestry[k] for k in ("status", "ahead_by", "behind_by")}
        initial = live_inventory(api)
        eligible, held = select_refs(api, plan, initial)
        report.update({"live_inventory_before": initial, "eligible": eligible, "held": held})
        if not args.execute or not eligible:
            report["result"] = "review_only" if not args.execute else "nothing_eligible"
            return 0
        with tempfile.TemporaryDirectory(prefix="maris-branch-cleanup-") as directory:
            subprocess.run(["git", "init", "--bare", "--quiet", directory], check=True, timeout=20)
            subprocess.run(["git", "-C", directory, "remote", "add", "origin",
                            "https://github.com/" + REPOSITORY + ".git"], check=True, timeout=20)
            environment = git_environment(token)
            probe = subprocess.run(push_arguments(eligible, dry_run=True), cwd=directory,
                                   env=environment, capture_output=True, text=True, timeout=90)
            report["git_dry_run"] = {"exit_code": probe.returncode, "stdout": probe.stdout,
                                     "stderr": probe.stderr}
            require(probe.returncode == 0, "Git dry run rejected the atomic leased deletion")
            release_gate(api, args.source_sha, args.release_id, args.publication_run_id)
            final = live_inventory(api)
            final_eligible, final_held = select_refs(api, plan, final)
            require([(x["name"], x["expected_sha"]) for x in final_eligible]
                    == [(x["name"], x["expected_sha"]) for x in eligible],
                    "Live cleanup eligibility changed; review without automatic retries")
            report["live_inventory_immediately_before_push"] = final
            report["final_held"] = final_held
            report["deletion_attempted"] = True
            try:
                pushed = subprocess.run(push_arguments(eligible), cwd=directory, env=environment,
                                        capture_output=True, text=True, timeout=90)
                report["git_push"] = {"exit_code": pushed.returncode, "stdout": pushed.stdout,
                                      "stderr": pushed.stderr}
            except subprocess.TimeoutExpired:
                report["git_push"] = {"exit_code": None, "error": "timeout_outcome_unconfirmed"}
            after = api.collection("/branches")
            report["branches_after"] = after
            remaining = {b["name"]: b["commit"]["sha"] for b in after}
            report["readback"] = [{**item, "observed_sha": remaining.get(item["name"]),
                                    "absent": item["name"] not in remaining} for item in eligible]
            report["observed_absent_refs"] = [
                {"name": item["name"], "previous_sha": item["expected_sha"]}
                for item in eligible if item["name"] not in remaining
            ]
            if report["git_push"]["exit_code"] == 0:
                report["deleted_refs"] = report["observed_absent_refs"]
            require(report["git_push"]["exit_code"] == 0
                    and len(report["deleted_refs"]) == len(eligible),
                    "Deletion result is incomplete or uncertain; inspect readback, never retry blindly")
            require(remaining.get("main") == args.source_sha, "Main changed during cleanup; inspect evidence")
            report["release_after"] = release_gate(api, args.source_sha, args.release_id, args.publication_run_id)
            report["result"] = "atomic_leased_deletion_confirmed_absent"
        return 0
    except Exception as error:
        report["error"] = type(error).__name__ + ": " + str(error)
        report["result"] = "blocked_or_requires_review"
        return 1
    finally:
        report["finished_at"] = now()
        if not args.output.exists():
            args.output.parent.mkdir(parents=True, exist_ok=True)
            with args.output.open("x", encoding="utf-8") as output:
                json.dump(report, output, ensure_ascii=False, indent=2)
                output.write("\n")
        print(json.dumps({"result": report.get("result"), "deletion_attempted": report["deletion_attempted"],
                          "confirmed_absent_count": len(report["deleted_refs"]), "error": report["error"]}))


if __name__ == "__main__":
    raise SystemExit(main())
