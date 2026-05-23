"""
Tests for quick_setup modules. Use temp dirs to avoid touching real config.
"""
from __future__ import annotations

import json
import os
from pathlib import Path

import pytest


def _quick_setup_root() -> Path:
    """Project root (folder containing catalog/, sample_data/, quick_setup package)."""
    return Path(__file__).resolve().parents[1]


@pytest.fixture
def tmp_config_dir(tmp_path):
    """Temporary cosmic_llm-style dir for config.toml, mcp_config.json."""
    d = tmp_path / "cosmic_llm"
    d.mkdir()
    return d


@pytest.fixture
def monkeypatch_paths(tmp_path, tmp_config_dir, monkeypatch):
    """Patch paths to use tmp_path for project root and tmp_config_dir for cosmic_llm."""
    root = _quick_setup_root()
    # Only override cosmic_llm dir so config goes to tmp
    import quick_setup.paths as paths_mod
    original_cosmic = paths_mod.cosmic_llm_dir
    def fake_cosmic():
        return tmp_config_dir
    monkeypatch.setattr(paths_mod, "cosmic_llm_dir", fake_cosmic)
    monkeypatch.setattr(paths_mod, "config_toml_path", lambda: tmp_config_dir / "config.toml")
    monkeypatch.setattr(paths_mod, "mcp_config_path", lambda: tmp_config_dir / "mcp_config.json")
    monkeypatch.setattr(paths_mod, "system_prompt_path", lambda: tmp_config_dir / "system_prompt.md")
    monkeypatch.setattr(paths_mod, "profiles_dir", lambda: tmp_config_dir / "profiles")
    return tmp_config_dir


class TestPaths:
    def test_home(self):
        from quick_setup.paths import home
        assert home().is_dir()
        assert str(home()) == os.path.expanduser("~")

    def test_cosmic_llm_dir(self):
        from quick_setup.paths import cosmic_llm_dir
        d = cosmic_llm_dir()
        assert "cosmic_llm" in str(d)

    def test_config_toml_path(self):
        from quick_setup.paths import config_toml_path
        p = config_toml_path()
        assert p.name == "config.toml"
        assert p.parent.name == "cosmic_llm"


class TestProfileCreator:
    def test_build_model_preset(self):
        from quick_setup.profile_creator import build_model_preset
        p = build_model_preset(
            preset_name="test_preset",
            backend="openai",
            model="gpt-4",
            endpoint="https://api.openai.com/v1/chat/completions",
            api_key="sk-x",
            temperature=0.3,
            max_tokens=4000,
        )
        assert p["backend"] == "openai"
        assert p["model"] == "gpt-4"
        assert p["temperature"] == 0.3
        assert "context_window_size" not in p

    def test_build_profile(self):
        from quick_setup.profile_creator import build_profile
        p = build_profile(
            model_preset_name="my_preset",
            prompts=["profiles/me.md"],
            tools_policy="default",
        )
        assert p["model_preset"] == "my_preset"
        assert p["prompts"] == ["profiles/me.md"]
        assert p["tools_policy"] == "default"
        assert "backend" not in p

    def test_build_profile_with_context_window(self):
        from quick_setup.profile_creator import build_profile
        p = build_profile(
            model_preset_name="p",
            context_window_size=128000,
        )
        assert p["context_window_size"] == 128000

    def test_load_save_config(self, tmp_config_dir):
        from quick_setup.profile_creator import load_config, save_config
        path = tmp_config_dir / "config.toml"
        save_config({"default": "p1", "profiles": {}}, config_path=path)
        data = load_config(path)
        assert data["default"] == "p1"
        assert "profiles" in data

    def test_add_or_update_profile(self, tmp_config_dir):
        from quick_setup.profile_creator import (
            load_config,
            add_or_update_profile,
            build_profile,
            build_model_preset,
        )
        path = tmp_config_dir / "config.toml"
        preset = build_model_preset(
            preset_name="p1",
            backend="openai",
            model="gpt-4",
            endpoint="https://api.openai.com/v1/chat/completions",
            api_key="k",
        )
        profile = build_profile(model_preset_name="p1", prompts=[], tools_policy="default")
        add_or_update_profile(
            "p1",
            profile,
            set_default=True,
            config_path=path,
            preset_name="p1",
            preset=preset,
        )
        data = load_config(path)
        assert data["default"] == "p1"
        assert "p1" in data["profiles"]
        assert data["profiles"]["p1"]["model_preset"] == "p1"
        assert "p1" in data["model_presets"]
        assert data["model_presets"]["p1"]["backend"] == "openai"
        assert "default" in data["tools_policies"]


