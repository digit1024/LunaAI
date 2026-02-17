#!/usr/bin/env python3
"""Run quick_setup tests and print results to stdout."""
import sys
from pathlib import Path
sys.path.insert(0, ".")

def run():
    print("Starting tests...", flush=True)
    from quick_setup.paths import home, cosmic_llm_dir, config_toml_path
    from quick_setup.profile_creator import (
        build_model_preset,
        build_profile,
        load_config,
        save_config,
        add_or_update_profile,
    )
    from quick_setup.prompts import get_personas, get_persona_path
    from quick_setup.deps import find_command, commands_required_for_mcp_catalog, ensure_commands
    from quick_setup.mcp_config import load_mcp_catalog, build_servers_from_catalog, build_luna_memory_server

    ok = []
    fail = []

    # Paths
    try:
        assert home().is_dir()
        assert "cosmic_llm" in str(cosmic_llm_dir())
        ok.append("paths")
    except Exception as e:
        fail.append(("paths", str(e)))

    # Profile (config refinement: preset + profile)
    try:
        preset = build_model_preset(
            preset_name="t",
            backend="openai",
            model="gpt-4",
            endpoint="https://api.openai.com/v1/chat/completions",
            api_key="k",
        )
        assert preset["backend"] == "openai"
        p = build_profile(model_preset_name="t", prompts=[], tools_policy="default")
        assert p["model_preset"] == "t"
        ok.append("profile_creator.build_profile")
    except Exception as e:
        fail.append(("profile_creator", str(e)))

    # Config load/save (temp dir)
    try:
        import tempfile
        with tempfile.TemporaryDirectory() as d:
            path = Path(d) / "config.toml"
            save_config({"default": "p", "profiles": {}}, config_path=path)
            data = load_config(path)
            assert data["default"] == "p"
        ok.append("profile_creator.load_save_config")
    except Exception as e:
        fail.append(("config load/save", str(e)))

    # Prompts
    try:
        personas = get_personas()
        assert "luna" in personas
        root = Path(__file__).parent / "sample_data"
        if root.exists():
            p = get_persona_path("luna", root)
            assert "luna.md" in str(p)
        ok.append("prompts")
    except Exception as e:
        fail.append(("prompts", str(e)))

    # Deps
    try:
        cmds = commands_required_for_mcp_catalog()
        assert "uvx" in cmds and "npx" in cmds
        found = find_command("python3") or find_command("uvx")
        assert found
        all_ok, msgs = ensure_commands(["uvx", "npx"], install=False)
        assert isinstance(msgs, list)
        ok.append("deps")
    except Exception as e:
        fail.append(("deps", str(e)))

    # MCP catalog
    try:
        import tempfile
        root = Path(__file__).parent
        cat_path = root / "catalog" / "mcp_servers.json"
        if cat_path.exists():
            cat = load_mcp_catalog(cat_path)
            assert "servers" in cat
            with tempfile.TemporaryDirectory() as d:
                servers = build_servers_from_catalog(
                    ["shell", "fetch"],
                    cosmic_llm_dir=Path(d),
                    catalog_path=cat_path,
                )
                assert "shell" in servers
            mem = build_luna_memory_server("/bin/foo", cosmic_llm_db_path="/tmp/db")
            assert mem["command"] == "/bin/foo"
        ok.append("mcp_config")
    except Exception as e:
        fail.append(("mcp_config", str(e)))

    # Main helpers
    try:
        from quick_setup.main import _load_catalog, _suggest_profile_name, _mcp_catalog_path
        cat = _load_catalog()
        assert "providers" in cat and "openai" in cat["providers"]
        n = _suggest_profile_name("luna", "gpt-4.1-mini")
        assert "luna" in n
        assert _mcp_catalog_path().name == "mcp_servers.json"
        ok.append("main_helpers")
    except Exception as e:
        fail.append(("main_helpers", str(e)))

    # build_profile with empty prompts (persona missing case)
    try:
        p = build_profile(model_preset_name="t", prompts=[], tools_policy="default")
        assert p.get("prompts") == []
        ok.append("profile_prompts_optional")
    except Exception as e:
        fail.append(("profile_prompts_optional", str(e)))

    print("PASS:", len(ok), ok, flush=True)
    if fail:
        print("FAIL:", len(fail), flush=True)
        for name, err in fail:
            print("  ", name, ":", err, flush=True)
        return 1
    return 0

if __name__ == "__main__":
    exit_code = run()
    # Write result to file for CI/headless
    result_path = Path(__file__).resolve().parent / "test_result.txt"
    result_path.write_text(f"exit_code={exit_code}\n")
    sys.exit(exit_code)
