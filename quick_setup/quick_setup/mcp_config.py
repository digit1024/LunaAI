"""
MCP configuration: build mcp_config.json from catalog (no-setup servers only).
Luna memory server is added separately when user opts in (requires binary path).
"""
from __future__ import annotations

import json
from pathlib import Path
from typing import Any

from . import paths


def _expand_value(value: str, cosmic_llm_dir: Path) -> str:
    if "{{HOME}}" in value:
        value = value.replace("{{HOME}}", str(paths.home()))
    if "{{COSMIC_LLM_DIR}}" in value:
        value = value.replace("{{COSMIC_LLM_DIR}}", str(cosmic_llm_dir))
    return value


def load_mcp_catalog(catalog_path: Path | None = None) -> dict[str, Any]:
    """Load catalog/mcp_servers.json. Returns { servers: [ { id, label, command, args, env, note }, ... ] }."""
    if catalog_path is None:
        catalog_path = Path(__file__).resolve().parents[1] / "catalog" / "mcp_servers.json"
    if not catalog_path.exists():
        return {"servers": []}
    with open(catalog_path) as f:
        data = json.load(f)
    return data


def build_servers_from_catalog(
    selected_ids: list[str],
    cosmic_llm_dir: Path | None = None,
    catalog_path: Path | None = None,
) -> dict[str, Any]:
    """
    Build mcpServers dict from catalog for given server ids.
    Expands {{HOME}} and {{COSMIC_LLM_DIR}} in args and env values.
    """
    data_dir = cosmic_llm_dir or paths.cosmic_llm_dir()
    catalog = load_mcp_catalog(catalog_path)
    entries = {s["id"]: s for s in catalog.get("servers", []) if "id" in s}
    servers: dict[str, Any] = {}
    for sid in selected_ids:
        if sid not in entries:
            continue
        e = entries[sid]
        args = [ _expand_value(a, data_dir) if isinstance(a, str) else a for a in e.get("args", []) ]
        env = { k: _expand_value(v, data_dir) for k, v in e.get("env", {}).items() }
        servers[sid] = {
            "command": e.get("command", ""),
            "args": args,
            "env": env,
        }
    return servers


def build_luna_memory_server(
    binary_path: str | Path,
    cosmic_llm_db_path: str | Path | None = None,
) -> dict[str, Any]:
    """Build cosmic-llm-memory server entry (requires setup: binary path). Not in no-setup catalog."""
    data_dir = paths.cosmic_llm_dir()
    db_path = cosmic_llm_db_path or str(data_dir / "conversations.db")
    return {
        "command": str(binary_path),
        "args": [],
        "env": {"COSMIC_LLM_DB_PATH": str(db_path)},
    }


def load_mcp_config(config_path: Path | None = None) -> dict[str, Any]:
    """Load mcp_config.json. Returns { mcpServers: {} } if missing."""
    from . import paths
    path = config_path or paths.mcp_config_path()
    if not path.exists():
        return {"mcpServers": {}}
    with open(path) as f:
        return json.load(f)


def save_mcp_config(
    config: dict[str, Any],
    config_path: Path | None = None,
) -> None:
    """Write mcp_config.json."""
    from . import paths
    path = config_path or paths.mcp_config_path()
    path.parent.mkdir(parents=True, exist_ok=True)
    with open(path, "w") as f:
        json.dump(config, f, indent=2)


def merge_mcp_servers(
    selected_catalog_ids: list[str],
    add_luna_memory: bool = False,
    luna_memory_binary: str | Path | None = None,
    cosmic_llm_dir: Path | None = None,
    cosmic_llm_db_path: str | Path | None = None,
    catalog_path: Path | None = None,
    config_path: Path | None = None,
    merge: bool = True,
) -> dict[str, Any]:
    """
    Build mcp_config with servers from catalog (selected_ids) and optionally
    cosmic-llm-memory. If merge=True, load existing mcp_config and add only
    missing servers.
    """
    data_dir = cosmic_llm_dir or paths.cosmic_llm_dir()
    servers = build_servers_from_catalog(
        selected_catalog_ids,
        cosmic_llm_dir=data_dir,
        catalog_path=catalog_path,
    )
    if add_luna_memory and luna_memory_binary:
        servers["cosmic-llm-memory"] = build_luna_memory_server(
            luna_memory_binary,
            cosmic_llm_db_path=cosmic_llm_db_path or str(data_dir / "conversations.db"),
        )
    if merge:
        current = load_mcp_config(config_path)
        existing = current.get("mcpServers", {})
        for name, cfg in servers.items():
            if name not in existing:
                existing[name] = cfg
        current["mcpServers"] = existing
        return current
    return {"mcpServers": servers}


# Legacy: keep for any callers that expect the old “defaults” API
def default_mcp_servers(
    *,
    mcp_luna_memory_binary: str | Path | None = None,
    cosmic_llm_db_path: str | Path | None = None,
    skills_folder: str | Path | None = None,
    allow_commands: str | None = None,
) -> dict[str, Any]:
    """Build default mcpServers dict (all catalog servers + Luna memory if path given)."""
    catalog = load_mcp_catalog()
    all_ids = [s["id"] for s in catalog.get("servers", []) if "id" in s]
    servers = build_servers_from_catalog(
        all_ids,
        cosmic_llm_dir=None,
        catalog_path=None,
    )
    if mcp_luna_memory_binary:
        servers["cosmic-llm-memory"] = build_luna_memory_server(
            mcp_luna_memory_binary,
            cosmic_llm_db_path=cosmic_llm_db_path,
        )
    return servers


def merge_default_mcp_servers(
    *,
    merge: bool = True,
    mcp_luna_memory_binary: str | Path | None = None,
    cosmic_llm_db_path: str | Path | None = None,
    skills_folder: str | Path | None = None,
    config_path: Path | None = None,
) -> dict[str, Any]:
    """Like merge_mcp_servers with all catalog servers selected and optional Luna memory."""
    catalog = load_mcp_catalog()
    all_ids = [s["id"] for s in catalog.get("servers", []) if "id" in s]
    return merge_mcp_servers(
        selected_catalog_ids=all_ids,
        add_luna_memory=bool(mcp_luna_memory_binary),
        luna_memory_binary=mcp_luna_memory_binary,
        cosmic_llm_db_path=cosmic_llm_db_path,
        config_path=config_path,
        merge=merge,
    )
