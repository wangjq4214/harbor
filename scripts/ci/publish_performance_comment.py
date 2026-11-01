"""Publish a bounded performance artifact using trusted default-branch code only.

This workflow_run consumer never executes PR code or uses artifact text as Markdown.
The artifact is data, not authority: run identity, PR association and current SHAs
are checked against the GitHub API before touching a comment.
"""

import io
import json
import os
from pathlib import Path
import re
import urllib.error
import urllib.parse
import urllib.request
import zipfile

from performance_report import render_report, validate_report


MARKER = "<!-- harbor-performance:v1"
STAMP = re.compile(r"^<!-- harbor-performance:v1 run=(\d+) number=(\d+) attempt=(\d+) -->")
ARTIFACT = "pr-performance-data"
MAX_BYTES = 256 * 1024


class SafeRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        redirected = super().redirect_request(req, fp, code, msg, headers, newurl)
        if urllib.parse.urlsplit(newurl).scheme != "https":
            raise ValueError("refusing insecure artifact redirect")
        if redirected and (
                urllib.parse.urlsplit(req.full_url).scheme,
                urllib.parse.urlsplit(req.full_url).netloc) != (
                    urllib.parse.urlsplit(newurl).scheme, urllib.parse.urlsplit(newurl).netloc):
            redirected.remove_header("Authorization")
        return redirected


class GitHub:
    def __init__(self):
        self.root = os.environ.get("GITHUB_API_URL", "https://api.github.com").rstrip("/")
        self.repo = os.environ["GITHUB_REPOSITORY"]
        self.token = os.environ["GITHUB_TOKEN"]
        self.opener = urllib.request.build_opener(SafeRedirect())

    def request(self, path, method="GET", data=None, binary=False):
        url = self.root + "/repos/" + self.repo + path
        body = None if data is None else json.dumps(data).encode()
        request = urllib.request.Request(url, data=body, method=method, headers={
            "Authorization": f"Bearer {self.token}", "Accept": "application/vnd.github+json",
            "Content-Type": "application/json", "X-GitHub-Api-Version": "2022-11-28",
            "User-Agent": "harbor-performance-comment",
        })
        with self.opener.open(request, timeout=30) as response:
            raw = response.read(MAX_BYTES + 1) if binary else response.read()
        if binary:
            if len(raw) > MAX_BYTES:
                raise ValueError("artifact archive too large")
            return raw
        return json.loads(raw) if raw else None

    def pages(self, path, key=None):
        values = []
        for page in range(1, 101):
            separator = "&" if "?" in path else "?"
            result = self.request(f"{path}{separator}per_page=100&page={page}")
            items = result[key] if key else result
            values.extend(items)
            if len(items) < 100:
                return values
        raise ValueError("pagination limit reached")


def load_artifact(raw):
    with zipfile.ZipFile(io.BytesIO(raw)) as archive:
        entries = archive.infolist()
        if len(entries) != 1 or entries[0].filename != "report.json":
            raise ValueError("unexpected artifact contents")
        entry = entries[0]
        if entry.file_size > MAX_BYTES or entry.is_dir():
            raise ValueError("report too large")
        # Never extract filenames or execute anything from this archive.
        data = archive.read(entry)
        if len(data) > MAX_BYTES:
            raise ValueError("report too large")
    return validate_report(json.loads(data))


def associated_prs(api, run):
    associated = run.get("pull_requests", [])
    if associated:
        return {entry["number"] for entry in associated}
    # Fork workflow runs may omit pull_requests; use the authenticated commit association.
    return {entry["number"] for entry in api.pages(f"/commits/{run['head_sha']}/pulls")}


def current_pr(api, run, number):
    if number not in associated_prs(api, run):
        return None
    pr = api.request(f"/pulls/{number}")
    # GitHub can expose the branch head or merge SHA for a PR workflow run.
    valid_run_shas = {pr["head"]["sha"], pr.get("merge_commit_sha")}
    if pr["state"] != "open" or run["head_sha"] not in valid_run_shas:
        return None
    if pr["base"]["repo"]["full_name"] != api.repo:
        return None
    return pr


