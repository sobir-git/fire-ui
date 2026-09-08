#!/usr/bin/env python3
"""Inspect or operate an opt-in Fire UI process through its local Unix socket."""
import argparse
import json
import socket
import sys


def request(path, payload):
    with socket.socket(socket.AF_UNIX) as connection:
        connection.settimeout(5)
        connection.connect(str(path))
        connection.sendall(json.dumps(payload, ensure_ascii=False).encode() + b"\n")
        with connection.makefile("r", encoding="utf-8") as response:
            return json.loads(response.readline())


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("socket")
    parser.add_argument("request", nargs="?", default="{}", help='JSON request; omit to inspect, or use - for stdin')
    args = parser.parse_args()
    try:
        payload = json.loads(sys.stdin.read() if args.request == "-" else args.request)
        result = request(args.socket, payload)
        print(json.dumps(result, ensure_ascii=False, indent=2))
        return int("error" in result)
    except (OSError, ValueError) as error:
        print(str(error), file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