class TestMcpConfig:
    def test_load_mcp_catalog(self):
        from quick_setup.mcp_config import load_mcp_catalog
        root = _quick_setup_root()
        path = root / "catalog" / "mcp_servers.json"
        if not path.exists():
            pytest.skip("catalog/mcp_servers.json not found")
        cat = load_mcp_catalog(path)
        assert "servers" in cat
        ids = [s["id"] for s in cat["servers"] if "id" in s]
        assert "shell" in ids
        assert "fetch" in ids

    def test_build_servers_from_catalog(self, tmp_config_dir):
        from quick_setup.mcp_config import build_servers_from_catalog, load_mcp_catalog
        root = _quick_setup_root()
        path = root / "catalog" / "mcp_servers.json"
        if not path.exists():
            pytest.skip("catalog/mcp_servers.json not found")
        servers = build_servers_from_catalog(
            ["shell", "filesystem"],
            cosmic_llm_dir=tmp_config_dir,
            catalog_path=path,
        )
        assert "shell" in servers
        assert "filesystem" in servers
        assert servers["shell"]["command"] == "uvx"
        # {{HOME}} and {{COSMIC_LLM_DIR}} should be expanded (no placeholders left)
        assert "{{" not in str(servers["filesystem"]["args"])

    def test_build_luna_memory_server(self, tmp_config_dir):
        from quick_setup.mcp_config import build_luna_memory_server
        s = build_luna_memory_server("/usr/bin/mcp_luna_history", cosmic_llm_db_path=str(tmp_config_dir / "db.db"))
        assert s["command"] == "/usr/bin/mcp_luna_history"
        assert s["env"]["COSMIC_LLM_DB_PATH"] == str(tmp_config_dir / "db.db")


class TestDeps:
    def test_commands_required(self):
        from quick_setup.deps import commands_required_for_mcp_catalog
        assert "uvx" in commands_required_for_mcp_catalog()
        assert "npx" in commands_required_for_mcp_catalog()

    def test_find_command_existing(self):
        from quick_setup.deps import find_command
        # At least one of uvx or npx or python3 should be there
        found = find_command("python3") or find_command("uvx") or find_command("npx")
        assert found is not None

    def test_ensure_commands_no_install(self):
        from quick_setup.deps import ensure_commands
        all_ok, msgs = ensure_commands(["uvx", "npx"], install=False)
        assert isinstance(msgs, list)
        assert len(msgs) == 2
        # If both found, all_ok is True
        assert isinstance(all_ok, bool)


class TestPrompts:
    def test_get_personas(self):
        from quick_setup.prompts import get_personas
        p = get_personas()
        assert "luna" in p
        assert "vera" in p
        assert "jude" in p

    def test_get_persona_path(self):
        from quick_setup.prompts import get_persona_path
        root = _quick_setup_root() / "sample_data"
        path = get_persona_path("luna", root)
        assert path.name == "luna.md"
        assert "personas" in str(path)

    def test_install_system_prompt(self, tmp_path):
        from quick_setup.prompts import install_system_prompt
        target = tmp_path / "system_prompt.md"
        install_system_prompt("You are helpful.", system_prompt_path=target)
        assert target.read_text() == "You are helpful."


class TestMainHelpers:
    """Test main module helpers that don't need interactive input."""

    def test_load_catalog(self):
        from quick_setup.main import _load_catalog
        cat = _load_catalog()
        assert "providers" in cat
        assert "openai" in cat["providers"]

    def test_sample_data_root(self):
        from quick_setup.main import _sample_data_root
        root = _sample_data_root()
        assert root.is_dir()
        assert (root / "personas" / "luna.md").exists()

    def test_suggest_profile_name(self):
        from quick_setup.main import _suggest_profile_name
        name = _suggest_profile_name("luna", "gpt-4.1-mini")
        assert "luna" in name
        assert "gpt" in name or "4" in name

    def test_mcp_catalog_path(self):
        from quick_setup.main import _mcp_catalog_path
        p = _mcp_catalog_path()
        assert p.name == "mcp_servers.json"
        assert p.exists()


