"""
dial.py — DIAL (Discovery and Launch) protocol library
=======================================================

Generic DIAL client for controlling apps on smart TVs and Chromecasts via
SSDP discovery and HTTP REST commands.

DIAL specification: http://www.dial-multiscreen.org/dial-protocol-specification

Usage::

    from dial import discover_devices, DIALClient

    devices = discover_devices()
    client  = DIALClient(devices[0])
    client.launch("YouTube", "v=dQw4w9WgXcQ")
"""

from __future__ import annotations

import logging
import re
import socket
import time
import urllib.parse
import xml.etree.ElementTree as ET
from dataclasses import dataclass, field

import requests

logger = logging.getLogger(__name__)

# ---------------------------------------------------------------------------
# Exceptions
# ---------------------------------------------------------------------------


class DIALError(Exception):
    """Base exception for all DIAL errors."""


class DeviceNotFoundError(DIALError):
    """Raised when no DIAL devices are found on the network."""


class AppNotRunningError(DIALError):
    """Raised when an operation requires the app to be running but it is not."""


class LaunchError(DIALError):
    """Raised when a DIAL launch request fails."""


# ---------------------------------------------------------------------------
# Constants
# ---------------------------------------------------------------------------

_SSDP_ADDR = "239.255.255.250"
_SSDP_PORT = 1900
_SSDP_MX = 3
_SSDP_ST = "urn:dial-multiscreen-org:service:dial:1"

_NS = {
    "dial": "urn:dial-multiscreen-org:schemas:dial",
    "upnp": "urn:schemas-upnp-org:device-1-0",
}

# ---------------------------------------------------------------------------
# Data classes
# ---------------------------------------------------------------------------


@dataclass
class DIALDevice:
    """Represents a DIAL-capable device discovered on the network."""

    friendly_name: str
    """Human-readable name reported by the device."""

    manufacturer: str
    """Device manufacturer string."""

    model_name: str
    """Device model name."""

    app_url: str
    """Base URL of the DIAL REST service (e.g. ``http://192.168.1.5:8008/apps``)."""

    udn: str
    """Unique Device Name (UDN) from the UPnP description."""

    location: str
    """Raw LOCATION header returned during SSDP discovery."""

    extra: dict = field(default_factory=dict)
    """Any additional fields parsed from the UPnP device description."""

    def __str__(self) -> str:
        return (
            f"{self.friendly_name} "
            f"({self.manufacturer} {self.model_name}) @ {self.app_url}"
        )


@dataclass
class AppState:
    """State of a DIAL application on a device."""

    name: str
    """Application name (e.g. ``YouTube``)."""

    state: str
    """Running state: ``running``, ``stopped``, or ``installable``."""

    instance_url: str | None
    """URL of the running app instance (present only when *running*)."""

    additional_data: dict = field(default_factory=dict)
    """Extra ``<additionalData>`` key-value pairs from the response."""


# ---------------------------------------------------------------------------
# SSDP discovery
# ---------------------------------------------------------------------------


def _ssdp_request() -> bytes:
    return (
        "M-SEARCH * HTTP/1.1\r\n"
        f"HOST: {_SSDP_ADDR}:{_SSDP_PORT}\r\n"
        'MAN: "ssdp:discover"\r\n'
        f"MX: {_SSDP_MX}\r\n"
        f"ST: {_SSDP_ST}\r\n"
        "\r\n"
    ).encode()


def _parse_ssdp_response(raw: bytes) -> dict[str, str]:
    """Parse raw SSDP HTTP response headers into a dict (keys lower-cased)."""
    headers: dict[str, str] = {}
    for line in raw.decode(errors="replace").splitlines()[1:]:
        if ":" in line:
            key, _, value = line.partition(":")
            headers[key.strip().lower()] = value.strip()
    return headers


