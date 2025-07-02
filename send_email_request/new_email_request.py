import sys
import argparse
import os
import base64
import zlib  # https://en.wikipedia.org/wiki/Zlib
import json
from time import time_ns
from typing import Optional
from pathlib import Path


def b64_to_json(data_b64: bytes) -> dict:
    compressed = base64.b64decode(data_b64)
    data_bytes = zlib.decompress(compressed)  # Gzip
    return json.loads(data_bytes)


def write_to_file(email_request: dict, output_dir: str) -> Optional[str]:

    # Generate E-Mail ID for the given email request
    # to identify the E-mail it belongs to
    email = email_request.get("email")
    email_bytes = bytes(json.dumps(email, separators=(',', ':')), "utf-8")

    # E-mail ID
    eid = hex(zlib.crc32(email_bytes))[2:]

    # Nanoseconds timestamp to prevent duplications and use to order by
    ts = hex(int(time_ns() / 100))[2:]

    # Email Request Unique ID value to farther insure no duplications
    eruid = email_request.get("id")

    # Create a JSON structured string from the `email_request` dictionary
    json_content = bytes(json.dumps(email_request, indent=4, sort_keys=True), "utf-8")

    # Request integrity
    crc = hex(zlib.crc32(json_content))[2:]

    filename = os.path.join(output_dir, f"{eid}.{ts}.{eruid}.{crc}.json")

    Path(output_dir).mkdir(parents=True, exist_ok=True)

    with open(filename, 'wb') as fs:
        fs.write(json_content)
        print(f"Output: {filename}")
        return filename


def write_request(request: str, output_dir: str):

    request = request.strip()

    if request.startswith('{'):
        request = json.loads(request)
    else:
        request = b64_to_json(bytes(request, "utf-8"))

    write_to_file(request, output_dir)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "-r",
        "--request",
        help="An E-mail Send Request compressed by zlib, encoded in base64.",
        type=str
    )

    parser.add_argument(
        "-o",
        "--output",
        help="Output directory for E-mail Send Requests.",
        type=str,
        default="."
    )
    args = parser.parse_args()

    if isinstance(args.request, str):
        write_request(args.request, args.output)
    else:
        # STDIN stream feature just in case there is a char length limit in the runtime
        # environment.
        # https://docs.microsoft.com/en-us/troubleshoot/windows-client/shell-experience/command-line-string-limitation
        for email_send_request in sys.stdin:
            write_request(email_send_request, args.output)


if __name__ == "__main__":
    main()
