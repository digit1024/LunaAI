"""
User systemd setup: install luna-server.service, enable and start it.
"""
from __future__ import annotations

import subprocess
from pathlib import Path

from . import paths

SERVICE_NAME = "luna-server.service"


def service_file_content(binary_path: Path) -> str:
    """Generate [Unit] and [Service] for Luna server (user systemd)."""
    binary = binary_path.resolve()
    return f"""[Unit]
Description=Luna AI Server
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
ExecStart={binary}
Restart=on-failure
RestartSec=5
StandardOutput=journal
StandardError=journal

# Use same config as when run interactively (~/.local/share/cosmic_llm/config.toml)
NoNewPrivileges=true
PrivateTmp=true

[Install]
WantedBy=default.target
"""


def install_user_service(binary_path: Path) -> tuple[bool, str]:
    """
    Write luna-server.service to ~/.config/systemd/user/, daemon-reload,
    enable and start. Returns (success, message).
    """
    binary_path = Path(binary_path)
    if not binary_path.exists():
        return False, f"Binary not found: {binary_path}"
    unit_dir = paths.user_systemd_dir()
    unit_path = paths.luna_server_service_path()
    unit_dir.mkdir(parents=True, exist_ok=True)
    unit_path.write_text(service_file_content(binary_path), encoding="utf-8")
    try:
        subprocess.run(
            ["systemctl", "--user", "daemon-reload"],
            check=True,
            timeout=10,
            capture_output=True,
            text=True,
        )
    except (subprocess.CalledProcessError, FileNotFoundError, subprocess.TimeoutExpired) as e:
        return False, f"daemon-reload failed: {e}"
    try:
        subprocess.run(
            ["systemctl", "--user", "enable", SERVICE_NAME],
            check=True,
            timeout=10,
            capture_output=True,
            text=True,
        )
    except (subprocess.CalledProcessError, FileNotFoundError, subprocess.TimeoutExpired) as e:
        return False, f"enable failed: {e}"
    try:
        subprocess.run(
            ["systemctl", "--user", "start", SERVICE_NAME],
            check=True,
            timeout=10,
            capture_output=True,
            text=True,
        )
    except (subprocess.CalledProcessError, FileNotFoundError, subprocess.TimeoutExpired) as e:
        return False, f"start failed: {e}"
    return True, (
        f"Installed {unit_path}, enabled and started {SERVICE_NAME}. "
        "To run at boot (without login): loginctl enable-linger $USER"
    )
