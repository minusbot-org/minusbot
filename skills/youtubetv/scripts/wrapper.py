from __future__ import annotations

import sys
import json
import os
import socket
import contextlib
from typing import Any

try:
    from dial import DIALDevice, DIALClient, discover_devices
except ImportError:
    import dial

    DIALDevice = dial.DIALDevice  # type: ignore
    DIALClient = dial.DIALClient  # type: ignore
    discover_devices = dial.discover_devices  # type: ignore

from logging_utils import setup_logging

FAVORITES_FILE = os.environ.get("FAVORITES_FILE", "/data/youtubetv_favorites.json")

logger = setup_logging("youtubetv")


class YouTubeDIALClient:
    """YouTube-specific DIAL client built on top of DIALClient.

    Provides YouTube-specific methods for controlling YouTube on Chromecast
    devices using the DIAL (Discovery and Launch) protocol.

    Attributes:
        device: The underlying DIALDevice being controlled.
    """

    def __init__(self, device: DIALDevice, http_timeout: float = 8.0) -> None:
        """Initialize the YouTube DIAL client.

        Args:
            device: The DIAL device to control.
            http_timeout: HTTP request timeout in seconds.
        """
        self._client = DIALClient(device, http_timeout=http_timeout)
        self._device = device

    @property
    def device(self) -> DIALDevice:
        """Get the underlying DIAL device.

        Returns:
            The DIALDevice instance.
        """
        return self._device

    def launch_video(self, video_id: str) -> str:
        """Launch a YouTube video.

        Args:
            video_id: The YouTube video ID to play.

        Returns:
            The response from the launch request.
        """
        return self._client.launch("YouTube", {"v": video_id})

    def launch_playlist(self, playlist_id: str, video_id: str | None = None) -> str:
        """Launch a YouTube playlist.

        Args:
            playlist_id: The YouTube playlist ID.
            video_id: Optional video ID to start from.

        Returns:
            The response from the launch request.
        """
        params: dict[str, str] = {"list": playlist_id}
        if video_id:
            params["v"] = video_id
        return self._client.launch("YouTube", params)

    def launch_search(self, query: str) -> str:
        """Launch YouTube with a search query.

        Args:
            query: The search query string.

        Returns:
            The response from the launch request.
        """
        return self._client.launch("YouTube", {"q": query})

    def launch_channel(self, channel_id: str) -> str:
        """Launch a YouTube channel.

        Args:
            channel_id: The YouTube channel ID.

        Returns:
            The response from the launch request.
        """
        return self._client.launch("YouTube", {"channel": channel_id})

    def stop(self) -> bool:
        """Stop YouTube playback.

        Returns:
            True if the app was running and stopped, False otherwise.
        """
        return self._client.stop("YouTube")

    def get_app_state(self) -> dict[str, Any]:
        """Get the current state of the YouTube app.

        Returns:
            A dictionary containing app state information with keys:
            - name: App name
            - state: App state (running/stopped)
            - instance_url: Instance URL
            - additional_data: Additional app data
        """
        app_state = self._client.get_app_state("YouTube")
        return {
            "name": app_state.name,
            "state": app_state.state,
            "instance_url": app_state.instance_url,
            "additional_data": app_state.additional_data,
        }

    @classmethod
    def from_address(
        cls, host: str, port: int = 8008, app_path: str = "/apps"
    ) -> YouTubeDIALClient:
        """Create a client from a device address.

        Args:
            host: The device IP address or hostname.
            port: The device port (default: 8008).
            app_path: The app URL path (default: /apps).

        Returns:
            A new YouTubeDIALClient instance.
        """
        device = DIALDevice(
            friendly_name=host,
            manufacturer="",
            model_name="",
            app_url=f"http://{host}:{port}{app_path}",
            udn="",
            location="",
        )
        return cls(device)


def load_favorites() -> dict[str, dict[str, Any]]:
    """Load favorites from the favorites file.

    Converts legacy string favorites to dict format during migration.

    Returns:
        A dictionary mapping favorite names to device info with keys:
        - ip: Device IP address
        - port: Device port (default: 8008)
    """
    if not os.path.exists(FAVORITES_FILE):
        return {}
    try:
        with open(FAVORITES_FILE) as f:
            data = json.load(f)
            new_data = {}
            for k, v in data.items():
                if isinstance(v, str):
                    new_data[k] = {"ip": v, "port": 8008}
                else:
                    new_data[k] = v
            return new_data
    except json.JSONDecodeError:
        return {}


