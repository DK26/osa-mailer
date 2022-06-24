import base64
import os
import json
import zlib  # https://en.wikipedia.org/wiki/Zlib
from time import time_ns
from time import time
from datetime import datetime
from typing import Optional


def json_to_b64(data_json: dict) -> bytes:
    data_bytes = bytes(json.dumps(data_json, separators=(',', ':')), "utf-8")
    compressed = zlib.compress(data_bytes, level=9)  # Gzip -- Python 2.7: `zlib.compress(data_bytes, 9)`
    return base64.b64encode(compressed)


def b64_to_json(data_b64: bytes) -> dict:
    compressed = base64.b64decode(data_b64)
    data_bytes = zlib.decompress(compressed)  # Gzip
    return json.loads(data_bytes)


def write_to_file(entry: dict) -> Optional[str]:

    # Generate E-Mail ID for the given entry to identify the E-mail it belongs to
    email = entry.get("email")
    email_bytes = bytes(json.dumps(email, separators=(',', ':')), "utf-8")

    # E-mail ID
    eid = hex(zlib.crc32(email_bytes))[2:]

    # Nanoseconds timestamp to prevent duplications and use to order by
    ts = hex(int(time_ns() / 100))[2:]

    # Entry Unique ID value to farther insure no duplications
    enid = entry.get("id")

    # Create a JSON structured string from the `entry` dictionary
    json_content = bytes(json.dumps(entry, indent=4), "utf-8")

    # Entry integrity
    crc = hex(zlib.crc32(json_content))[2:]

    filename = f"{eid}.{ts}.{enid}.{crc}.json"

    with open(filename, 'wb') as fs:
        fs.write(json_content)
        return filename


def new_uid() -> str:
    uid_base = base64.b64encode(os.urandom(2)) + bytes(str(time()), "utf-8")
    uid = hex(zlib.crc32(uid_base))[2:]

    # Python 2.7
    # uid = hex(zlib.crc32(uid_base) & 0xffffffff)[2:-1]

    return uid


def main():
    email_entry = {

        # Unique ID of the entry
        "id": new_uid(),

        # UTC ISO 8601
        "utc": datetime.utcnow().isoformat(),

        # E-mail addresses to notify in case of error
        "notify_error": ["Developers <dev-team@somemail.com>"],

        # E-mail header from which a unique E-mail ID is constructed to associated E-mail entries
        "email": {

            # Name of the external system that produced this entry
            "system": "MyExternalSystem",

            # Name of the subsystem that produced this entry
            "subsystem": "[ID:12345] Trigger: Server Disk Out-of-Space",

            # E-mail header
            "from": "Mail System <some@email.com>",
            "to": ["Some One <someone@somemail.com>"],
            "cc": [],
            "bcc": [],
            "reply_to": [
                "System Admin <admin@somemail.com>",
                "Project Lead <lead@somemail.com>"
            ],
            "subject": "Warning: Your server's disk is out-of-space",
            "template": "ops_department",  # Name of the Template.
            "alternative_content": "Unable to render HTML. Please refer to the Ops department for details.",
            "attachments": [
                "guides/disk-capacity-guidelines.pdf"
            ]

        },

        # Template variables
        "template": {
            "title": "Detected Problems in Your Server",
            "message": "We have detected a disk capacity problem with one of your servers. Please refer to the instructions below",
            "details": {
                "Hostname": "MailServer01",
                "IP Address": "192.168.0.1",
                "Disk Capacity Percentage": 95
            },
            "instructions": [
                "Remove unused software",
                "Delete temporary files",
                "Use a drive-cleaner application",
                "Add additional hard-drive"
            ],
            "motd": "We are very excited to inform you about our new project that allows you to time-travel. Please refer the web-site below to find out more"
        }
    }

    b64 = json_to_b64(email_entry)

    b64_len = len(b64)
    print(f"B64 Length: {len(b64)}")

    # noinspection Assert
    # https://docs.microsoft.com/en-us/troubleshoot/windows-client/shell-experience/command-line-string-limitation
    assert b64_len <= 8191

    restored_entry = b64_to_json(b64)

    # noinspection Assert
    assert email_entry == restored_entry

    write_to_file(restored_entry)


if __name__ == '__main__':
    main()
