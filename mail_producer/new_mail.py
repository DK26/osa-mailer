import base64
import os
import json
import zlib  # https://en.wikipedia.org/wiki/Zlib
from time import time_ns


def json_to_b64(data_json: dict) -> bytes:
    data_bytes = bytes(json.dumps(data_json, separators=(',', ':')), "utf-8")
    compressed = zlib.compress(data_bytes, level=9)  # Gzip
    return base64.b64encode(compressed)


def b64_to_json(data_b64: bytes) -> dict:
    compressed = base64.b64decode(data_b64)
    data_bytes = zlib.decompress(compressed)  # Gzip
    return json.loads(data_bytes)


def generate_file_name(header: dict) -> str:

    header_bytes = bytes(json.dumps(header, separators=(',', ':')), "utf-8")
    header_crc = hex(zlib.crc32(header_bytes))[2:]
    ts = time_ns()  # To prevent duplications + Order by value
    rnd = str(base64.b64encode(os.urandom(2)), "utf-8")  # To farther insure no duplications

    return f"{header_crc}.{ts}.{rnd}.json"


def main():
    mail_entry = {
        "notify_error": ["Developers <dev-team@somemail.com>"],  # Notify in case of error
        "header": {
            "from": "Mail System <some@email.com>",
            "to": ["Some One <someone@somemail.com>"],
            "cc": [],
            "bcc": [],
            "reply_to": [],
            "template": "test",  # Represents the name of the Template.
            "alternative_content": "Unable to present template",
            "attachments": []
        },
        "body": {  # Template variables
            "hello": "world",
            "some_values": [1, 2, 3, 4],
            "table": {
                "x": 0,
                "y": 0
            }
        }
    }

    b64 = json_to_b64(mail_entry)
    restored_entry = b64_to_json(b64)

    # noinspection Assert
    assert mail_entry == restored_entry

    filename = generate_file_name(restored_entry.get('header'))

    with open(filename, 'w', encoding='utf-8') as fs:
        json.dump(restored_entry, fs, indent=4)


if __name__ == '__main__':
    main()
