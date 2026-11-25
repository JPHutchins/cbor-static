"""cargo-fuzz targets on the pinned nightly: built, and each one's seeds replayed once, unmutated."""

from camas import Claude, Config, Parallel, Task

from harness.manifests import RootManifest
from harness.workspace import (
	FUZZ_CORPUS,
	FUZZ_SEEDS,
	FUZZ_TARGET_DIR,
	FUZZ_TARGETS,
	ROOT,
	TARGET_DIR,
	TOOLCHAIN,
)

NIGHTLY = ROOT / TOOLCHAIN["nightly"]["DIR"]
CARGO_FUZZ = f"cargo fuzz {{}} --fuzz-dir {ROOT / 'fuzz'} --debug-assertions"
FUZZ_ENV = {"CARGO_TARGET_DIR": FUZZ_TARGET_DIR}
SEEDS = f"{FUZZ_SEEDS}/{{TARGET}}"
CORPUS = f"{FUZZ_CORPUS}/{{TARGET}}"
ARTIFACTS = f"{FUZZ_TARGET_DIR}/artifacts"

fixtures = Task("uv run python -m harness.fuzz fixtures", cwd=ROOT)
fix = Task("uv run ruff check --fix --exit-zero {paths}", paths="tasks.py", mutates=True) + Task(
	"uv run ruff format {paths}", paths="tasks.py", mutates=True
) | fixtures + Task("cargo fmt", mutates=True)
lints = Task(
	f"cargo clippy --manifest-path {ROOT / 'fuzz' / 'Cargo.toml'} --all-targets -- -D warnings {RootManifest.read(ROOT).workspace.lints.flags()}",
	cwd=NIGHTLY,
	env={"CARGO_TARGET_DIR": f"{TARGET_DIR}/fuzz-clippy"},
)
static = (
	Task("uv run ruff format --check {paths}", paths="tasks.py")
	| Task("uv run ruff check {paths}", paths="tasks.py")
	| Task("uv run mypy tasks.py")
	| Task("cargo fmt --check")
	| Task("uv run python -m harness.fuzz check", cwd=ROOT)
)
check = fixtures + static
test = Task(
	f"cargo test --manifest-path {ROOT / 'fuzz' / 'Cargo.toml'} --lib",
	cwd=NIGHTLY,
	env={"CARGO_TARGET_DIR": f"{TARGET_DIR}/fuzz-test"},
)
build = Task(CARGO_FUZZ.format("build"), cwd=NIGHTLY, env=FUZZ_ENV)
seeds = Task("uv run python -m harness.fuzz seeds", cwd=ROOT)
replay = Parallel(
	Task(f"{CARGO_FUZZ.format('run')} {{TARGET}} {SEEDS} -- -runs=0", cwd=NIGHTLY, env=FUZZ_ENV),
	matrix={"TARGET": FUZZ_TARGETS},
	help="each target's seeds, run once each without mutation",
)
reproduce = Parallel(
	Task(f"{CARGO_FUZZ.format('run')} {{TARGET}} {{INPUT}} -- -runs=0", cwd=NIGHTLY, env=FUZZ_ENV),
	matrix={"TARGET": FUZZ_TARGETS, "INPUT": (ARTIFACTS,)},
	help="one target on the crash inputs fuzz.fuzz wrote, or with --INPUT=<file> on one the daily run uploaded",
)
fuzz = Parallel(
	fixtures
	+ Task("uv run python -m harness.fuzz seeds {TARGET}", cwd=ROOT)
	+ build
	+ Task(
		f"{CARGO_FUZZ.format('run')} {{TARGET}} {CORPUS} {SEEDS} -- -fork={{FORK}} -max_total_time={{SECONDS}} "
		"-rss_limit_mb=2048 -artifact_prefix={ARTIFACTS}/",
		cwd=NIGHTLY,
		env=FUZZ_ENV,
	)
	+ Task(f"{CARGO_FUZZ.format('cmin')} {{TARGET}} {CORPUS}", cwd=NIGHTLY, env=FUZZ_ENV),
	matrix={
		"TARGET": FUZZ_TARGETS,
		"SECONDS": ("60",),
		"FORK": ("1",),
		"ARTIFACTS": (ARTIFACTS,),
	},
	help="fuzz one target from its seeds and its corpus, which it minimizes after; crash inputs go to ARTIFACTS",
)
ci = fixtures + (static | lints | test | build) + seeds + replay
all = fix + check

_ = Config(default_task=all, github_task=ci, agent=Claude(fix=fix, check=check))
