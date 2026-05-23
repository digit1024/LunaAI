"""
Luna AI Quick Setup – interactive, opinionated configuration for server + thin_ui.
"""
from __future__ import annotations

import json
import getpass
import os
import sys
from pathlib import Path
from typing import Any
from urllib.request import Request, urlopen

from . import paths
from .profile_creator import (
    add_or_update_profile,
    build_model_preset,
    build_profile,
)
from .server_config import merge_server_into_config, write_thin_ui_server_config, thin_ui_connect_host
from .mcp_config import load_mcp_catalog, merge_mcp_servers, save_mcp_config
from .deps import commands_required_for_mcp_catalog, ensure_commands
from .systemd_setup import install_user_service
from .skills_install import install_self_config_skill
from .prompts import (
    get_personas,
    install_system_prompt,
    install_persona_as_profile_prompt,
    get_persona_path,
)
from .luna_features import (
    enabled_mcp_for_selection,
    finalize_full_luna_config,
    chat_backend_needs_embedding_key_prompt,
)


def _project_root() -> Path:
    return Path(__file__).resolve().parents[1]


def _load_catalog() -> dict[str, Any]:
    # Project root = directory containing the quick_setup package (e.g. quick_setup/)
    root = _project_root()
    catalog_path = root / "catalog" / "providers_models.json"
    if not catalog_path.exists():
        raise FileNotFoundError(f"Catalog not found: {catalog_path}")
    with open(catalog_path) as f:
        return json.load(f)


def _sample_data_root() -> Path:
    root = _project_root()
    sample_dir = root / "sample_data"
    if sample_dir.is_dir():
        return sample_dir
    return root


def _ask_provider(catalog: dict[str, Any]) -> tuple[str, dict[str, Any]]:
    providers = catalog.get("providers", {})
    choices = list(providers.keys())
    print("\n  Provider:")
    for i, pid in enumerate(choices, 1):
        p = providers[pid]
        print(f"    {i}. {p.get('label', pid)}")
    while True:
        raw = input(f"  Choose (1–{len(choices)}) [1]: ").strip() or "1"
        try:
            idx = int(raw)
            if 1 <= idx <= len(choices):
                return choices[idx - 1], providers[choices[idx - 1]]
        except ValueError:
            pass
        print("  Invalid choice.")


def _ask_model(provider_id: str, provider: dict[str, Any]) -> tuple[str, int | None]:
    models = provider.get("models", [])
    default_id = provider.get("default_model_id", models[0]["id"] if models else "")
    print("\n  Model (suggested default: fast & cheap, not slowest):")
    for i, m in enumerate(models, 1):
        note = " [default]" if m.get("id") == default_id else ""
        print(f"    {i}. {m.get('label', m['id'])} – {m.get('note', '')}{note}")
    while True:
        raw = input(f"  Choose (1–{len(models)}) [{default_id}]: ").strip()
        if not raw:
            return default_id, next((m.get("context_window") for m in models if m.get("id") == default_id), None)
        try:
            idx = int(raw)
            if 1 <= idx <= len(models):
                m = models[idx - 1]
                return m["id"], m.get("context_window")
        except (ValueError, KeyError):
            pass
        print("  Invalid choice.")


def _ask_api_key(provider_label: str) -> str:
    print(f"\n  API key for {provider_label}:")
    return getpass.getpass("  Key (hidden): ").strip()


def _ask_personality() -> str:
    personas = get_personas()
    ids = list(personas.keys())
    print("\n  Personality (system prompt):")
    for i, pid in enumerate(ids, 1):
        print(f"    {i}. {personas[pid]}")
    while True:
        raw = input(f"  Choose (1–{len(ids)}) [1]: ").strip() or "1"
        try:
            idx = int(raw)
            if 1 <= idx <= len(ids):
                return ids[idx - 1]
        except ValueError:
            pass
        print("  Invalid choice.")


def _ask_temperature() -> float:
    raw = input("\n  Temperature [0.3]: ").strip() or "0.3"
    try:
        v = float(raw)
        if 0 <= v <= 2:
            return v
    except ValueError:
        pass
    return 0.3


def _ask_max_tokens() -> int:
    raw = input("  Max tokens [4000]: ").strip() or "4000"
    try:
        return max(1, int(raw))
    except ValueError:
        return 4000


def _suggest_profile_name(persona_id: str, model_id: str) -> str:
    p = persona_id if persona_id in get_personas() else persona_id
    m = model_id.replace("/", "_").split("-")[0][:12]
    return f"{p}_{m}"


