"""
Ensure required commands (uvx, npx, etc.) are installed before or during setup.
Installs when possible without sudo; otherwise prints instructions.
"""
from __future__ import annotations

import os
import shutil
import subprocess
from pathlib import Path
from typing import Callable


def _extended_path() -> str:
    """PATH with common user install dirs so we detect freshly installed tools."""
    home = Path.home()
    extra = [
        str(home / ".local" / "bin"),
        str(home / ".cargo" / "bin"),
    ]
    # nvm default node path (approximate; exact path depends on version)
    nvm_dir = home / ".nvm" / "versions" / "node"
    if nvm_dir.is_dir():
        for v in nvm_dir.iterdir():
            if v.is_dir():
                extra.append(str(v / "bin"))
                break
    return os.pathsep.join(extra + [os.environ.get("PATH", "")])


def find_command(cmd: str) -> str | None:
    """Return path to command if in PATH (including extended), else None."""
    path = _extended_path()
    return shutil.which(cmd, path=path)


def commands_required_for_mcp_catalog() -> list[str]:
    """Commands needed by any server in the no-setup MCP catalog (uvx, npx)."""
    return ["uvx", "npx"]


# Registry: cmd -> () -> (bool, str). Extend this to add new installable commands (Open/Closed).
_INSTALLERS: dict[str, Callable[[], tuple[bool, str]]] = {}


def _register_installer(cmd: str):
    def decorator(fn):
        _INSTALLERS[cmd] = fn
        return fn
    return decorator


@_register_installer("uvx")
def install_uv() -> tuple[bool, str]:
    """
    Install uv (provides uvx) via official installer. No sudo.
    Returns (success, message).
    """
    if find_command("uvx"):
        return True, "uvx already available"
    install_dir = Path.home() / ".local" / "bin"
    install_dir.mkdir(parents=True, exist_ok=True)
    env = os.environ.copy()
    env["UV_INSTALL_DIR"] = str(install_dir)
    env["PATH"] = str(install_dir) + os.pathsep + env.get("PATH", "")
    try:
        # UV_NO_MODIFY_PATH=1 so we don't touch shell rc; we'll tell user to add PATH
        env["UV_NO_MODIFY_PATH"] = "1"
        r = subprocess.run(
            ["sh", "-c", "curl -LsSf https://astral.sh/uv/install.sh | sh"],
            env=env,
            timeout=120,
            capture_output=True,
            text=True,
        )
        if r.returncode != 0:
            return False, (r.stderr or r.stdout or "uv install failed").strip()
        if find_command("uvx"):
            return True, "uv installed (uvx available)"
        return True, "uv installed; add ~/.local/bin to PATH if uvx not found"
    except subprocess.TimeoutExpired:
        return False, "uv install timed out"
    except FileNotFoundError:
        return False, "curl or sh not found; install uv manually: curl -LsSf https://astral.sh/uv/install.sh | sh"
    except Exception as e:
        return False, str(e)


def install_node_npx() -> tuple[bool, str]:
    """
    Try to install Node.js/npx. Tries: (1) apt with sudo, (2) instructions.
    Returns (success, message).
    """
    if find_command("npx"):
        return True, "npx already available"
    # Try apt (may require sudo)
    try:
        r = subprocess.run(
            ["sudo", "apt-get", "install", "-y", "nodejs", "npm"],
            timeout=120,
            capture_output=True,
            text=True,
        )
        if r.returncode == 0 and find_command("npx"):
            return True, "Node.js/npx installed via apt"
        if r.returncode != 0:
            pass  # fall through to instructions
    except FileNotFoundError:
        pass
    except subprocess.TimeoutExpired:
        pass
    # No sudo or failed: print instructions
    msg = (
        "Node.js/npx not found. Install one of:\n"
        "  • sudo apt install nodejs npm\n"
        "  • nvm (no sudo): curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.40.1/install.sh | bash\n"
        "    then: source ~/.nvm/nvm.sh && nvm install --lts\n"
        "  • https://nodejs.org (binary for Linux)"
    )
    return False, msg


_register_installer("npx")(install_node_npx)


def ensure_commands(commands: list[str], install: bool = True) -> tuple[bool, list[str]]:
    """
    Check that each command is available; if install=True, try to install missing.
    Returns (all_ok, list of messages for user).
    """
    messages: list[str] = []
    all_ok = True
    for cmd in commands:
        if find_command(cmd):
            messages.append(f"  ✓ {cmd} found")
            continue
        messages.append(f"  ✗ {cmd} not found")
        if not install:
            all_ok = False
            continue
        installer = _INSTALLERS.get(cmd)
        if installer:
            ok, msg = installer()
            messages.append(f"    → {msg}")
            if not ok:
                all_ok = False
        else:
            messages.append(f"    → Install {cmd} manually and re-run.")
            all_ok = False
    return all_ok, messages
