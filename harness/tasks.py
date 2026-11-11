"""The Python size harness, its checks, and the Phase 0 measurements it drives."""

from camas import Claude, Config, Parallel, Task

from harness.backends import BACKENDS
from harness.backends.cbor_static import CBOR_STATIC
from harness.backends.minicbor import MINICBOR_HAND_TEXTKEYS
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
crosscheck = Task("uv run python -m harness.crosscheck")
ram = Task("uv run python -m harness.ram")
phase0 = (sweep | sweep_v6m | crosscheck | pay_for_what_you_use | ram) + machinery + gate

_ = Config(default_task=all, github_task=check, agent=Claude(fix=fix, check=check))
