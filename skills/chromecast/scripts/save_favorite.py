import sys
import json
import os


def main() -> None:
    try:
        data = json.loads(sys.argv[1])
        name = data.get("friendly_name")

        if not name:
            print("Missing friendly_name")
            return

        conf_dir = "/data"
        if not os.path.exists(conf_dir):
            os.makedirs(conf_dir)

        fav_path = os.path.join(conf_dir, "favorite.json")
        with open(fav_path, "w") as f:
            json.dump({"friendly_name": name}, f)

        print(f"Device '{name}' saved as favorite.")
    except Exception as e:
        print(f"Error: {str(e)}")


if __name__ == "__main__":
    main()
