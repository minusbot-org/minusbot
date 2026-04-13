import pychromecast
import sys
import json
import os


def main():
    try:
        data = json.loads(sys.argv[1])
        url = data.get("url")
        content_type = data.get("content_type", "video/mp4")
        device_name = data.get("device_name")

        if not device_name:
            fav_path = "/data/favorite.json"
            if os.path.exists(fav_path):
                with open(fav_path, "r") as f:
                    fav = json.load(f)
                    device_name = fav.get("friendly_name")

        if not device_name:
            print("Error: No device specified and no favorite saved.")
            return

        print(f"Connecting to {device_name}...")
        chromecasts, browser = pychromecast.get_listed_chromecasts(
            friendly_names=[device_name]
        )

        if not chromecasts:
            # Fallback to general discovery if direct listed fails
            chromecasts, browser = pychromecast.get_chromecasts()
            cast = next(
                (cc for cc in chromecasts if cc.cast_info.friendly_name == device_name),
                None,
            )
        else:
            cast = chromecasts[0]

        if not cast:
            print(f"Device '{device_name}' not found.")
            browser.stop_discovery()
            return

        browser.stop_discovery()
        cast.wait()
        mc = cast.media_controller
        mc.play_media(url, content_type)
        mc.block_until_active()

        print(f"Now playing {url} on {device_name}")

    except Exception as e:
        print(f"Casting failed: {str(e)}")


if __name__ == "__main__":
    main()
