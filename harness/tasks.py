"""The Python size harness: lint, types and doctests."""

from camas import Claude, Config, Task

fix = Task("uv run ruff check --fix {paths}", paths=".", mutates=True) + Task(
	"uv run ruff format {paths}", paths=".", mutates=True
)
check = (
	Task("uv run ruff format --check {paths}", paths=".")
	| Task("uv run ruff check {paths}", paths=".")
	| Task("uv run mypy", cwd="..")
	| Task("uv run pytest --doctest-modules -q harness", cwd="..")
)
all = fix + check

_ = Config(default_task=all, github_task=check, agent=Claude(fix=fix, check=check))
