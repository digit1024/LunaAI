"""
Create or update a Luna AI LLM profile and merge it into config.toml.
Generates [profiles.<name>] with backend, api_key, model, endpoint, temperature,
max_tokens, context_window_size, summarize_threshold, profile_prompt_file, etc.
"""
from __future__ import annotations

from pathlib import Path
from typing import Any

from . import paths

try:
    import toml
except ImportError:
    toml = None  # type: ignore[assignment]


def _ensure_toml() -> None:
    if toml is None:
        raise RuntimeError("Install 'toml' (pip install toml) for config write support.")


def build_profile(
    *,
    name: str,
    backend: str,
    api_key: str,
    model: str,
    endpoint: str,
    temperature: float = 0.3,
    max_tokens: int = 4000,
    context_window_size: int | None = None,
    summarize_threshold: float = 0.7,
    profile_prompt_file: str | None = None,
    enabled_mcp: list[str] | None = None,
    hidden: bool = False,
) -> dict[str, Any]:
    """Build a profile dict suitable for TOML [profiles.<name>]."""
    p: dict[str, Any] = {
        "backend": backend,
        "api_key": api_key,
        "model": model,
        "endpoint": endpoint,
        "temperature": temperature,
        "max_tokens": max_tokens,
        "summarize_threshold": summarize_threshold,
        "hidden": hidden,
    }
    if context_window_size is not None:
        p["context_window_size"] = context_window_size
    if profile_prompt_file:
        p["profile_prompt_file"] = profile_prompt_file
    if enabled_mcp:
        p["enabled_mcp"] = enabled_mcp
    return p


def load_config(config_path: Path | None = None) -> dict[str, Any]:
    """Load config.toml as dict. Returns empty dict if missing."""
    path = config_path or paths.config_toml_path()
    if not path.exists():
        return {}
    _ensure_toml()
    return toml.load(path)


def save_config(data: dict[str, Any], config_path: Path | None = None) -> None:
    """Write config dict to config.toml."""
    path = config_path or paths.config_toml_path()
    path.parent.mkdir(parents=True, exist_ok=True)
    _ensure_toml()
    with open(path, "w") as f:
        toml.dump(data, f)


def add_or_update_profile(
    profile_name: str,
    profile: dict[str, Any],
    set_default: bool = True,
    config_path: Path | None = None,
) -> None:
    """
    Merge one profile into config.toml and optionally set as default.
    Creates config file and [profiles] section if missing.
    """
    data = load_config(config_path)
    if "profiles" not in data:
        data["profiles"] = {}
    data["profiles"][profile_name] = profile
    if set_default:
        data["default"] = profile_name
    save_config(data, config_path)
