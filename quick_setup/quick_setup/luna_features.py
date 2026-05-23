"""
Full Luna feature blocks: tools_policy enabled_mcp, embedding, deep_sleep, title_summary.
Authoritative enabled_mcp is set here in finalize_full_luna_config (after MCP selection).
"""
from __future__ import annotations

from pathlib import Path
from typing import Any

from .profile_creator import load_config, save_config

# Align with src/config/mod.rs defaults
EMBEDDING_ENDPOINT = "https://api.openai.com/v1/embeddings"
EMBEDDING_MODEL = "text-embedding-3-small"
EMBEDDING_DIMENSIONS = 1536
LUNA_MEMORY_SERVER_ID = "cosmic-llm-memory"


def _normalize_backend(backend: str) -> str:
    if backend in ("deepseek", "openrouter"):
        return "openai"
    return backend


def enabled_mcp_for_selection(
    selected_ids: list[str],
    add_luna_memory: bool,
) -> list[str]:
    """Deduped MCP server ids for tools_policies.default.enabled_mcp."""
    out: list[str] = []
    seen: set[str] = set()
    for sid in selected_ids:
        if sid and sid not in seen:
            seen.add(sid)
            out.append(sid)
    if add_luna_memory and LUNA_MEMORY_SERVER_ID not in seen:
        out.append(LUNA_MEMORY_SERVER_ID)
    return out


def merge_tools_policy_enabled_mcp(
    data: dict[str, Any],
    enabled_mcp: list[str],
) -> None:
    """Ensure tools_policies.default exists and set enabled_mcp."""
    if "tools_policies" not in data:
        data["tools_policies"] = {}
    if "default" not in data["tools_policies"]:
        data["tools_policies"]["default"] = {
            "enabled_mcp": [],
            "enabled_tools": ["*"],
            "disabled_tools": [],
        }
    policy = data["tools_policies"]["default"]
    policy["enabled_mcp"] = enabled_mcp
    policy.setdefault("enabled_tools", ["*"])
    policy.setdefault("disabled_tools", [])


def build_embedding_config(
    *,
    api_key: str,
    backend: str,
) -> dict[str, Any]:
    """OpenAI-compatible embedding section; enabled=true."""
    return {
        "enabled": True,
        "endpoint": EMBEDDING_ENDPOINT,
        "model": EMBEDDING_MODEL,
        "dimensions": EMBEDDING_DIMENSIONS,
        "api_key": api_key,
    }


def embedding_api_key_from_chat(
    chat_api_key: str,
    chat_backend: str,
    embedding_api_key: str | None = None,
) -> str:
    """
    Resolve embedding API key: explicit embedding key, or chat key when backend is openai-compatible.
    """
    if embedding_api_key and embedding_api_key.strip():
        return embedding_api_key.strip()
    if _normalize_backend(chat_backend) == "openai":
        return chat_api_key.strip()
    return ""


def chat_backend_needs_embedding_key_prompt(chat_backend: str) -> bool:
    """Anthropic/Gemini chat backends need a separate OpenAI embeddings key."""
    return _normalize_backend(chat_backend) != "openai"


def build_deep_sleep_config(profile_name: str) -> dict[str, Any]:
    return {
        "enabled": True,
        "profile": profile_name,
    }


def build_title_summary_config(profile_name: str) -> dict[str, Any]:
    return {
        "title_generation_profile": profile_name,
    }


def finalize_full_luna_config(
    *,
    profile_name: str,
    enabled_mcp: list[str],
    chat_api_key: str,
    chat_backend: str,
    full_luna: bool = True,
    embedding_api_key: str | None = None,
    config_path: Path | None = None,
) -> dict[str, Any]:
    """
    Patch config.toml after MCP selection: tools policy, optional full Luna blocks.
    Returns a summary dict for the CLI (embedding_on, deep_sleep_on, enabled_mcp).
    """
    data = load_config(config_path)
    merge_tools_policy_enabled_mcp(data, enabled_mcp)

    summary: dict[str, Any] = {
        "enabled_mcp": enabled_mcp,
        "embedding_on": False,
        "deep_sleep_on": False,
        "title_summary_on": False,
    }

    if full_luna:
        emb_key = embedding_api_key_from_chat(
            chat_api_key, chat_backend, embedding_api_key
        )
        if emb_key:
            data["embedding"] = build_embedding_config(
                api_key=emb_key,
                backend=chat_backend,
            )
            summary["embedding_on"] = True
        data["deep_sleep"] = build_deep_sleep_config(profile_name)
        summary["deep_sleep_on"] = True
        data["title_summary"] = build_title_summary_config(profile_name)
        summary["title_summary_on"] = True

    save_config(data, config_path)
    return summary
