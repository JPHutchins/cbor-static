"""The protocol types against their published vectors."""

from camas import Claude, Config, Parallel, Task

from harness.slices import slices
from harness.workspace import CONFORMANCE, CONFORMANCE_HOST, STABLE_HOST, TOOLCHAIN, nightly_miri, seeded

HOST_TEST = f"cargo test {CONFORMANCE_HOST}"
HOST_RUN = f"cargo run -q {CONFORMANCE_HOST}"
MIRI_MODULES = ("messages", "headers", "keys", "records")

fix = Task("uv run ruff check --fix --exit-zero {paths}", paths="tasks.py", mutates=True) + Task(
	"uv run ruff format {paths}", paths="tasks.py", mutates=True
)
test = seeded(Task(HOST_TEST, env=STABLE_HOST))
slices_cover_the_tests = Task(
	f'uv run python -m harness.slices {" ".join(MIRI_MODULES)} --cargo "{HOST_TEST} --tests"',
	env=STABLE_HOST,
	help="the Miri slices list exactly the host run's tests",
)
smp_oracle = Task(
	f'uv run python -m harness.smp_oracle --example "{HOST_RUN} --example smp_encodings"',
	env=STABLE_HOST,
	help="JPHutchins/smp reads what the SMP types write exactly when cbor-static reads it, but pinned shapes",
)
check = (
	Task("uv run ruff format --check {paths}", paths="tasks.py")
	| Task("uv run ruff check {paths}", paths="tasks.py")
	| Task("uv run mypy tasks.py")
	| (test + slices_cover_the_tests + smp_oracle)
)
NIGHTLY = f"{CONFORMANCE} --features '{TOOLCHAIN['nightly']['FEATURES']}'"
miri = Parallel(
	nightly_miri(f"cargo miri test {NIGHTLY} --tests -- {{SLICE}}"),
	matrix={"SLICE": slices(MIRI_MODULES)},
) | nightly_miri(f"cargo miri test {NIGHTLY} --doc")
ci = check | miri
all = fix + check

_ = Config(default_task=all, github_task=ci, agent=Claude(fix=fix, check=check))