def save_favorites(
    favorites: dict[str, Any],
    filepath: str = "/data/youtubetv_favorites.json",
) -> None:
    """Save favorites to a file.

    Args:
        favorites: Dictionary mapping favorite names to device info.
        filepath: Path to save the favorites file.
    """
    migrated = {}
    for k, v in favorites.items():
        if isinstance(v, str):
            migrated[k] = {"ip": v, "port": 8008}
        else:
            migrated[k] = v

    os.makedirs(os.path.dirname(filepath), exist_ok=True)
    with open(filepath, "w") as f:
        json.dump(migrated, f, indent=2)


def resolve_client(device_arg: str | None, port_arg: int | None = None) -> YouTubeDIALClient | None:
    """Resolve a device argument to a YouTube DIAL client.

    Args:
        device_arg: Device name (favorite), IP address, or None.
        port_arg: Alternative port argument.

    Returns:
        YouTubeDIALClient for the resolved device.

    Raises:
        ValueError: If favorite is invalid.
        Exception: If device cannot be found.
    """
    favorites = load_favorites()

    if not device_arg:
        if favorites:
            name, data = next(iter(favorites.items()))
            ip = data.get("ip")
            if not ip or not isinstance(ip, str):
                raise ValueError("Invalid favorite: missing IP")
            port = data.get("port", 8008)
            logger.info("Using default favorite: %s (%s:%s)", name, ip, port)
            return YouTubeDIALClient.from_address(ip, port=int(port))

        logger.info("No device specified and no favorites found. Scanning...")
        devices = discover_devices(timeout=3)
        if devices:
            return YouTubeDIALClient(devices[0])
        else:
            raise Exception("No devices found on network.")

    if device_arg in favorites:
        data = favorites[device_arg]
        ip = data.get("ip")
        if not ip or not isinstance(ip, str):
            raise ValueError(f"Invalid favorite '{device_arg}': missing IP")
        port = data.get("port", 8008)
        return YouTubeDIALClient.from_address(ip, port=int(port))

    if device_arg:
        try:
            socket.inet_aton(device_arg)
            port_arg_int = int(port_arg) if port_arg else 8008
            return YouTubeDIALClient.from_address(device_arg, port=port_arg_int)
        except (OSError, ValueError):
            pass

    logger.info("Device '%s' not in favorites. Scanning...", device_arg)
    devices = discover_devices(timeout=3)
    for dev in devices:
        if dev.friendly_name == device_arg:
            return YouTubeDIALClient(dev)

    for dev in devices:
        if device_arg.lower() in dev.friendly_name.lower():
            logger.info("Found partial match: %s", dev.friendly_name)
            return YouTubeDIALClient(dev)

    raise Exception(f"Device '{device_arg}' not found.")


def handle_discover(**kwargs: Any) -> None:
    """Discover YouTube-compatible devices on the network.

    Args:
        **kwargs: Keyword arguments including 'timeout' (default: 5 seconds).

    Returns:
        None - results are printed to stdout as JSON.
    """
    timeout = kwargs.get("timeout", 5)
    logger.info("Scanning for devices (timeout=%ss)...", timeout)
    devices = discover_devices(timeout=float(timeout))
    results = []
    for d in devices:
        port = 8008
        with contextlib.suppress(Exception):
            port = int(d.app_url.split(":")[2].split("/")[0])

        results.append(
            {
                "friendly_name": d.friendly_name,
                "model_name": d.model_name,
                "manufacturer": d.manufacturer,
                "app_url": d.app_url,
                "udn": d.udn,
                "ip": d.app_url.split("//")[1].split(":")[0],
                "port": port,
            }
        )
    print(json.dumps(results, indent=2))


def handle_save_favorite(name: str, ip: str, port: int) -> None:
    """Save a device as a favorite.

    Args:
        name: The name to give this favorite.
        ip: The device IP address.
        port: The device port (default: 8008).

    Returns:
        None - results are printed to stdout as JSON.
    """
    favs = load_favorites()
    favs[name] = {"ip": ip, "port": port}
    save_favorites(favs, filepath=FAVORITES_FILE)
    print(
        json.dumps(
            {
                "status": "success",
                "message": f"Saved favorite '{name}' with IP {ip}:{port}",
            }
        )
    )


