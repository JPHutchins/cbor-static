"""Every check for cbor-static, local and CI."""

import os
import sys
import tomllib
from pathlib import Path

from camas import Claude, Config, Parallel, Project, Task

if "CBOR_STATIC_DEV_SHELL" not in os.environ and not any(
	arg.startswith("--github-matrix") for arg in sys.argv
):
	sys.exit("cbor-static: run camas inside the dev shell, e.g. `nix develop -c camas <task>`")

ROOT = Path(__file__).parent
TRIPLES = tuple(tomllib.loads((ROOT / "rust-toolchain.toml").read_text())["toolchain"]["targets"])
FEATURES = tuple(tomllib.loads((ROOT / "cbor-static" / "Cargo.toml").read_text())["features"])
RUSTS = {"stable": ".", "nightly": "nightly"}
TOOLCHAIN = {
	rust: {
		"RUST": rust,
		"DIR": directory,
		"FEATURES": " ".join(
			f"cbor-static/{feature}" for feature in FEATURES if feature not in RUSTS.keys() - {rust}
		),
	}
	for rust, directory in RUSTS.items()
}
TOOLCHAINS = tuple(TOOLCHAIN.values())
TARGET_DIR = f"{ROOT}/target"

harness = Project("harness")

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
unit = Task(
	f"cargo test --workspace --features '{TOOLCHAIN['stable']['FEATURES']}'",
	env={"CARGO_TARGET_DIR": f"{TARGET_DIR}/stable-host"},
)
usize16 = Task(
	"cargo test -p cbor-static",
	env={"CARGO_TARGET_DIR": f"{TARGET_DIR}/usize16-host", "RUSTFLAGS": "--cfg cbor_static_usize16"},
	help="R-103: wire lengths capped at u16::MAX, as on a 16-bit target",
)
test = Parallel(
	Task(
		"cargo test --workspace --features '{FEATURES}'",
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
	variants=TOOLCHAINS,
	matrix={"TRIPLE": TRIPLES},
)
miri = Task(
	f"cargo miri test -p cbor-static --features '{TOOLCHAIN['nightly']['FEATURES']}'",
	cwd=TOOLCHAIN["nightly"]["DIR"],
	env={"CARGO_TARGET_DIR": f"{TARGET_DIR}/miri"},
)
fix = rust_fmt | tasks_fix | nix_fmt | harness
check = rust_fmt_check | nix_fmt_check | clippy | unit | usize16 | tasks_check | harness
f32 = Task(
	"cargo test --release -p cbor-static --test floats -- --ignored",
	env={"CARGO_TARGET_DIR": f"{TARGET_DIR}/release-host"},
	help="R-101, R-135: all 2^32 binary32 patterns",
)
kani = Task("cargo kani -p cbor-static", env={"CARGO_TARGET_DIR": f"{TARGET_DIR}/kani"})
ci = (check - unit) | test | build | miri | f32 | kani | flake
all = fix + check

_ = Config(default_task=all, github_task=ci, agent=Claude(fix=fix, check=check))
