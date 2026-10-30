"""Configuration: defaults, loading and saving."""

import json
from pathlib import Path

from .constants import SYNC_DIR_DEFAULT

CONFIG_DIR = Path.home() / ".config" / "gdrive"
CONFIG_FILE = CONFIG_DIR / "settings.json"
LOG_DIR = CONFIG_DIR

DEFAULTS = {
    "sync_dir": SYNC_DIR_DEFAULT,
    "sync_interval": "manual",
    "watch_changes": False,
    "paused": False,
    "bandwidth_limit": "",
    "download_limit": "",
    "upload_limit": "",
    "selected_folders": [],
    "excluded_patterns": [
        "*.tmp",
        "*.swp",
        ".DS_Store",
        "Thumbs.db",
        "~$*",
        "**/node_modules/**",
        "**/venv/**",
        "**/.venv/**",
        "**/__pycache__/**",
        "**/target/**",
    ],
    "sync_mode": "mirror",
    "notifications_enabled": True,
    "launch_on_startup": True,
    "start_hidden": True,
    "show_in_file_manager": True,
    "proxy_enabled": False,
    "proxy_url": "",
    "encryption_enabled": False,
    "logging_enabled": True,
    "log_level": "info",
    "log_rotate": True,
    "log_max_size_mb": 10,
    "log_max_files": 5,
    "conflict_action": "newer",
    "remove_empty_dirs": False,
    "skip_hidden": True,
    "store_metadata": True,
    "num_threads": 8,
    "language": "es",
    "window_width": 1000,
    "window_height": 700,
}


def load_config():
    CONFIG_DIR.mkdir(parents=True, exist_ok=True)
    if CONFIG_FILE.exists():
        try:
            with open(CONFIG_FILE) as f:
                cfg = json.load(f)
            for k, v in DEFAULTS.items():
                cfg.setdefault(k, v)
            return cfg
        except Exception:
            pass
    return dict(DEFAULTS)


def save_config(cfg):
    CONFIG_DIR.mkdir(parents=True, exist_ok=True)
    with open(CONFIG_FILE, "w") as f:
        json.dump(cfg, f, indent=2)
