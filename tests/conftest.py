"""The Rust `daw` every test drives.

The release build, which `uv run daw` also runs, is brought up to date once per
run; AAW_DAW names another binary to test instead. The Python CLI and the test
helpers find it through AAW_DAW, and the binary finds this interpreter through
AAW_PYTHON for the commands it passes on.
"""

import os
import shutil
import subprocess
import sys
from pathlib import Path

import pytest

ENGINE = Path(__file__).resolve().parents[1] / "engine"


@pytest.fixture(scope="session", autouse=True)
def rust_daw():
    os.environ["AAW_PYTHON"] = sys.executable
    override = os.environ.get("AAW_DAW")
    if override:
        return Path(override)
    cargo = shutil.which("cargo") or str(Path.home() / ".cargo" / "bin" / "cargo")
    subprocess.run([cargo, "build", "--quiet", "--release", "-p", "aaw-cli"], cwd=ENGINE, check=True)
    binary = ENGINE / "target" / "release" / "daw"
    os.environ["AAW_DAW"] = str(binary)
    return binary
