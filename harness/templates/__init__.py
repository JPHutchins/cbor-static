"""Rust and Cargo sources the harness generates, with ``$name`` placeholders (``string.Template``)."""

from pathlib import Path
from string import Template


def render(template: str, /, **values: str) -> str:
	"""Fill every placeholder of ``template``; a missing value is an error.

	>>> render("roundtrip.rs", fn_name="f", decode="d()", encode="e(m)").splitlines()[0]
	'fn f(input: &[u8], out: &mut [u8]) -> usize {'
	"""
	return Template((Path(__file__).parent / template).read_text()).substitute(values)