def _fetch_device_description(location: str, timeout: float) -> dict | None:
    """
    Fetch and parse a UPnP device description XML from *location*.

    Returns a dict with device metadata or ``None`` on failure.
    """
    try:
        resp = requests.get(location, timeout=timeout)
        resp.raise_for_status()
    except requests.RequestException as exc:
        logger.debug("Failed to fetch device description from %s: %s", location, exc)
        return None

    app_url: str | None = resp.headers.get("Application-URL")

    try:
        root = ET.fromstring(resp.text)
    except ET.ParseError as exc:
        logger.debug(
            "Failed to parse device description XML from %s: %s", location, exc
        )
        return None

    def _find(tag: str, ns: str = "upnp") -> str:
        el = root.find(f".//{{{_NS[ns]}}}{tag}")
        return el.text.strip() if el is not None and el.text else ""

    if not app_url:
        app_url = _find("X_DIALEX_DeviceID") or ""

    return {
        "friendly_name": _find("friendlyName"),
        "manufacturer": _find("manufacturer"),
        "model_name": _find("modelName"),
        "udn": _find("UDN"),
        "app_url": app_url,
    }


def discover_devices(
    timeout: float = 5.0,
    max_devices: int = 20,
) -> list[DIALDevice]:
    """
    Discover DIAL-capable devices on the local network using SSDP.

    Parameters
    ----------
    timeout:
        Total seconds to listen for SSDP responses.
    max_devices:
        Stop after collecting this many unique device locations.

    Returns
    -------
    list[DIALDevice]
        Discovered devices sorted by friendly name.  Empty if none found.
    """
    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM, socket.IPPROTO_UDP)
    sock.setsockopt(socket.IPPROTO_IP, socket.IP_MULTICAST_TTL, 4)
    sock.settimeout(timeout)

    try:
        sock.sendto(_ssdp_request(), (_SSDP_ADDR, _SSDP_PORT))
        logger.debug("SSDP M-SEARCH sent; waiting %.1f s…", timeout)

        seen: set[str] = set()
        deadline = time.monotonic() + timeout

        while time.monotonic() < deadline and len(seen) < max_devices:
            try:
                data, _ = sock.recvfrom(4096)
            except TimeoutError:
                break
            location = _parse_ssdp_response(data).get("location", "")
            if location and location not in seen:
                seen.add(location)
                logger.debug("Found SSDP location: %s", location)
    finally:
        sock.close()

    devices: list[DIALDevice] = []
    for location in seen:
        info = _fetch_device_description(location, timeout=3.0)
        if not info or not info.get("app_url"):
            continue
        device = DIALDevice(
            friendly_name=info["friendly_name"] or location,
            manufacturer=info["manufacturer"],
            model_name=info["model_name"],
            app_url=info["app_url"].rstrip("/"),
            udn=info["udn"],
            location=location,
        )
        devices.append(device)
        logger.info("Discovered: %s", device)

    devices.sort(key=lambda d: d.friendly_name.lower())
    return devices


# ---------------------------------------------------------------------------
# Generic DIAL REST client
# ---------------------------------------------------------------------------


