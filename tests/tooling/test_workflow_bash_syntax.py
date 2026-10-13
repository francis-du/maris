import re
import subprocess
from pathlib import Path
from unittest import TestCase

ROOT = Path(__file__).resolve().parents[2]


def bash_blocks(path: Path):
    lines = path.read_text(encoding="utf-8").splitlines()
    for index, line in enumerate(lines):
        if line.strip() != "shell: bash":
            continue
        shell_indent = len(line) - len(line.lstrip())
        run_index = None
        for cursor in range(index + 1, len(lines)):
            candidate = lines[cursor]
            if candidate.strip():
                indent = len(candidate) - len(candidate.lstrip())
                if indent < shell_indent:
                    break
            if candidate.strip() == "run: |":
                run_index = cursor
                break
        if run_index is None:
            continue

        run_indent = len(lines[run_index]) - len(lines[run_index].lstrip())
        raw = []
        for candidate in lines[run_index + 1:]:
            if candidate.strip():
                indent = len(candidate) - len(candidate.lstrip())
                if indent <= run_indent:
                    break
            raw.append(candidate)
        nonempty = [line for line in raw if line.strip()]
        if not nonempty:
            raise AssertionError(f"{path}: empty bash run block after line {run_index + 1}")
        body_indent = min(len(line) - len(line.lstrip()) for line in nonempty)
        script = "\n".join(line[body_indent:] if line.strip() else "" for line in raw) + "\n"
        script = re.sub(r"\$\{\{.*?\}\}", "GITHUB_EXPR", script)
        yield run_index + 1, script


class WorkflowBashSyntax(TestCase):
    def test_every_declared_bash_workflow_block_parses(self):
        workflows = sorted((ROOT / ".github/workflows").glob("*.yml"))
        self.assertTrue(workflows)
        checked = 0
        for workflow in workflows:
            for line, script in bash_blocks(workflow):
                checked += 1
                result = subprocess.run(
                    ["bash", "-n"],
                    input=script,
                    text=True,
                    capture_output=True,
                    check=False,
                )
                self.assertEqual(
                    result.returncode,
                    0,
                    f"{workflow.relative_to(ROOT)}:{line}: {result.stderr}\n{script}",
                )
        self.assertGreater(checked, 0)
