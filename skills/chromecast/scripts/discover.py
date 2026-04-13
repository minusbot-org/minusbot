import pychromecast
import sys
import json


def main():
    try:
        data = json.loads(sys.argv[1]) if len(sys.argv) > 1 else {}
        timeout = data.get("timeout", 5)

        # Using get_chromecasts as suggested for better discovery
        casts, browser = pychromecast.get_chromecasts(timeout=timeout)
        devices = []

        for cast in casts:
            device = cast.cast_info
            devices.append(
                {
                    "friendly_name": device.friendly_name,
                    "model_name": device.model_name,
                    "host": device.host,
                    "port": device.port,
                    "uuid": str(device.uuid),
                }
            )

        # Stop discovery to clean up
        browser.stop_discovery()

        if not devices:
            print("No Chromecasts found on the local network.")
        else:
            print(json.dumps(devices, indent=2))

    except Exception as e:
        print(f"Error during discovery: {str(e)}")


if __name__ == "__main__":
    main()
