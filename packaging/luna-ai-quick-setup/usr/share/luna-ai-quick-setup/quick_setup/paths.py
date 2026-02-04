"""
Resolve config and data paths for Luna AI (cosmic_llm) and thin_ui.
Matches Rust: dirs::data_dir()/cosmic_llm, dirs::config_dir()/luna_thin_ui.
"""
import os
from pathlib import Path


def home() -> Path:
    """User home directory (for placeholders like {{HOME}})."""
    return Path.home()


def _data_dir() -> Path:
    xdg = os.environ.get("XDG_DATA_HOME")
    if xdg:
        return Path(xdg)
    return Path.home() / ".local" / "share"


def _config_dir() -> Path:
    xdg = os.environ.get("XDG_CONFIG_HOME")
    if xdg:
        return Path(xdg)
    return Path.home() / ".config"


def cosmic_llm_dir() -> Path:
    return _data_dir() / "cosmic_llm"


def config_toml_path() -> Path:
    return cosmic_llm_dir() / "config.toml"


def mcp_config_path() -> Path:
    return cosmic_llm_dir() / "mcp_config.json"


def system_prompt_path() -> Path:
    return cosmic_llm_dir() / "system_prompt.md"


def profiles_dir() -> Path:
    return cosmic_llm_dir() / "profiles"


def skills_dir() -> Path:
    """Luna skills directory (used by agent-skills-mcp MCP server)."""
    return cosmic_llm_dir() / "skills"


def profile_prompt_path(profile_name: str) -> Path:
    return profiles_dir() / f"{profile_name}.md"


def luna_thin_ui_config_dir() -> Path:
    return _config_dir() / "luna_thin_ui"


def thin_ui_server_config_path() -> Path:
    return luna_thin_ui_config_dir() / "server_config.toml"


def user_systemd_dir() -> Path:
    """User systemd unit directory (~/.config/systemd/user)."""
    return _config_dir() / "systemd" / "user"


def luna_server_service_path() -> Path:
    """Path to luna-server.service in user systemd."""
    return user_systemd_dir() / "luna-server.service"
