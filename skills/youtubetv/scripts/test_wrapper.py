"""Unit tests for wrapper.py functions."""

from wrapper import load_favorites, save_favorites

import json
import os
import tempfile


def test_load_favorites_empty() -> None:
    """Test loading favorites when file doesn't exist."""
    favorites = load_favorites()
    assert isinstance(favorites, dict)


def test_save_favorites() -> None:
    """Test saving favorites creates the file in temp directory."""
    test_favs = {"test": {"ip": "192.168.1.1", "port": 8008}}

    with tempfile.TemporaryDirectory() as tmpdir:
        fav_file = os.path.join(tmpdir, "test_favorites.json")

        save_favorites(test_favs, fav_file)

        assert os.path.exists(fav_file)

        with open(fav_file) as f:
            loaded = json.load(f)
            assert loaded == test_favs


def test_save_favorites_migration() -> None:
    """Test migration from old format (str) to new format (dict)."""
    old_format = {"test": "192.168.1.1"}
    expected = {"test": {"ip": "192.168.1.1", "port": 8008}}

    with tempfile.TemporaryDirectory() as tmpdir:
        fav_file = os.path.join(tmpdir, "test_favorites.json")

        save_favorites(old_format, fav_file)

        with open(fav_file) as f:
            loaded = json.load(f)
            assert loaded == expected
