"""
Server configuration: [server] in config.toml and thin_ui server_config.toml.
"""
from __future__ import annotations

from pathlib import Path
from typing import Any

from . import paths
from .profile_creator import load_config, save_config


SERVER_DEFAULTS = {
    "enabled": True,
    "host": "127.0.0.1",
    "port": 8080,
    "api_key": "",
    "stream_timeout_secs": 300,
    "healthcheck_interval_secs": 30,
    "wal_enabled": True,
    "wal_autocheckpoint": 200,
    "sqlite_busy_timeout_ms": 5000,
}


def _clamp_port(port: int) -> int:
    """Ensure port is in valid range 1-65535."""
    return max(1, min(65535, port))


def _generate_api_key() -> str:
    import secrets
    return secrets.token_urlsafe(24)


def merge_server_into_config(
    host: str = "127.0.0.1",
    port: int = 8080,
    api_key: str = "",
    config_path: Path | None = None,
) -> str:
    """Merge [server] into config.toml. Returns the api_key written (generated if blank)."""
    data = load_config(config_path)
    key = (api_key or "").strip() or _generate_api_key()
    data["server"] = {
        **SERVER_DEFAULTS,
        "host": host,
        "port": _clamp_port(port),
        "api_key": key,
    }
    save_config(data, config_path)
    return key


def thin_ui_connect_host(server_host: str) -> str:
    """Host for thin_ui to connect: localhost when server binds 0.0.0.0."""
    return "localhost" if server_host.strip() == "0.0.0.0" else server_host.strip()


def write_thin_ui_server_config(
    host: str,
    port: int,
    api_key: str,
    config_path: Path | None = None,
) -> Path:
    """
    Write thin_ui server_config.toml so the user can connect right away.
    Uses thin_ui_connect_host(host) so 0.0.0.0 → localhost for same-machine connect.
    Returns the path written.
    """
    path = config_path or paths.thin_ui_server_config_path()
    path.parent.mkdir(parents=True, exist_ok=True)
    connect_host = thin_ui_connect_host(host)
    key = (api_key or "").strip()
    if not key:
        raise ValueError("api_key is required for thin UI server_config.toml")
    data: dict[str, Any] = {
        "host": connect_host,
        "port": port,
        "api_key": key,
    }
    try:
        import toml
        with open(path, "w") as f:
            f.write("# Luna Thin UI – connect to Luna server (quick_setup)\n")
            toml.dump(data, f)
    except ImportError:
        raise RuntimeError("Install 'toml' (pip install toml) for config write support.")
    return path
