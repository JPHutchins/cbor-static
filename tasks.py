"""Every check for cbor-static, local and CI."""

import os
import shlex
import sys
import tomllib

from camas import Claude, Config, Parallel, Project, Task

from harness.workspace import (
	ENABLED,
	ROOT,
	SEED,
	TARGET_DIR,
	TOOLCHAIN,
	TOOLCHAINS,
	features,
	nightly_miri,
	seeded,
)

if "CBOR_STATIC_DEV_SHELL" not in os.environ and not any(
	arg.startswith("--github-matrix") for arg in sys.argv
):
	sys.exit("cbor-static: run camas inside the dev shell, e.g. `nix develop -c camas <task>`")

TRIPLES = tuple(tomllib.loads((ROOT / "rust-toolchain.toml").read_text())["toolchain"]["targets"])
BARE_TOOLCHAINS = tuple(
	{**toolchain, "FEATURES": features(ENABLED[toolchain["RUST"]] - {"std"})} for toolchain in TOOLCHAINS
)
MIRI_SHARDS = tuple(str(shard) for shard in range(1, 5))
MIRI_PACKAGE = f"-p cbor-static --features '{TOOLCHAIN['nightly']['FEATURES']}'"


conformance = Project("cbor-static-conformance")
harness = Project("harness")
stack = Project("harness/stack")

rust_fmt = Task("cargo fmt --all", mutates=True)
tasks_fix = Task("uv run ruff check --fix --exit-zero {paths}", paths="tasks.py", mutates=True) + Task(
	"uv run ruff format {paths}", paths="tasks.py", mutates=True
)
tasks_check = (
	Task("uv run ruff format --check {paths}", paths="tasks.py")
	| Task("uv run ruff check {paths}", paths="tasks.py")
	| Task("uv run mypy tasks.py")
)
rust_fmt_check = Task("cargo fmt --all --check")
nix_fmt = Task("treefmt", mutates=True)
nix_fmt_check = Task("treefmt --ci")
flake = Task("nix flake check")
clippy = Parallel(
	Task(
		"cargo clippy --workspace --all-targets --features '{FEATURES}' -- -D warnings",
		cwd="{DIR}",
		env={"CARGO_TARGET_DIR": f"{TARGET_DIR}/{{RUST}}-clippy"},
	),
	variants=TOOLCHAINS,
)
unit = seeded(
	Task(
		f"cargo test --workspace --exclude cbor-static-conformance --features '{TOOLCHAIN['stable']['FEATURES']}'",
		env={"CARGO_TARGET_DIR": f"{TARGET_DIR}/stable-host"},
	),
)
usize16 = seeded(
	Task(
		"cargo test -p cbor-static",
		env={
			"CARGO_TARGET_DIR": f"{TARGET_DIR}/usize16-host",
			"RUSTFLAGS": "--cfg cbor_static_usize16 -D warnings",
		},
	),
	help="R-103: wire lengths capped at u16::MAX, as on a 16-bit target; also the no-feature build, warnings denied",
)
serde_bare = seeded(
	Task(
		"cargo test -p cbor-static --features serde",
		env={"CARGO_TARGET_DIR": f"{TARGET_DIR}/serde-host", "RUSTFLAGS": "-D warnings"},
	),
	help="D-13a: the serde layer without alloc, which refuses strings in chunks it would assemble with it",
)
oneshot = seeded(
	Task(
		f"cargo test -p cbor-static --features '{TOOLCHAIN['stable']['FEATURES']}' --test oneshot --test sizes",
		env={
			"CARGO_TARGET_DIR": f"{TARGET_DIR}/oneshot-host",
			"RUSTFLAGS": "--cfg cbor_static_oneshot -D warnings",
		},
	),
	help="the one-shot engine build, on whole input",
)
test = seeded(
	Task(
		"cargo test --workspace --exclude cbor-static-conformance --features '{FEATURES}'",
		cwd="{DIR}",
		env={"CARGO_TARGET_DIR": f"{TARGET_DIR}/{{RUST}}-host"},
	),
	variants=TOOLCHAINS,
)
build = Parallel(
	Task(
		"cargo build -p cbor-static --release --target {TRIPLE} --features '{FEATURES}'",
		cwd="{DIR}",
		env={"CARGO_TARGET_DIR": f"{TARGET_DIR}/{{RUST}}-{{TRIPLE}}"},
	),
	variants=BARE_TOOLCHAINS,
	matrix={"TRIPLE": TRIPLES},
)
miri = Parallel(
	nightly_miri(
		f"cargo miri nextest run {MIRI_PACKAGE} --no-fail-fast --partition slice:{{SHARD}}/{len(MIRI_SHARDS)}"
	),
	matrix={"SHARD": MIRI_SHARDS},
)
miri_doc = nightly_miri(f"cargo miri test {MIRI_PACKAGE} --doc", help="nextest runs no doctests")
fix = rust_fmt | tasks_fix | nix_fmt | harness | conformance
check = (
	rust_fmt_check
	| nix_fmt_check
	| clippy
	| unit
	| usize16
	| serde_bare
	| oneshot
	| tasks_check
	| harness
	| conformance
)
f32 = seeded(
	Task(
		"cargo test --release -p cbor-static --test floats -- --ignored",
		env={"CARGO_TARGET_DIR": f"{TARGET_DIR}/release-host"},
	),
	help="R-101, R-135: all 2^32 binary32 patterns",
)
diagnostics = seeded(
	Task(
		f"cargo test -p cbor-static --features '{TOOLCHAIN['stable']['FEATURES']}' --test diagnostics -- --ignored",
		env={"CARGO_TARGET_DIR": f"{TARGET_DIR}/stable-host"},
	),
	help="Q16: derive diagnostics under trybuild, whose expected stderr is the pinned stable rustc's",
)
properties = test | usize16 | serde_bare | oneshot
kani = Task("cargo kani -p cbor-static", env={"CARGO_TARGET_DIR": f"{TARGET_DIR}/kani"})
ci_jobs = (check - unit) | test | build | miri_doc | f32 | diagnostics | kani | flake | stack
ci = ci_jobs | miri
daily_properties = Task(
	f"uv run python -m harness.daily seeded properties --axis {SEED} --out {shlex.quote(f'{TARGET_DIR}/daily')}",
	help="Q18: the property suites on a fresh seed, reported for the daily workflow",
)
daily_jobs = Parallel(daily_properties)
all = fix + check

_ = Config(default_task=all, github_task=ci, agent=Claude(fix=fix, check=check))
