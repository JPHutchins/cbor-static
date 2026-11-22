"""The protocol types against their published vectors."""

from camas import Claude, Config, Parallel, Task

from harness.slices import slices
from harness.workspace import TARGET_DIR, TOOLCHAIN, nightly_miri, seeded

PACKAGE = "-p cbor-static-conformance"
HOST_TARGET = {"CARGO_TARGET_DIR": f"{TARGET_DIR}/stable-host"}
HOST = f"{PACKAGE} --features '{TOOLCHAIN['stable']['FEATURES']}'"
HOST_TEST = f"cargo test {HOST}"
HOST_RUN = f"cargo run -q {HOST}"
MIRI_MODULES = ("messages", "headers", "keys", "records")

fix = Task("uv run ruff check --fix --exit-zero {paths}", paths="tasks.py", mutates=True) + Task(
	"uv run ruff format {paths}", paths="tasks.py", mutates=True
)
test = seeded(Task(HOST_TEST, env=HOST_TARGET))
slices_cover_the_tests = Task(
	f'uv run python -m harness.slices {" ".join(MIRI_MODULES)} --cargo "{HOST_TEST} --tests"',
	env=HOST_TARGET,
	help="the Miri slices list exactly the host run's tests",
)
smp_oracle = Task(
	f'uv run python -m harness.smp_oracle --example "{HOST_RUN} --example smp_encodings"',
	env=HOST_TARGET,
	help="JPHutchins/smp reads what the SMP types write exactly when cbor-static reads it, but pinned shapes",
)
check = (
	Task("uv run ruff format --check {paths}", paths="tasks.py")
	| Task("uv run ruff check {paths}", paths="tasks.py")
	| Task("uv run mypy tasks.py")
	| (test + slices_cover_the_tests + smp_oracle)
)
NIGHTLY = f"{PACKAGE} --features '{TOOLCHAIN['nightly']['FEATURES']}'"
miri = Parallel(
	nightly_miri(f"cargo miri test {NIGHTLY} --tests -- {{SLICE}}"),
	matrix={"SLICE": slices(MIRI_MODULES)},
) | nightly_miri(f"cargo miri test {NIGHTLY} --doc")
ci = check | miri
all = fix + check

_ = Config(default_task=all, github_task=ci, agent=Claude(fix=fix, check=check))
