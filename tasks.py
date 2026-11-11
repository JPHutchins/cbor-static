"""Every check for cbor-static, local and CI."""

import tomllib
from pathlib import Path

from camas import Claude, Config, Parallel, Project, Task

ROOT = Path(__file__).parent
TRIPLES = tuple(tomllib.loads((ROOT / "rust-toolchain.toml").read_text())["toolchain"]["targets"])
TOOLCHAINS = ({"RUST": "stable", "DIR": "."}, {"RUST": "nightly", "DIR": "nightly"})
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
clippy = Task(
	"cargo clippy --workspace --all-targets -- -D warnings",
	env={"CARGO_TARGET_DIR": f"{TARGET_DIR}/clippy"},
)
unit = Task("cargo test --workspace", env={"CARGO_TARGET_DIR": f"{TARGET_DIR}/stable-host"})
usize16 = Task(
	"cargo test -p cbor-static",
	env={"CARGO_TARGET_DIR": f"{TARGET_DIR}/usize16-host", "RUSTFLAGS": "--cfg cbor_static_usize16"},
	help="R-103: wire lengths capped at u16::MAX, as on a 16-bit target",
)
test = Parallel(
	Task("cargo test --workspace", cwd="{DIR}", env={"CARGO_TARGET_DIR": f"{TARGET_DIR}/{{RUST}}-host"}),
	variants=TOOLCHAINS,
)
build = Parallel(
	Task(
		"cargo build -p cbor-static --release --target {TRIPLE}",
		cwd="{DIR}",
		env={"CARGO_TARGET_DIR": f"{TARGET_DIR}/{{RUST}}-{{TRIPLE}}"},
	),
	variants=TOOLCHAINS,
	matrix={"TRIPLE": TRIPLES},
)
miri = Task("cargo miri test -p cbor-static", cwd="nightly", env={"CARGO_TARGET_DIR": f"{TARGET_DIR}/miri"})
doctor = (
	Task("git submodule update --init")
	| (
		Task("rustup toolchain install", help="installs what rust-toolchain.toml declares")
		+ Task("rustup toolchain install", cwd="nightly")
	)
	| Task("uv sync --locked")
)

fix = rust_fmt | tasks_fix | harness
check = rust_fmt_check | clippy | unit | usize16 | tasks_check | harness
ci = (check - unit) | test | build | miri
all = fix + check

_ = Config(default_task=all, github_task=ci, agent=Claude(fix=fix, check=check))
