import json
import sys
import os
import urllib.request
import urllib.parse
from typing import Any, Optional


def get_config(key: str, default: Optional[Any] = None) -> Optional[str]:
    """Get configuration from environment variables."""
    return os.environ.get(f"CONFIG_{key}", default)


def run() -> None:
    if len(sys.argv) < 2:
        print("Error: No inputs provided")
        return

    try:
        inputs = json.loads(sys.argv[1])
    except json.JSONDecodeError:
        print("Error: Invalid JSON input")
        return

    query = inputs.get("query")
    if not query:
        print("Error: No query provided")
        return

    api_key = os.environ.get("API_KEY")
    if not api_key:
        print(
            "Error: API_KEY not found in environment. Please configure it in the vault."
        )
        return

    params: dict[str, Any] = {
        "engine": get_config("engine", "google"),
        "q": query,
        "api_key": api_key,
        "google_domain": get_config("google_domain", "google.com"),
        "gl": get_config("gl", "us"),
        "hl": get_config("hl", "en"),
    }

    location = get_config("location")
    if location:
        params["location"] = location

    url = "https://serpapi.com/search?" + urllib.parse.urlencode(params)

    try:
        with urllib.request.urlopen(url) as response:
            data = json.loads(response.read().decode())

            if "error" in data:
                print(f"Search error: {data['error']}")
                return

            if "organic_results" in data:
                results: list[str] = []
                for r in data["organic_results"][:5]:
                    title = r.get("title", "No Title")
                    link = r.get("link", "#")
                    snippet = r.get("snippet", "")
                    results.append(f"[{title}]({link}): {snippet}")

                output = "\n\n".join(results)
                print(output if output else "No results found.")
            else:
                print("No organic results found.")

    except Exception as e:
        print(f"Error performing search: {str(e)}")


if __name__ == "__main__":
    run()