class TestLunaFeatures:
    def test_enabled_mcp_for_selection_with_memory(self):
        from quick_setup.luna_features import enabled_mcp_for_selection

        ids = enabled_mcp_for_selection(["shell", "skills"], True)
        assert ids == ["shell", "skills", "cosmic-llm-memory"]

    def test_enabled_mcp_for_selection_dedupes(self):
        from quick_setup.luna_features import enabled_mcp_for_selection

        ids = enabled_mcp_for_selection(["shell", "shell", "fetch"], False)
        assert ids == ["shell", "fetch"]

    def test_finalize_full_luna_config(self, tmp_config_dir):
        from quick_setup.profile_creator import load_config, save_config
        from quick_setup.luna_features import finalize_full_luna_config

        path = tmp_config_dir / "config.toml"
        save_config({"default": "p1", "profiles": {"p1": {}}, "tools_policies": {"default": {}}}, path)
        summary = finalize_full_luna_config(
            profile_name="p1",
            enabled_mcp=["shell", "cosmic-llm-memory"],
            chat_api_key="sk-chat",
            chat_backend="openai",
            full_luna=True,
            config_path=path,
        )
        data = load_config(path)
        assert data["tools_policies"]["default"]["enabled_mcp"] == [
            "shell",
            "cosmic-llm-memory",
        ]
        assert data["embedding"]["enabled"] is True
        assert data["embedding"]["api_key"] == "sk-chat"
        assert data["deep_sleep"]["enabled"] is True
        assert data["deep_sleep"]["profile"] == "p1"
        assert data["title_summary"]["title_generation_profile"] == "p1"
        assert summary["embedding_on"] is True
        assert summary["deep_sleep_on"] is True

    def test_finalize_skips_full_luna_blocks(self, tmp_config_dir):
        from quick_setup.profile_creator import load_config, save_config
        from quick_setup.luna_features import finalize_full_luna_config

        path = tmp_config_dir / "config.toml"
        save_config({"tools_policies": {"default": {"enabled_mcp": []}}}, path)
        summary = finalize_full_luna_config(
            profile_name="p1",
            enabled_mcp=["fetch"],
            chat_api_key="sk-x",
            chat_backend="openai",
            full_luna=False,
            config_path=path,
        )
        data = load_config(path)
        assert data["tools_policies"]["default"]["enabled_mcp"] == ["fetch"]
        assert "embedding" not in data
        assert "deep_sleep" not in data
        assert summary["embedding_on"] is False

    def test_embedding_api_key_from_openai_backend(self):
        from quick_setup.luna_features import embedding_api_key_from_chat

        assert embedding_api_key_from_chat("sk-a", "openai") == "sk-a"
        assert embedding_api_key_from_chat("sk-a", "deepseek") == "sk-a"
        assert embedding_api_key_from_chat("sk-a", "anthropic", "sk-emb") == "sk-emb"

    def test_chat_backend_needs_embedding_key_prompt(self):
        from quick_setup.luna_features import chat_backend_needs_embedding_key_prompt

        assert chat_backend_needs_embedding_key_prompt("openai") is False
        assert chat_backend_needs_embedding_key_prompt("anthropic") is True
        assert chat_backend_needs_embedding_key_prompt("gemini") is True


class TestServerConfig:
    def test_merge_server_into_config(self, tmp_config_dir):
        from quick_setup.profile_creator import load_config
        from quick_setup.server_config import merge_server_into_config
        path = tmp_config_dir / "config.toml"
        merge_server_into_config(host="127.0.0.1", port=9090, api_key="secret", config_path=path)
        data = load_config(path)
        assert data["server"]["host"] == "127.0.0.1"
        assert data["server"]["port"] == 9090
        assert data["server"]["api_key"] == "secret"

    def test_write_thin_ui_server_config(self, tmp_path):
        from quick_setup.server_config import write_thin_ui_server_config
        path = tmp_path / "server_config.toml"
        write_thin_ui_server_config("localhost", 8080, "key", config_path=path)
        import toml
        data = toml.load(path)
        assert data["host"] == "localhost"
        assert data["port"] == 8080
