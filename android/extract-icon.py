"""Extract an EXE's largest icon into a build-only Android resource directory.

Requires pefile and Pillow. The executable is read as PE data and never launched.
Original game artwork belongs in ignored build output, not in the source tree.
"""

# SPDX-License-Identifier: GPL-3.0-only
import argparse
import hashlib
from io import BytesIO
import json
from pathlib import Path
import struct

import pefile
from PIL import Image


def extract_icon(executable, output, resource_id=None):
    source = executable.read_bytes()
    pe = pefile.PE(data=source, fast_load=True)
    pe.parse_data_directories(
        directories=[pefile.DIRECTORY_ENTRY["IMAGE_DIRECTORY_ENTRY_RESOURCE"]]
    )
    if not hasattr(pe, "DIRECTORY_ENTRY_RESOURCE"):
        raise ValueError("The executable contains no PE resources")
    resources = {entry.id: entry for entry in pe.DIRECTORY_ENTRY_RESOURCE.entries}
    if 3 not in resources or 14 not in resources:
        raise ValueError("The executable contains no icon or icon group resources")

    # Keep every language variant; group and icon IDs alone need not be unique.
    icons = {}
    for icon in resources[3].directory.entries:
        for language in icon.directory.entries:
            record = language.data.struct
            icons[(icon.id, language.id)] = pe.get_data(record.OffsetToData, record.Size)

    candidates = []
    for group in resources[14].directory.entries:
        for language in group.directory.entries:
            record = language.data.struct
            data = pe.get_data(record.OffsetToData, record.Size)
            if len(data) < 6:
                raise ValueError("Truncated icon group")
            reserved, kind, count = struct.unpack_from("<HHH", data)
            if reserved or kind != 1 or len(data) < 6 + count * 14:
                raise ValueError("Invalid icon group")
            for index in range(count):
                entry = data[6 + index * 14:20 + index * 14]
                width, height, _, _, _, depth, size, identifier = struct.unpack("<BBBBHHIH", entry)
                if resource_id is not None and identifier != resource_id:
                    continue
                raw = icons.get((identifier, language.id))
                if raw is None:
                    raw = next((value for (resource_id, _), value in icons.items()
                                if resource_id == identifier), None)
                if raw is None:
                    raise ValueError("Missing or truncated icon bitmap")
                dimensions = (width or 256, height or 256)
                pixels = dimensions[0] * dimensions[1]
                # Some PE icon groups retain a 32-bit DIB size after replacing
                # the resource with a PNG. The PE record supplies its real size.
                png = raw.startswith(b"\x89PNG\r\n\x1a\n")
                if len(raw) != size and not (png and depth == 32 and size == 40 + pixels * 4):
                    raise ValueError("Missing or truncated icon bitmap")
                ico_entry = entry[:8] + struct.pack("<I", len(raw))
                candidates.append((pixels, depth, ico_entry, raw, group.id, language.id, dimensions, identifier))
    if not candidates:
        raise ValueError("The executable's icon groups are empty")
    # Stable sorting retains the first PE resource/language for equal size/depth.
    _, _, entry, raw, group, language, dimensions, identifier = max(candidates, key=lambda icon: icon[:2])
    ico = struct.pack("<HHH", 0, 1, 1) + entry + struct.pack("<I", 22) + raw
    with Image.open(BytesIO(ico)) as decoded:
        if decoded.size != dimensions:
            raise ValueError("Icon dimensions do not match the PE icon group")
        image = decoded.convert("RGBA")
    output.mkdir(parents=True, exist_ok=True)
    destination = output / "drawable-nodpi" / "ic_game_icon.png"
    destination.parent.mkdir(parents=True, exist_ok=True)
    image.save(destination, format="PNG")
    metadata = {
        "source": executable.name,
        "sourceSha256": hashlib.sha256(source).hexdigest(),
        "group": group,
        "resourceId": identifier,
        "language": language,
        "width": image.width,
        "height": image.height,
        "rgbaSha256": hashlib.sha256(image.tobytes()).hexdigest(),
    }
    (output.parent / "icon-provenance.json").write_text(
        json.dumps(metadata, indent=2) + "\n", encoding="utf-8"
    )
    print(f"Extracted {image.width}x{image.height} EXE icon to {destination}")
    return metadata


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("executable", type=Path)
    parser.add_argument("output", type=Path, help="Build-only Android res directory")
    parser.add_argument("--resource-id", type=int, help="Select a specific PE icon bitmap resource")
    arguments = parser.parse_args()
    executable = arguments.executable.resolve(strict=True)
    output = arguments.output.resolve()
    if output == executable.parent or executable.parent in output.parents:
        parser.error("Output must be outside the original game directory")
    extract_icon(executable, output, arguments.resource_id)


if __name__ == "__main__":
    main()