def _ask_profile_name(suggested: str) -> str:
    raw = input(f"\n  Profile name [{suggested}]: ").strip() or suggested
    return raw.strip() or suggested


def _clamp_port(port: int) -> int:
    """Ensure port is in valid range 1-65535."""
    return max(1, min(65535, port))


def _ask_server() -> tuple[str, int, str]:
    print("\n  Server API (Luna backend):")
    host = input("  Host [0.0.0.0]: ").strip() or "0.0.0.0"
    port_raw = input("  Port [8080]: ").strip() or "8080"
    try:
        port = _clamp_port(int(port_raw))
        if port != int(port_raw):
            print("  Port adjusted to valid range 1-65535.")
    except ValueError:
        port = 8080
    api_key = input("  Server API key (optional): ").strip()
    return host, port, api_key


# Luna MCP Memory server: release binary (no build required)
MCP_LUNA_MEMORY_RELEASE_URL = "https://github.com/digit1024/mcp_luna_memory/releases/download/1.0/mcp_luna_history"


def _download_mcp_luna_memory_binary(dest: Path) -> bool:
    """Download mcp_luna_history from release URL to dest. Create parent dir, chmod +x. Return True on success."""
    dest = Path(dest).resolve()
    dest.parent.mkdir(parents=True, exist_ok=True)
    try:
        req = Request(MCP_LUNA_MEMORY_RELEASE_URL, headers={"User-Agent": "LunaQuickSetup/1.0"})
        with urlopen(req, timeout=60) as resp:
            data = resp.read()
        dest.write_bytes(data)
        os.chmod(dest, 0o755)
        return True
    except OSError as e:
        print(f"  Download failed: {e}")
        print(f"  Get binary manually: {MCP_LUNA_MEMORY_RELEASE_URL}")
        return False


def _ask_mcp_memory_path() -> str:
    default_dir = paths.cosmic_llm_dir() / "bin"
    default_path = default_dir / "mcp_luna_history"
    default = str(default_path)
    raw = input(f"  Path to mcp_luna_history binary [{default}]: ").strip() or default
    if raw == default and not default_path.exists():
        print("  Downloading mcp_luna_history from release...")
        if _download_mcp_luna_memory_binary(default_path):
            print("  Done.")
        # else: already printed error + URL
    return raw


def _mcp_catalog_path() -> Path:
    return _project_root() / "catalog" / "mcp_servers.json"


def _ask_mcp_servers_from_catalog() -> tuple[list[str], bool, str | None]:
    """Returns (selected catalog ids, add_luna_memory, luna_binary_path or None)."""
    catalog_path = _mcp_catalog_path()
    if not catalog_path.exists():
        print("  MCP catalog not found at", catalog_path, "– skipping MCP server selection.")
        return [], False, None
    catalog = load_mcp_catalog(catalog_path)
    servers = catalog.get("servers", [])
    if not servers:
        print("  MCP catalog is empty – skipping MCP server selection.")
        return [], False, None
    print("\n  MCP servers (no-setup only; pick by number, comma-separated or 'all'):")
    for i, s in enumerate(servers, 1):
        print(f"    {i}. {s.get('label', s.get('id', ''))} – {s.get('note', '')}")
    raw = input("  Which to add? [all]: ").strip().lower() or "all"
    if raw == "all":
        selected_ids = list(dict.fromkeys([s["id"] for s in servers if "id" in s]))
    else:
        selected_ids = []
        for part in raw.split(","):
            part = part.strip()
            try:
                idx = int(part)
                if 1 <= idx <= len(servers) and "id" in servers[idx - 1]:
                    selected_ids.append(servers[idx - 1]["id"])
            except ValueError:
                pass
        selected_ids = list(dict.fromkeys(selected_ids))  # dedupe
    add_luna = False
    luna_bin = None
    if input(
        "  Add Luna memory server? (binary downloaded automatically if needed) [Y/n]: "
    ).strip().lower() not in ("n", "no"):
        add_luna = True
        luna_bin = _ask_mcp_memory_path()
    return selected_ids, add_luna, luna_bin


def _ask_full_luna() -> bool:
    raw = input(
        "\n  Enable memory RAG, deep sleep, and auto conversation titles? [Y/n]: "
    ).strip().lower()
    return raw not in ("n", "no")


def _ask_embedding_api_key() -> str:
    print("\n  Memory embeddings use OpenAI-compatible API (text-embedding-3-small).")
    return getpass.getpass("  OpenAI API key for embeddings (hidden): ").strip()


