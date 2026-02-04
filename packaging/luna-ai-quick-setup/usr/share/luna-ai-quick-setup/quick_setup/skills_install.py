"""
Install bundled skills (e.g. self_config) into Luna skills dir for agent-skills-mcp.
"""
from __future__ import annotations

import shutil
from pathlib import Path

from . import paths


def _self_config_source_dir(project_root: Path) -> Path:
    """Path to quick_setup/self_config (source)."""
    return project_root / "self_config"


def install_self_config_skill(project_root: Path | None = None) -> tuple[bool, str]:
    """
    Copy self_config skill from quick_setup/self_config to
    ~/.local/share/cosmic_llm/skills/self_config/ so the skills MCP server can use it.
    Returns (success, message).
    """
    root = project_root or Path(__file__).resolve().parents[1]
    src = _self_config_source_dir(root)
    if not src.is_dir() or not (src / "SKILL.md").exists():
        return False, f"self_config skill not found at {src}"
    dest_dir = paths.skills_dir() / "self_config"
    dest_dir.mkdir(parents=True, exist_ok=True)
    for f in src.iterdir():
        if f.is_file():
            shutil.copy2(f, dest_dir / f.name)
    return True, f"Installed self_config skill to {dest_dir}"