def handle_list_saved() -> None:
    """List all saved favorites.

    Returns:
        None - results are printed to stdout as JSON.
    """
    favs = load_favorites()
    results = []
    for name, data in favs.items():
        results.append({"name": name, "ip": data.get("ip"), "port": data.get("port", 8008)})
    print(json.dumps(results, indent=2))


def handle_control(action: str, device_arg: str | None, **kwargs: Any) -> None:
    """Control YouTube playback on a device.

    Args:
        action: The action to perform (play_video, play_playlist, search, channel, stop, status).
        device_arg: Device name or IP address.
        **kwargs: Action-specific arguments.

    Returns:
        None - results are printed to stdout as JSON.
    """
    port_arg = kwargs.get("port")
    client = resolve_client(device_arg, port_arg)
    if not client:
        print(json.dumps({"status": "error", "message": "Failed to resolve client"}))
        return

    result = ""
    try:
        if action == "play_video":
            vid = kwargs.get("video_id")
            playlist = kwargs.get("playlist_id")
            if not vid:
                raise ValueError("video_id is required")

            if playlist:
                client.launch_playlist(playlist, video_id=vid)
                result = f"Launched playlist {playlist} starting at video {vid}"
            else:
                client.launch_video(vid)
                result = f"Launched video {vid}"

        elif action == "play_playlist":
            playlist = kwargs.get("playlist_id")
            if not playlist:
                raise ValueError("playlist_id is required")
            client.launch_playlist(playlist)
            result = f"Launched playlist {playlist}"

        elif action == "search":
            query = kwargs.get("query")
            if not query:
                raise ValueError("query is required")
            client.launch_search(query)
            result = f"Launched search for '{query}'"

        elif action == "channel":
            channel = kwargs.get("channel_id")
            if not channel:
                raise ValueError("channel_id is required")
            client.launch_channel(channel)
            result = f"Launched channel {channel}"

        elif action == "stop":
            stopped = client.stop()
            result = "Stopped playback" if stopped else "App was not running"

        elif action == "status":
            state = client.get_app_state()
            print(
                json.dumps(
                    {
                        "name": state.get("name", "YouTube"),
                        "state": state.get("state", "running"),
                        "instance_url": state.get("instance_url", ""),
                        "additional_data": state.get("additional_data", {}),
                    },
                    indent=2,
                )
            )
            return

        else:
            print(json.dumps({"status": "error", "message": f"Unknown control action: {action}"}))
            return

        print(
            json.dumps(
                {
                    "status": "success",
                    "message": result,
                    "device": client.device.friendly_name,
                }
            )
        )

    except ValueError:
        print(json.dumps({"status": "error", "message": "Operation failed"}))


def main() -> None:
    """Main entry point for the CLI.

    Reads mode and arguments from command line JSON and dispatches
    to appropriate handler function.

    Usage:
        python wrapper.py '{"mode": "discover"}'
        python wrapper.py '{"mode": "control", "action": "play_video", "video_id": "abc123"}'
    """
    if len(sys.argv) < 2:
        print(json.dumps({"error": "No input provided"}))
        return

    try:
        data = json.loads(sys.argv[1])
    except json.JSONDecodeError:
        print(json.dumps({"error": "Invalid JSON input"}))
        return

    mode = data.get("mode")

    if mode == "discover":
        handle_discover(**data)
    elif mode == "save":
        name = data.get("friendly_name")
        ip = data.get("ip")
        port = data.get("port", 8008)
        if not name or not ip:
            print(json.dumps({"error": "friendly_name and ip required for save"}))
            return
        handle_save_favorite(name, ip, int(port))
    elif mode == "list":
        handle_list_saved()
    elif mode == "control":
        action = data.get("action")
        device = data.get("device")
        video_id = data.get("video_id")
        playlist_id = data.get("playlist_id")
        query = data.get("query")
        channel_id = data.get("channel_id")
        port = data.get("port")

        handle_control(
            action,
            device,
            video_id=video_id,
            playlist_id=playlist_id,
            query=query,
            channel_id=channel_id,
            port=port,
        )
    else:
        print(json.dumps({"error": f"Unknown mode: {mode}"}))


if __name__ == "__main__":
    main()