def _print_setup_summary(
    *,
    profile_name: str,
    feature_summary: dict[str, Any],
    systemd_installed: bool,
) -> None:
    print("\n  —— Setup summary ——")
    print(f"  Default profile:     {profile_name}")
    mcp_list = feature_summary.get("enabled_mcp") or []
    if mcp_list:
        print(f"  MCP enabled in policy: {', '.join(mcp_list)}")
    else:
        print("  MCP enabled in policy: (none)")
    print(
        "  Memory RAG (embedding):",
        "on" if feature_summary.get("embedding_on") else "off",
    )
    print(
        "  Deep sleep:          ",
        "on" if feature_summary.get("deep_sleep_on") else "off",
    )
    print(
        "  Auto titles:         ",
        "on" if feature_summary.get("title_summary_on") else "off",
    )
    print(f"\n  Config:    {paths.config_toml_path()}")
    print(f"  Thin UI:   {paths.thin_ui_server_config_path()}")
    print(f"  MCP JSON:  {paths.mcp_config_path()}")
    print(f"  Skills:    {paths.skills_dir()}")
    if systemd_installed:
        print(f"  Systemd:   {paths.luna_server_service_path()}")
        print("  Run at boot (no login): loginctl enable-linger $USER")
        print("  Check service: systemctl --user status luna-server.service")
    if feature_summary.get("deep_sleep_on"):
        print("  Test deep sleep:  cosmic_llm --deep-sleep")
    print("\n  Connect: run luna-thin (or: cargo run -p luna_thin_ui from LunaAI repo).")
    print("  Done.")


def _ensure_toml() -> None:
    """Ensure the 'toml' package is available; try to install if missing."""
    try:
        import toml  # noqa: F401
        return
    except ImportError:
        pass
    print("  Python package 'toml' required for writing config. Installing...")
    import subprocess
    r = subprocess.run(
        [sys.executable, "-m", "pip", "install", "toml"],
        capture_output=True,
        text=True,
        timeout=60,
    )
    if r.returncode == 0:
        print("  ✓ toml installed")
        return
    print("  Could not install toml. Run: pip install toml")
    print("  Then run this setup again.")
    sys.exit(1)