def matches_run(report, run, pr):
    return (
        report["pr"] == pr["number"]
        and report["run_id"] == run["id"]
        and report["run_number"] == run["run_number"]
        and report["attempt"] == run["run_attempt"]
        and report["head_sha"] == pr["head"]["sha"]
        and report["base_sha"] == pr["base"]["sha"]
        and report["merge_sha"] == pr.get("merge_commit_sha")
    )


def publish(api, event):
    notice = event["workflow_run"]
    run = api.request(f"/actions/runs/{int(notice['id'])}")
    if (run["event"] != "pull_request" or run["status"] != "completed"
            or run["run_attempt"] != notice["run_attempt"]
            or run["repository"]["full_name"] != api.repo):
        print("Ignoring ineligible or superseded run attempt.")
        return

    reports = [item for item in api.pages(f"/actions/runs/{run['id']}/artifacts", "artifacts")
               if item["name"] == f"{ARTIFACT}-{run['run_attempt']}" and not item["expired"]]
    report = None
    if len(reports) == 1:
        try:
            artifact = reports[0]
            if artifact["size_in_bytes"] > MAX_BYTES:
                raise ValueError("artifact too large")
            report = load_artifact(api.request(f"/actions/artifacts/{int(artifact['id'])}/zip", binary=True))
        except (ValueError, TypeError, KeyError, zipfile.BadZipFile, RuntimeError, urllib.error.HTTPError):
            print("Performance artifact is invalid or unavailable; publishing no measurements.")

    numbers = [report["pr"]] if report else sorted(associated_prs(api, run))
    if len(numbers) != 1:
        print("Cannot uniquely associate run with a PR; no comment written.")
        return
    pr = current_pr(api, run, numbers[0])
    if not pr:
        print("Ignoring closed, unassociated or stale PR head.")
        return
    if report and not matches_run(report, run, pr):
        print("Ignoring stale or mismatched report provenance.")
        return

    run_url = (os.environ.get("GITHUB_SERVER_URL", "https://github.com").rstrip("/")
               + f"/{api.repo}/actions/runs/{run['id']}/attempts/{run['run_attempt']}")
    stamp = (f"{MARKER} run={run['id']} number={run['run_number']} "
             f"attempt={run['run_attempt']} -->")
    if report:
        body = render_report(report)
    else:
        body = ("## Performance changes (advisory)\n\n"
                "**Measurements unavailable.** This run did not produce a valid, complete report.\n\n"
                "No performance comparison is claimed. Existing quality checks are unchanged; "
                "this report does not block merging.\n")
    body = f"{stamp}\n{body}\n[Workflow run / logs]({run_url})\n"

    comments = api.pages(f"/issues/{pr['number']}/comments")
    owned = [comment for comment in comments
             if comment.get("user", {}).get("login") == "github-actions[bot]"
             and comment.get("user", {}).get("type") == "Bot"
             and STAMP.match(comment.get("body") or "")]
    existing = min(owned, key=lambda comment: comment["id"]) if owned else None
    if existing:
        match = STAMP.match(existing["body"])
        if (int(match[2]), int(match[3])) > (run["run_number"], run["run_attempt"]):
            print("A newer report is already published; leaving it unchanged.")
            return

    # Recheck after downloading/parsing/paginating: the PR may have advanced meanwhile.
    latest = current_pr(api, run, pr["number"])
    if (not latest or latest["head"]["sha"] != pr["head"]["sha"]
            or latest["base"]["sha"] != pr["base"]["sha"]
            or (report and not matches_run(report, run, latest))):
        print("PR changed during publication; no comment written.")
        return
    if existing:
        api.request(f"/issues/comments/{existing['id']}", "PATCH", {"body": body})
    else:
        api.request(f"/issues/{pr['number']}/comments", "POST", {"body": body})
    print(f"Updated advisory performance comment for PR #{pr['number']}.")


def main():
    event = json.loads(Path(os.environ["GITHUB_EVENT_PATH"]).read_text(encoding="utf-8"))
    publish(GitHub(), event)


if __name__ == "__main__":
    main()
