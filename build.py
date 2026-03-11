#!/usr/bin/env python3
"""Build script to package Steam Library Organizer as a standalone .exe"""

import PyInstaller.__main__

PyInstaller.__main__.run(
    [
        "organizer.py",
        "--onefile",
        "--name=SteamLibraryOrganizer",
        "--console",  # needs a terminal for interactive prompts
        "--clean",
        # Hidden imports that PyInstaller sometimes misses
        "--hidden-import=anthropic",
        "--hidden-import=rich",
    ]
)
