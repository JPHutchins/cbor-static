"""The toolchains, target directories and seeds that every camas project in the workspace shares.

Read from the workspace's own files once, at import.
"""

import tomllib
from functools import partial
from pathlib import Path
from typing import Final

from camas import Parallel, Task

from harness.manifests import FuzzManifest

ROOT: Final = Path(__file__).parent.parent
FEATURES: Final = frozenset(tomllib.loads((ROOT / "cbor-static" / "Cargo.toml").read_text())["features"])
RUSTS: Final = {"stable": ".", "nightly": "nightly"}
ENABLED: Final = {rust: FEATURES - (RUSTS.keys() - {rust}) for rust in RUSTS}


def features(enabled: frozenset[str]) -> str:
	"""Cargo's `--features` value for `enabled`, qualified by package.

	>>> features(frozenset({"std", "alloc"}))
	'cbor-static/alloc cbor-static/std'
	"""
	return " ".join(f"cbor-static/{feature}" for feature in sorted(enabled))


TOOLCHAIN: Final = {
	rust: {"RUST": rust, "DIR": directory, "FEATURES": features(ENABLED[rust])}
	for rust, directory in RUSTS.items()
}
TOOLCHAINS: Final = tuple(TOOLCHAIN.values())
TARGET_DIR: Final = f"{ROOT}/target"
STABLE_HOST: Final = {"CARGO_TARGET_DIR": f"{TARGET_DIR}/stable-host"}
CONFORMANCE: Final = "-p cbor-static-conformance"
FUZZ_TARGETS: Final = tuple(target.name for target in FuzzManifest.read(ROOT).targets)
FUZZ_TARGET_DIR: Final = f"{TARGET_DIR}/fuzz"
FUZZ_SEEDS: Final = f"{FUZZ_TARGET_DIR}/seeds"
CONFORMANCE_HOST: Final = f"{CONFORMANCE} --features '{TOOLCHAIN['stable']['FEATURES']}'"
PROPTEST_RNG_SEED: Final = "PROPTEST_RNG_SEED"
PINNED_SEED: Final = "20261120"
SEED: Final = "SEED"
seeded: Final = partial(Parallel, matrix={SEED: (PINNED_SEED,)}, env={PROPTEST_RNG_SEED: f"{{{SEED}}}"})
nightly_miri: Final = partial(
	Task,
	cwd=ROOT / TOOLCHAIN["nightly"]["DIR"],
	env={
		"CARGO_TARGET_DIR": f"{TARGET_DIR}/miri",
		PROPTEST_RNG_SEED: PINNED_SEED,
		"MIRIFLAGS": f"-Zmiri-env-forward={PROPTEST_RNG_SEED}",
	},
)
