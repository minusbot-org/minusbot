import json
import os
import pytest
from unittest.mock import Mock, patch

import sys

sys.path.insert(0, os.path.dirname(__file__))

from wrapper import (
    load_favorites,
    save_favorites,
    resolve_client,
    handle_discover,
    handle_save_favorite,
    handle_list_saved,
    handle_control,
)


class TestLoadFavorites:
    def test_load_favorites_empty(self, tmp_path, monkeypatch):
        json_file = tmp_path / "test_favorites.json"
        json_file.write_text("{}")

        monkeypatch.setenv("FAVORITES_FILE", str(json_file))

        with patch.dict(os.environ, {"FAVORITES_FILE": str(json_file)}):
            result = load_favorites()
            assert result == {}

    def test_load_favorites_valid(self, tmp_path, monkeypatch):
        json_file = tmp_path / "test_favorites.json"
        data = {"living_room": {"ip": "192.168.1.100", "port": 8008}}
        json_file.write_text(json.dumps(data))

        monkeypatch.setattr("wrapper.FAVORITES_FILE", str(json_file))
        result = load_favorites()
        assert result == data

    def test_load_favorites_migration(self, tmp_path, monkeypatch):
        json_file = tmp_path / "test_favorites.json"
        old_data = {"living_room": "192.168.1.100"}
        json_file.write_text(json.dumps(old_data))

        monkeypatch.setattr("wrapper.FAVORITES_FILE", str(json_file))
        result = load_favorites()
        assert result == {"living_room": {"ip": "192.168.1.100", "port": 8008}}

    def test_load_favorites_invalid_json(self, tmp_path, monkeypatch):
        json_file = tmp_path / "test_favorites.json"
        json_file.write_text("invalid json")

        monkeypatch.setattr("wrapper.FAVORITES_FILE", str(json_file))
        result = load_favorites()
        assert result == {}


class TestSaveFavorites:
    def test_save_favorites(self, tmp_path):
        favorites = {
            "living_room": {"ip": "192.168.1.100", "port": 8008},
            "bedroom": {"ip": "192.168.1.101", "port": 8009},
        }
        json_file = tmp_path / "test_favorites.json"

        save_favorites(favorites, str(json_file))

        assert json_file.exists()
        loaded = json.loads(json_file.read_text())
        assert loaded == favorites

    def test_save_favorites_migration(self, tmp_path):
        favorites = {"living_room": "192.168.1.100"}
        json_file = tmp_path / "test_favorites.json"

        save_favorites(favorites, str(json_file))

        assert json_file.exists()
        loaded = json.loads(json_file.read_text())
        assert loaded == {"living_room": {"ip": "192.168.1.100", "port": 8008}}


class TestResolveClient:
    def test_resolve_client_with_ip(self):
        client = resolve_client("192.168.1.100", 8008)
        assert client is not None
        assert client.device.friendly_name == "192.168.1.100"

    def test_resolve_client_invalid_favorite(self, tmp_path, monkeypatch):
        json_file = tmp_path / "test_favorites.json"
        favorites = {"bad_device": {"ip": 12345}}
        json_file.write_text(json.dumps(favorites))

        monkeypatch.setattr("wrapper.FAVORITES_FILE", str(json_file))

        with pytest.raises(ValueError, match="Invalid favorite"):
            resolve_client("bad_device")


class TestHandleDiscover:
    def test_handle_discover(self, capsys):
        mock_device = Mock()
        mock_device.friendly_name = "Chromecast 1"
        mock_device.model_name = "Cast Device"
        mock_device.manufacturer = "Google"
        mock_device.app_url = "http://192.168.1.100:8008/apps"
        mock_device.udn = "uuid:12345"

        with patch("wrapper.discover_devices") as mock_discover:
            mock_discover.return_value = [mock_device]
            handle_discover(timeout=5)

        captured = capsys.readouterr()
        result = json.loads(captured.out)
        assert len(result) == 1
        assert result[0]["friendly_name"] == "Chromecast 1"


class TestHandleSaveFavorite:
    def test_handle_save_favorite(self, tmp_path, capsys, monkeypatch):
        json_file = tmp_path / "test_favorites.json"

        with (
            patch("wrapper.FAVORITES_FILE", str(json_file)),
            patch("wrapper.save_favorites"),
        ):
            handle_save_favorite("living_room", "192.168.1.100", 8008)

        captured = capsys.readouterr()
        result = json.loads(captured.out)
        assert result["status"] == "success"
        assert "192.168.1.100:8008" in result["message"]


class TestHandleListSaved:
    def test_handle_list_saved_empty(self, tmp_path, capsys, monkeypatch):
        json_file = tmp_path / "test_favorites.json"
        json_file.write_text("{}")

        with patch("wrapper.FAVORITES_FILE", str(json_file)):
            handle_list_saved()

        captured = capsys.readouterr()
        result = json.loads(captured.out)
        assert result == []

    def test_handle_list_saved_with_data(self, tmp_path, capsys, monkeypatch):
        json_file = tmp_path / "test_favorites.json"
        favorites = {
            "living_room": {"ip": "192.168.1.100", "port": 8008},
            "bedroom": {"ip": "192.168.1.101", "port": 8009},
        }
        json_file.write_text(json.dumps(favorites))

        with patch("wrapper.FAVORITES_FILE", str(json_file)):
            handle_list_saved()

        captured = capsys.readouterr()
        result = json.loads(captured.out)
        assert len(result) == 2


class TestHandleControl:
    def test_handle_control_play_video(self, tmp_path, capsys, monkeypatch):
        json_file = tmp_path / "test_favorites.json"
        favorites = {"living_room": {"ip": "192.168.1.100", "port": 8008}}
        json_file.write_text(json.dumps(favorites))

        with (
            patch("wrapper.FAVORITES_FILE", str(json_file)),
            patch("wrapper.load_favorites", return_value=favorites),
            patch("wrapper.YouTubeDIALClient.from_address") as MockClient,
        ):
            mock_instance = Mock()
            mock_device = Mock()
            mock_device.friendly_name = "living_room"
            mock_instance.device = mock_device
            mock_instance.launch_video.return_value = "launched"
            MockClient.return_value = mock_instance

            handle_control("play_video", "living_room", video_id="abc123")

        captured = capsys.readouterr()
        result = json.loads(captured.out)
        assert result["status"] == "success"
        assert "abc123" in result["message"]

    def test_handle_control_invalid_video_id(self, tmp_path, capsys, monkeypatch):
        json_file = tmp_path / "test_favorites.json"
        favorites = {"living_room": {"ip": "192.168.1.100", "port": 8008}}
        json_file.write_text(json.dumps(favorites))

        with patch("wrapper.FAVORITES_FILE", str(json_file)):
            handle_control("play_video", "living_room")

        captured = capsys.readouterr()
        result = json.loads(captured.out)
        assert result["status"] == "error"

    def test_handle_control_unknown_action(self, tmp_path, capsys, monkeypatch):
        json_file = tmp_path / "test_favorites.json"
        favorites = {"living_room": {"ip": "192.168.1.100", "port": 8008}}
        json_file.write_text(json.dumps(favorites))

        with patch("wrapper.FAVORITES_FILE", str(json_file)):
            handle_control("unknown_action", "living_room")

        captured = capsys.readouterr()
        result = json.loads(captured.out)
        assert result["status"] == "error"


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
