"""
Create or update a Luna AI LLM profile and merge into config.toml.
Uses the post–config-refinement shape: [model_presets.<name>], [tools_policies.*],
[profiles.<name>] with model_preset, prompts, tools_policy (no inline backend/model/endpoint).
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


# Server only has "openai", "anthropic", "gemini", "ollama". DeepSeek is OpenAI-compatible.
def _normalize_backend(backend: str) -> str:
    if backend == "deepseek" or backend == "openrouter":
        return "openai"
    return backend


def build_model_preset(
    *,
    preset_name: str,
    backend: str,
    model: str,
    endpoint: str,
    api_key: str,
    temperature: float | None = 0.3,
    max_tokens: int | None = 4000,
    context_window_size: int | None = None,
) -> dict[str, Any]:
    """Build a model preset dict for TOML [model_presets.<preset_name>]. Endpoint = full URL."""
    backend = _normalize_backend(backend)
    p: dict[str, Any] = {
        "backend": backend,
        "model": model,
        "endpoint": endpoint.strip() or "https://api.openai.com/v1/chat/completions",
        "api_key": api_key,
    }
    if temperature is not None:
        p["temperature"] = temperature
    if max_tokens is not None:
        p["max_tokens"] = max_tokens
    if context_window_size is not None:
        p["context_window_size"] = context_window_size
    return p


def build_profile(
    *,
    model_preset_name: str,
    prompts: list[str] | None = None,
    tools_policy: str = "default",
    hidden: bool = False,
    summarize_threshold: float = 0.7,
    context_window_size: int | None = None,
) -> dict[str, Any]:
    """Build a profile dict for TOML [profiles.<name>] (references preset + policy + prompts)."""
    p: dict[str, Any] = {
        "model_preset": model_preset_name,
        "prompts": prompts if prompts is not None else [],
        "tools_policy": tools_policy,
        "hidden": hidden,
        "summarize_threshold": summarize_threshold,
    }
    if context_window_size is not None:
        p["context_window_size"] = context_window_size
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


def _ensure_tools_policy_default(data: dict[str, Any]) -> None:
    """Ensure [tools_policies.default] exists so profile can reference it."""
    if "tools_policies" not in data:
        data["tools_policies"] = {}
    if "default" not in data["tools_policies"]:
        data["tools_policies"]["default"] = {
            "enabled_mcp": [],
            "enabled_tools": ["*"],
            "disabled_tools": [],
        }


def add_or_update_profile(
    profile_name: str,
    profile: dict[str, Any],
    set_default: bool = True,
    config_path: Path | None = None,
    preset_name: str | None = None,
    preset: dict[str, Any] | None = None,
) -> None:
    """
    Merge one profile into config.toml and optionally set as default.
    If preset_name and preset are provided, also merge [model_presets.<preset_name>].
    Creates config file, [profiles], [model_presets], and [tools_policies.default] if missing.
    """
    data = load_config(config_path)
    if "profiles" not in data:
        data["profiles"] = {}
    if preset_name is not None and preset is not None:
        if "model_presets" not in data:
            data["model_presets"] = {}
        data["model_presets"][preset_name] = preset
    _ensure_tools_policy_default(data)
    data["profiles"][profile_name] = profile
    if set_default:
        data["default"] = profile_name
    save_config(data, config_path)
