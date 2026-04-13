#!/usr/bin/env python3
"""
ytv_dial.py — YouTube DIAL controller: library + CLI
=====================================================

High-level YouTube wrapper around the generic :mod:`dial` library.
"""

from __future__ import annotations

import logging

from dial import AppState, DIALClient, DIALDevice

logger = logging.getLogger(__name__)

_YT_APP = "YouTube"


class YouTubeDIALClient:
    """YouTube-specific DIAL client built on top of DIALClient."""

    def __init__(self, device: DIALDevice, http_timeout: float = 8.0) -> None:
        self._client = DIALClient(device, http_timeout=http_timeout)

    @property
    def device(self) -> DIALDevice:
        """The target DIALDevice."""
        return self._client.device

    def get_app_state(self) -> AppState:
        """Query the current YouTube app state on the device."""
        return self._client.get_app_state(_YT_APP)

    def launch_video(self, video_id: str) -> str:
        """Launch YouTube and start playing a video."""
        return self._client.launch(_YT_APP, {"v": video_id})

    def launch_playlist(self, playlist_id: str, video_id: str | None = None) -> str:
        """Launch YouTube and start playing a playlist."""
        params: dict[str, str] = {"list": playlist_id}
        if video_id:
            params["v"] = video_id
        return self._client.launch(_YT_APP, params)

    def launch_search(self, query: str) -> str:
        """Open a YouTube search on the TV."""
        return self._client.launch(_YT_APP, {"q": query})

    def launch_channel(self, channel_id: str) -> str:
        """Navigate to a YouTube channel page on the TV."""
        return self._client.launch(_YT_APP, {"channel": channel_id})

    def stop(self) -> bool:
        """Stop the YouTube app on the target device."""
        return self._client.stop(_YT_APP)

    @classmethod
    def from_address(
        cls, host: str, port: int = 8008, app_path: str = "/apps", **kwargs
    ) -> YouTubeDIALClient:
        """Build a YouTubeDIALClient directly from a host address."""
        inner = DIALClient.from_address(host, port=port, app_path=app_path)
        return cls(inner.device, **kwargs)
