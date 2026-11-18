"""R-208: the stack-size baseline and its check."""

from camas import Config, Task

fix = Task("uv run python -m harness.stack.frames fix", mutates=True)
check = Task("uv run python -m harness.stack.frames check")

_ = Config(default_task=check, github_task=check)
