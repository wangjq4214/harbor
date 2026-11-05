"""Explicit, synthetic child-shell checks. No user/remote profile files are changed."""
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parent
ARGS = ["", "a b", "q'\"\\", "世界", "--resume"]


def quote(value):
    return "'" + str(value).replace("'", "'\"'\"'") + "'"


def psquote(value):
    return "'" + str(value).replace("'", "''") + "'"


def posix(path):
    value = str(path).replace("\\", "/")
    if len(value) > 2 and value[1] == ":":
        return "/" + value[0].lower() + value[2:]
    return value


def events(output):
    result = []
    for match in re.finditer(rb"\x1b\]777;harbor;1;([^\x07]+)\x07", output):
        parts = match[1].decode("ascii").split(";")
        result.append([parts[0]] + [bytes.fromhex(p).decode("utf-8") for p in parts[1:]])
    return result


class ProjectIntegration(unittest.TestCase):
    def test_pwsh_context_and_original_argv_after_agent_exit(self):
        executable = shutil.which("pwsh")
        self.assertIsNotNone(executable, "PowerShell 7.3+ required")
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            project = root / "after space"
            project.mkdir()
            report = root / "args.txt"
            (root / "pi.ps1").write_text(
                "[IO.File]::WriteAllText($env:HARBOR_SYNTHETIC_REPORT, "
                "(($args | ForEach-Object { [Convert]::ToHexString([Text.Encoding]::UTF8.GetBytes($_)) }) -join ';'))\n",
                encoding="utf-8",
            )
            script = (
                f". {psquote(ROOT / 'harbor-project.ps1')}; "
                "$handler = (Get-PSReadLineOption).AddToHistoryHandler; "
                "$null = $handler.Invoke('pi'); "
                f"pi {' '.join(map(psquote, ARGS))}; "
                "prompt | Out-Null; "
                "$null = $handler.Invoke('Set-Location'); "
                f"Set-Location -LiteralPath {psquote(project)}; prompt | Out-Null; "
                "$null = $handler.Invoke('Get-Date'); Get-Date | Out-Null; prompt | Out-Null; "
                "$null = $handler.Invoke('pi; Set-Location C:/'); "
                "pi compound; prompt | Out-Null;"
            )
            env = os.environ.copy()
            env["PATH"] = str(root) + os.pathsep + env["PATH"]
            env["HARBOR_SYNTHETIC_REPORT"] = str(report)
            output = subprocess.run([executable, "-NoLogo", "-NoProfile", "-NonInteractive", "-Command", script],
                                    env=env, capture_output=True, timeout=20)
            self.assertEqual(output.returncode, 0, "isolated pwsh integration failed")
            observed = events(output.stdout)
            agent = next(e for e in observed if e[0] == "agent")
            self.assertEqual(agent[1:3], ["pwsh", ""])
            self.assertEqual(agent[4:], ["pi"] + ARGS)
            contexts = [e for e in observed if e[0] == "context"]
            self.assertEqual(Path(contexts[-1][3]).resolve(), project.resolve())
            self.assertTrue(any(e[0] == "busy" for e in observed))
            self.assertEqual(next(e for e in observed if e[0] == "record")[4:], ["pi", "compound"])
            self.assertEqual(report.read_text(), "636F6D706F756E64")

    def test_bash_cooperating_environment_context_and_original_argv(self):
        executable = os.environ.get("HARBOR_TEST_BASH") or shutil.which("bash")
        self.assertIsNotNone(executable, "bash with od/tr required")
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            project = root / "after space"
            project.mkdir()
            report = root / "args.txt"
            shim = root / "pi"
            shim.write_text('#!/usr/bin/env bash\nprintf "%s\\0" "$@" > "$HARBOR_SYNTHETIC_REPORT"\n', encoding="utf-8")
            shim.chmod(0o700)
            script = (
                f"source {quote(posix(ROOT / 'harbor-project.bash'))} ssh synthetic-host\n"
                f"pi {' '.join(map(quote, ARGS))}\n"
                f"cd -- {quote(posix(project))}\n"
                "true\nexit\n"
            )
            env = os.environ.copy()
            env["PATH"] = str(root) + os.pathsep + env["PATH"]
            env["HARBOR_SYNTHETIC_REPORT"] = posix(report)
            output = subprocess.run([executable, "--noprofile", "--norc", "-i"], input=script.encode("utf-8"),
                                    env=env, capture_output=True, timeout=20)
            self.assertEqual(output.returncode, 0, "isolated bash integration failed")
            observed = events(output.stdout)
            agent = next(e for e in observed if e[0] == "agent")
            self.assertEqual(agent[1:3], ["ssh", "synthetic-host"])
            self.assertEqual(agent[4:], ["pi"] + ARGS)
            self.assertEqual([e for e in observed if e[0] == "context"][-1][3], posix(project))
            self.assertEqual(report.read_bytes().split(b"\0")[:-1], [a.encode("utf-8") for a in ARGS])


if __name__ == "__main__":
    unittest.main()
