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
    "host": "0.0.0.0",
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


def merge_server_into_config(
    host: str = "0.0.0.0",
    port: int = 8080,
    api_key: str = "",
    config_path: Path | None = None,
) -> None:
    """Merge [server] section into config.toml. Creates file if missing. Port clamped to 1-65535."""
    data = load_config(config_path)
    data["server"] = {
        **SERVER_DEFAULTS,
        "host": host,
        "port": _clamp_port(port),
        "api_key": api_key or SERVER_DEFAULTS["api_key"],
    }
    save_config(data, config_path)


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
    data: dict[str, Any] = {
        "host": connect_host,
        "port": port,
        "api_key": api_key,
    }
    try:
        import toml
        with open(path, "w") as f:
            f.write("# Luna Thin UI – connect to Luna server (quick_setup)\n")
            toml.dump(data, f)
    except ImportError:
        raise RuntimeError("Install 'toml' (pip install toml) for config write support.")
    return path
