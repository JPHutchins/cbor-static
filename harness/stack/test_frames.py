import pytest

from harness.stack.frames import app


def test_no_command_fails_so_a_task_that_drops_it_cannot_pass_the_gate() -> None:
	with pytest.raises(SystemExit) as exited:
		app([])
	assert exited.value.code == 2