class DIALClient:
    """
    Generic DIAL REST client bound to a single *device*.

    Provides low-level ``launch()``, ``get_app_state()``, and ``stop()``
    methods that work with any DIAL application by name.

    For a YouTube-specific high-level API see :mod:`ytv_dial`.

    Parameters
    ----------
    device:
        A :class:`DIALDevice` from :func:`discover_devices`, or built manually.
    http_timeout:
        Seconds before an HTTP request times out.

    Example
    -------
    ::

        devices = discover_devices()
        client  = DIALClient(devices[0])
        client.launch("YouTube", "v=dQw4w9WgXcQ")
    """

    def __init__(self, device: DIALDevice, http_timeout: float = 8.0) -> None:
        self._device = device
        self._timeout = http_timeout
        self._session = requests.Session()
        self._session.headers.update(
            {
                "Content-Type": "text/plain; charset=utf-8",
                "Origin": "package:com.google.android.youtube.tv",
            }
        )

    # ------------------------------------------------------------------
    # Properties
    # ------------------------------------------------------------------

    @property
    def device(self) -> DIALDevice:
        """The target :class:`DIALDevice`."""
        return self._device

    # ------------------------------------------------------------------
    # Internal helpers
    # ------------------------------------------------------------------

    def _app_endpoint(self, app_name: str) -> str:
        return f"{self._device.app_url}/{app_name}"

    def _get(self, url: str) -> requests.Response:
        resp = self._session.get(url, timeout=self._timeout)
        resp.raise_for_status()
        return resp

    def _post(self, url: str, body: str = "") -> requests.Response:
        resp = self._session.post(url, data=body.encode(), timeout=self._timeout)
        resp.raise_for_status()
        return resp

    def _delete(self, url: str) -> requests.Response:
        resp = self._session.delete(url, timeout=self._timeout)
        resp.raise_for_status()
        return resp

    # ------------------------------------------------------------------
    # Public API
    # ------------------------------------------------------------------

    def get_app_state(self, app_name: str) -> AppState:
        """
        Query the current state of *app_name* on the target device.

        Parameters
        ----------
        app_name:
            DIAL application name, e.g. ``"YouTube"``.

        Returns
        -------
        AppState

        Raises
        ------
        requests.HTTPError
        """
        resp = self._get(self._app_endpoint(app_name))
        root = ET.fromstring(resp.text)

        state_el = root.find(f"{{{_NS['dial']}}}state")
        state = (
            state_el.text.strip()
            if state_el is not None and state_el.text
            else "unknown"
        )

        instance_url: str | None = resp.headers.get("Location")
        link_el = root.find(f"{{{_NS['dial']}}}link")
        if link_el is not None:
            instance_url = link_el.get("href") or instance_url

        additional: dict[str, str] = {}
        ad_el = root.find(f"{{{_NS['dial']}}}additionalData")
        if ad_el is not None:
            for child in ad_el:
                tag = re.sub(r"\{[^}]+\}", "", child.tag)
                additional[tag] = child.text or ""

        return AppState(
            name=app_name,
            state=state,
            instance_url=instance_url,
            additional_data=additional,
        )

    def launch(self, app_name: str, params: str | dict) -> str:
        """
        POST a launch request to a DIAL application.

        Parameters
        ----------
        app_name:
            DIAL application name, e.g. ``"YouTube"``.
        params:
            Either a pre-encoded query string (``"v=dQw4w9WgXcQ"``) or a
            plain dict (``{"v": "dQw4w9WgXcQ"}``).

        Returns
        -------
        str
            The ``Location`` header of the newly started app instance.
        """
        body = urllib.parse.urlencode(params) if isinstance(params, dict) else params
        url = self._app_endpoint(app_name)
        logger.debug("DIAL POST %s  body=%s", url, body)
        resp = self._post(url, body)
        location = resp.headers.get("Location", "")
        logger.info(
            "Launched %s on %s → %s", app_name, self._device.friendly_name, location
        )
        return location

    def stop(self, app_name: str) -> bool:
        """
        Stop *app_name* on the target device.

        Parameters
        ----------
        app_name:
            DIAL application name, e.g. ``"YouTube"``.

        Returns
        -------
        bool
            ``True`` if the app was stopped, ``False`` if it was not running.

        Raises
        ------
        requests.HTTPError
        """
        state = self.get_app_state(app_name)
        if state.state != "running":
            logger.info(
                "%s is not running on %s (state=%s)",
                app_name,
                self._device.friendly_name,
                state.state,
            )
            return False

        target = state.instance_url or self._app_endpoint(app_name) + "/run"
        logger.debug("DIAL DELETE %s", target)
        self._delete(target)
        logger.info("Stopped %s on %s", app_name, self._device.friendly_name)
        return True

    # ------------------------------------------------------------------
    # Convenience factory
    # ------------------------------------------------------------------

    @classmethod
    def from_address(
        cls,
        host: str,
        port: int = 8008,
        app_path: str = "/apps",
        **kwargs,
    ) -> DIALClient:
        """
        Build a :class:`DIALClient` directly from a host address (no SSDP).

        Parameters
        ----------
        host:
            Device IP address or hostname.
        port:
            DIAL service port (Chromecast default: ``8008``).
        app_path:
            Path prefix for the DIAL REST API.

        Example
        -------
        ::

            client = DIALClient.from_address("192.168.1.100")
            client.launch("YouTube", {"v": "dQw4w9WgXcQ"})
        """
        device = DIALDevice(
            friendly_name=host,
            manufacturer="",
            model_name="",
            app_url=f"http://{host}:{port}{app_path}",
            udn="",
            location="",
        )
        return cls(device, **kwargs)
