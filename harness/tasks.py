"""The Python size harness, its checks, and the Phase 0 and R-009 measurements it drives."""

from camas import Claude, Config, Parallel, Task

from harness.backends import BACKENDS
from harness.backends.cbor_static import CBOR_STATIC
from harness.backends.minicbor import MINICBOR_HAND_TEXTKEYS
from harness.features import BY_TEMPLATE
from harness.model import SHAPES

fix = Task("uv run ruff check --fix --exit-zero {paths}", paths=".", mutates=True) + Task(
	"uv run ruff format {paths}", paths=".", mutates=True
)
check = (
	Task("uv run ruff format --check {paths}", paths=".")
	| Task("uv run ruff check {paths}", paths=".")
	| Task("uv run mypy .")
	| Task("uv run pytest --doctest-modules -q")
)
all = fix + check

sweep = Parallel(
	Task("uv run python -m harness.sweep --backend {BACKEND} --shape {SHAPE}"),
	matrix={"BACKEND": tuple(BACKENDS), "SHAPE": tuple(SHAPES)},
)
sweep_v6m = Parallel(
	Task("uv run python -m harness.sweep --backend {BACKEND} --shape {SHAPE} --target thumbv6m-none-eabi"),
	matrix={"BACKEND": (CBOR_STATIC.name, MINICBOR_HAND_TEXTKEYS.name), "SHAPE": tuple(SHAPES)},
	help="informational (R-007); D-06 gates thumbv7m only",
)
machinery = Task("uv run python -m harness.symbols machinery")
gate = Task("uv run python -m harness.gate")
size_gate = sweep + machinery + gate
pay_for_what_you_use = Task("uv run python -m harness.symbols fixtures")
unused = Task("uv run python -m harness.unused")
panics = Task(
	"uv run python -m harness.panics", help="R-132: no core::panicking in a cbor-static thumbv7m ELF"
)
crosscheck = Task("uv run python -m harness.crosscheck")
ram = Task("uv run python -m harness.ram")
phase0 = (sweep | sweep_v6m | crosscheck | pay_for_what_you_use | ram) + machinery + gate + unused + panics
features = Parallel(
	Task("uv run python -m harness.features --feature {FEATURE}"),
	matrix={"FEATURE": tuple(BY_TEMPLATE)},
	help="R-009: each feature's fixture against its reference, thumbv7m, N=1 and 8",
)
r009 = (sweep | features) + machinery + gate + Task("uv run python -m harness.r009") + panics
ci = (check | crosscheck | pay_for_what_you_use | size_gate + unused) + panics

_ = Config(default_task=all, github_task=ci, agent=Claude(fix=fix, check=check))
