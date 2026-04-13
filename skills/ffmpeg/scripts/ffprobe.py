import sys
import json
import subprocess
import shlex


def main() -> None:
    if len(sys.argv) < 2:
        return

    try:
        args_json = json.loads(sys.argv[1])
        cmd_args = args_json.get("args", "")

        cmd = ["ffprobe"] + shlex.split(cmd_args)

        result = subprocess.run(cmd, capture_output=True, text=True)

        if result.stdout:
            print(result.stdout)
        if result.stderr:
            print(result.stderr)

    except Exception as e:
        print(f"Error: {str(e)}")


if __name__ == "__main__":
    main()
