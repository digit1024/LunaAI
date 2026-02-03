"""
System prompt: choose persona from sample_data/personas and install to cosmic_llm.
"""
from __future__ import annotations

from pathlib import Path


# Persona id -> (label, filename in sample_data/personas)
PERSONAS = {
    "luna": ("Luna AI – sarcastic helpful assistant", "luna.md"),
    "vera": ("Vera – professional concise personal assistant", "vera.md"),
    "jude": ("Jude – wicked twisted and funny assistant", "jude.md"),
}


def get_personas() -> dict[str, str]:
    """Return dict of persona_id -> label."""
    return {k: v[0] for k, v in PERSONAS.items()}


def get_persona_path(persona_id: str, sample_data_root: Path) -> Path:
    """Path to persona markdown file under sample_data/personas."""
    if persona_id not in PERSONAS:
        raise ValueError(f"Unknown persona: {persona_id}")
    filename = PERSONAS[persona_id][1]
    return sample_data_root / "personas" / filename


def install_system_prompt(
    content: str,
    system_prompt_path: Path | None = None,
) -> None:
    """Write system prompt to cosmic_llm system_prompt.md."""
    from . import paths
    path = system_prompt_path or paths.system_prompt_path()
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content, encoding="utf-8")


def install_persona_as_profile_prompt(
    persona_id: str,
    profile_name: str,
    sample_data_root: Path,
    profiles_dir: Path | None = None,
) -> Path:
    """
    Copy persona file to cosmic_llm/profiles/<profile_name>.md.
    Returns path to the installed file.
    """
    from . import paths
    src = get_persona_path(persona_id, sample_data_root)
    dest_dir = profiles_dir or paths.profiles_dir()
    dest_dir.mkdir(parents=True, exist_ok=True)
    dest = dest_dir / f"{profile_name}.md"
    dest.write_text(src.read_text(encoding="utf-8"), encoding="utf-8")
    return dest