def run() -> None:
    print("  Luna AI Quick Setup")
    print("  —————————————————")
    _ensure_toml()
    # Ensure commands needed for MCP (uvx, npx) are installed
    required = commands_required_for_mcp_catalog()
    print("\n  Checking dependencies (uvx, npx for MCP):")
    all_ok, msgs = ensure_commands(required, install=True)
    for m in msgs:
        print(m)
    # Load catalog early so missing file fails before any other prompts (F7)
    catalog = _load_catalog()
    if not all_ok:
        print("  Some tools are missing. You can still continue; MCP servers that need them will fail until installed.")
        if input("  Continue anyway? [Y/n]: ").strip().lower() in ("n", "no"):
            sys.exit(1)
    sample_root = _sample_data_root()

    # 1) Provider & model & API key
    provider_id, provider = _ask_provider(catalog)
    model_id, context_window = _ask_model(provider_id, provider)
    api_key = _ask_api_key(provider.get("label", provider_id))
    backend = provider.get("backend", provider_id)
    if not api_key and backend not in ("ollama",):
        print("  Warning: API key is empty; this profile may fail until you add a key to config.")
    endpoint = provider.get("endpoint", "")

    # 2) Personality
    persona_id = _ask_personality()
    temperature = _ask_temperature()
    max_tokens = _ask_max_tokens()
    profile_name = _ask_profile_name(_suggest_profile_name(persona_id, model_id))

    # Install system prompt (global) from persona; set prompts if we created the file
    prompts: list[str] = []
    persona_path = get_persona_path(persona_id, sample_root)
    if persona_path.exists():
        install_system_prompt(persona_path.read_text(encoding="utf-8"))
        install_persona_as_profile_prompt(persona_id, profile_name, sample_root)
        prompts = [f"profiles/{profile_name}.md"]
    else:
        print("  Warning: Persona file not found (sample_data/personas); profile will have no per-profile prompt.")

    # 3) Create model preset + profile (config refinement shape)
    preset_name = profile_name
    preset = build_model_preset(
        preset_name=preset_name,
        backend=backend,
        model=model_id,
        endpoint=endpoint.strip() or "https://api.openai.com/v1/chat/completions",
        api_key=api_key,
        temperature=temperature,
        max_tokens=max_tokens,
        context_window_size=context_window,
    )
    profile = build_profile(
        model_preset_name=preset_name,
        prompts=prompts,
        tools_policy="default",
        hidden=False,
        summarize_threshold=0.7,
        context_window_size=context_window,
    )
    add_or_update_profile(
        profile_name,
        profile,
        set_default=True,
        preset_name=preset_name,
        preset=preset,
    )
    print(f"\n  Profile '{profile_name}' and model preset '{preset_name}' written to config.toml.")

    # 4) Server
    host, port, server_api_key = _ask_server()
    merge_server_into_config(host=host, port=port, api_key=server_api_key)
    thin_ui_path = write_thin_ui_server_config(host=host, port=port, api_key=server_api_key)
    connect_host = thin_ui_connect_host(host)
    print("  Server section written to config.toml.")
    print(f"  Thin UI configured: {thin_ui_path} → {connect_host}:{port} (run luna-thin to connect).")
    if host.strip() == "0.0.0.0":
        print("  (Thin UI uses localhost → same-machine only; for remote access edit server_config.toml host.)")

    # 4b) Install self_config skill (for skills MCP server)
    ok, msg = install_self_config_skill(_project_root())
    if ok:
        print(f"  {msg}")
    else:
        print("  ", msg)

    # 5) MCP (from catalog: no-setup servers only; Luna memory optional)
    selected_ids, add_luna_memory, luna_bin = _ask_mcp_servers_from_catalog()
    if selected_ids or add_luna_memory:
        mcp_config = merge_mcp_servers(
            selected_catalog_ids=selected_ids,
            add_luna_memory=add_luna_memory,
            luna_memory_binary=luna_bin,
            cosmic_llm_dir=paths.cosmic_llm_dir(),
            cosmic_llm_db_path=str(paths.cosmic_llm_dir() / "conversations.db"),
            catalog_path=_mcp_catalog_path(),
            merge=True,
        )
        save_mcp_config(mcp_config)
        print("  mcp_config.json written.")
    else:
        print("  Skipped MCP servers.")

    # 5b) Full Luna: tools_policy enabled_mcp + embedding + deep_sleep + titles
    full_luna = _ask_full_luna()
    embedding_api_key: str | None = None
    if full_luna and chat_backend_needs_embedding_key_prompt(backend):
        embedding_api_key = _ask_embedding_api_key()
        if not embedding_api_key:
            print(
                "  Warning: No embedding API key; memory RAG will stay off "
                "(set [embedding] in config.toml or OPENAI_API_KEY)."
            )

    enabled_mcp = enabled_mcp_for_selection(selected_ids, add_luna_memory)
    feature_summary = finalize_full_luna_config(
        profile_name=profile_name,
        enabled_mcp=enabled_mcp,
        chat_api_key=api_key,
        chat_backend=backend,
        full_luna=full_luna,
        embedding_api_key=embedding_api_key,
    )
    if enabled_mcp:
        print(f"  tools_policies.default.enabled_mcp = {enabled_mcp}")
    if feature_summary.get("embedding_on"):
        print("  [embedding] enabled for memory RAG.")
    if feature_summary.get("deep_sleep_on"):
        print(f"  [deep_sleep] enabled (profile={profile_name}).")
    if feature_summary.get("title_summary_on"):
        print("  [title_summary] enabled for auto titles.")

    # 6) User systemd: install service, enable and start
    installed_binary = Path("/usr/bin/cosmic_llm")
    dev_binary = _project_root().parent / "target" / "release" / "cosmic_llm"
    # Prefer installed binary when running from /usr/share (deb install); else dev if present
    if _project_root().resolve().parts[:3] == ("/", "usr", "share"):
        default_binary = installed_binary
    else:
        default_binary = dev_binary if dev_binary.exists() else installed_binary
    systemd_installed = False
    if input("\n  Install Luna server in user systemd (enable + start)? [Y/n]: ").strip().lower() not in ("n", "no"):
        binary_raw = input(f"  Path to cosmic_llm binary [{default_binary}]: ").strip() or str(default_binary)
        ok, msg = install_user_service(Path(binary_raw))
        systemd_installed = ok
        if ok:
            print(" ", msg)
        else:
            print("  ", msg)
            print("  You can install later: systemctl --user enable luna-server.service && systemctl --user start luna-server.service")

    _print_setup_summary(
        profile_name=profile_name,
        feature_summary=feature_summary,
        systemd_installed=systemd_installed,
    )


def main() -> None:
    run()


if __name__ == "__main__":
    main()
