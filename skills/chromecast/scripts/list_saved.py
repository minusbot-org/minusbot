import json
import os


def main() -> None:
    fav_path = "/data/favorite.json"
    if os.path.exists(fav_path):
        with open(fav_path, "r") as f:
            print(json.dumps(json.load(f), indent=2))
    else:
        print("No favorite device saved.")


if __name__ == "__main__":
    main()
