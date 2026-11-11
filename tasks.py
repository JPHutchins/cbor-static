"""Every check for cbor-static, local and CI."""

import tomllib
from pathlib import Path

from camas import Claude, Config, Parallel, Sequential, Task, by_suffix

TOOLCHAIN_FILE = tomllib.loads(Path("rust-toolchain.toml").read_text())["toolchain"]
STABLE: str = TOOLCHAIN_FILE["channel"]
NIGHTLY = "nightly-2026-11-10"
TOOLCHAINS = (STABLE, NIGHTLY)
TRIPLES: tuple[str, ...] = tuple(TOOLCHAIN_FILE["targets"])
PYTHON = by_suffix((".py",), default=("harness", "tasks.py"))


def target_dir(name: str) -> dict[str, str]:
	return {"CARGO_TARGET_DIR": f".camas/target/{name}"}


rust_fmt = Task("cargo fmt --all", mutates=True)
py_fmt = Task("uv run ruff format {paths}", paths=PYTHON, mutates=True)
py_fix = Task("uv run ruff check --fix {paths}", paths=PYTHON, mutates=True)
fix = Sequential(rust_fmt, py_fmt, py_fix)

fmt = Parallel(
	Task("cargo fmt --all --check"),
	Task("uv run ruff format --check {paths}", paths=PYTHON),
)
clippy = Task(
	"cargo clippy --workspace --all-targets -- -D warnings",
	env=target_dir(f"{STABLE}-clippy"),
)
test = Parallel(
	Task("cargo +{TOOLCHAIN} test --workspace", env=target_dir("{TOOLCHAIN}-host")),
	matrix={"TOOLCHAIN": TOOLCHAINS},
)
build = Parallel(
	Task(
		"cargo +{TOOLCHAIN} build -p cbor-static --release --target {TRIPLE}",
		env=target_dir("{TOOLCHAIN}-{TRIPLE}"),
	),
	matrix={"TOOLCHAIN": TOOLCHAINS, "TRIPLE": TRIPLES},
)
miri = Task(f"cargo +{NIGHTLY} miri test -p cbor-static", env=target_dir(f"{NIGHTLY}-miri"))
python = Parallel(
	Task("uv run ruff check {paths}", paths=PYTHON),
	Task("uv run mypy"),
	Task("uv run pytest --doctest-modules -q harness"),
)
doctor = Task(
	(
		"uv",
		"run",
		"python",
		"-m",
		"harness.doctor",
		"--stable",
		STABLE,
		"--nightly",
		NIGHTLY,
		"--targets",
		*TRIPLES,
	)
)

check = Parallel(fmt, clippy, python, Task("cargo test --workspace", env=target_dir(f"{STABLE}-host")))
ci = Parallel(fmt, clippy, test, build, miri, python)

_ = Config(
	default_task=check,
	github_task=ci,
	agent=Claude(fix=fix, check=check),
)
