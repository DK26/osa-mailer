import json
import zlib  # https://en.wikipedia.org/wiki/Zlib
import base64
import os
import datetime
from time import time


def json_to_b64(data_json: dict) -> bytes:
    data_bytes = bytes(json.dumps(data_json, separators=(',', ':')), "utf-8")
    compressed = zlib.compress(data_bytes, level=9)  # Gzip -- Python 2.7: `zlib.compress(data_bytes, 9)`
    return base64.b64encode(compressed)


def new_uid() -> str:
    uid_base = base64.b64encode(os.urandom(2)) + bytes(str(time()), "utf-8")
    uid = hex(zlib.crc32(uid_base))[2:]

    # Python 2.7
    # uid = hex(zlib.crc32(uid_base) & 0xffffffff)[2:-1]

    return uid


def main():
    email_request = {

        # Data related to the E-mail Send Request #

        # Unique ID of the E-mail Send Request
        "id": new_uid(),

        # UTC ISO 8601 RFC3339
        "utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),

        # E-mail addresses to notify in case of an error
        "notify_error": ["Developers <dev-team@somemail.com>"],

        # E-mail header from which a unique E-mail ID is constructed to associate
        # the E-mail send requests
        "email": {

            # Name of the external system that produced this entry
            "system": "MyExternalSystem",

            # Name of the subsystem that produced this entry
            "subsystem": "[ID:12345] Trigger: Server Disk Out-of-Space",

            # E-mail header
            "from": "Mail System <tech-support@somemail.com>",
            "to": ["Rick S. <someone@somemail.com>"],
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
            ],

            # Provide additional optional E-mail identifiers for the unique E-mail ID calculation
            "unique_by": ""

        },

        # Template context variables
        "context": {
            "message": {
                "head": "Detected Problems in Your Server",
                "body": "We have detected a disk capacity problem with one or more of your servers."
                        " Please refer to the instructions below"
            },
            "table": {
                # `type` indicates the type of the table, which is used by the template engine to render it.
                "type": 1,

                # `+` leading sign indicates the server to `accumulate` this key element's contents.
                # `+` expands `entries` to: { "idx": N, "items": [ { <ENTRY 1> }, { <ENTRY 2> }, ... , { <ENTRY N> } ] }
                "+entries": [
                    {
                        "pos": 1,  # In case the order gets mixed, so we will have the means to know hwo to re-order it
                        "label": "Hostname",
                        "value": "MailServer01"
                    },
                    {
                        "pos": 2,
                        "label": "IP Address",
                        "value": "192.168.0.1"
                    },
                    {
                        "pos": 3,
                        "label": "Disk Capacity Percentage",
                        "value": 95
                    }
                ]
            },
            "instructions": [
                "Remove unused software",
                "Delete temporary files",
                "Use a drive-cleaner application",
                "Add additional hard-drive"
            ],
            "motd": "We are very excited to inform you about our new project that allows you to time-travel. "
                    "Please refer the web-site below to find out more"
        }
    }

    # Encoded Sample
    print(str(json_to_b64(email_request), "utf-8"))

    # Decoded Sample
    # print(json.dumps(email_request, separators=(',', ':')))

    # Debug
    # print("length: {}".format(len(b64)))


if __name__ == '__main__':
    main()
