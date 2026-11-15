"""Losslessly prepare the supplied Loverable archives for a self-contained APK."""

import argparse
from concurrent.futures import ThreadPoolExecutor, as_completed
import hashlib
import json
import lzma
import os
from pathlib import Path
import re


CHUNK_SIZE = 1024 * 1024
PRESET = 3


def sha256_file(path):
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(CHUNK_SIZE), b""):
            digest.update(chunk)
    return digest.hexdigest()


def prepare_archive(source, output, previous):
    original_size = source.stat().st_size
    original_hash = sha256_file(source)
    asset = "bundled/" + source.name + ".xz"
    destination = output / asset
    if (
        previous
        and previous.get("size") == original_size
        and previous.get("sha256") == original_hash
        and previous.get("asset") == asset
        and previous.get("encoding") == "xz"
        and destination.is_file()
        and destination.stat().st_size == previous.get("encodedSize")
        and sha256_file(destination) == previous.get("encodedSha256")
    ):
        print("Verified cached " + source.name, flush=True)
        return previous

    temporary = destination.with_suffix(destination.suffix + ".part")
    encoder = lzma.LZMACompressor(
        format=lzma.FORMAT_XZ, check=lzma.CHECK_CRC64, preset=PRESET
    )
    encoded_hash = hashlib.sha256()
    read_hash = hashlib.sha256()
    try:
        with source.open("rb") as archive, temporary.open("wb") as target:
            for chunk in iter(lambda: archive.read(CHUNK_SIZE), b""):
                read_hash.update(chunk)
                encoded = encoder.compress(chunk)
                encoded_hash.update(encoded)
                target.write(encoded)
            encoded = encoder.flush()
            encoded_hash.update(encoded)
            target.write(encoded)
            target.flush()
            os.fsync(target.fileno())
        if read_hash.hexdigest() != original_hash:
            raise ValueError("Original archive changed while packaging " + source.name)
        restored_hash = hashlib.sha256()
        restored_size = 0
        with lzma.open(temporary, "rb") as restored:
            for chunk in iter(lambda: restored.read(CHUNK_SIZE), b""):
                restored_hash.update(chunk)
                restored_size += len(chunk)
        if restored_size != original_size or restored_hash.hexdigest() != original_hash:
            raise ValueError("XZ verification failed for " + source.name)
        temporary.replace(destination)
    finally:
        temporary.unlink(missing_ok=True)
    print("Prepared " + source.name, flush=True)
    return {
        "name": source.name,
        "size": original_size,
        "sha256": original_hash,
        "encoding": "xz",
        "asset": asset,
        "encodedSize": destination.stat().st_size,
        "encodedSha256": encoded_hash.hexdigest(),
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("game", type=Path, help="Original directory containing data*.ykc")
    parser.add_argument("output", type=Path, help="Build-only assets directory outside the original game")
    arguments = parser.parse_args()
    game = arguments.game.resolve(strict=True)
    output = arguments.output.resolve()
    if output == game or game in output.parents:
        parser.error("Build output must be outside the original game directory")
    archives = sorted(
        path for path in game.iterdir()
        if path.is_file() and re.fullmatch(r"data[0-9]+\.ykc", path.name, re.IGNORECASE)
    )
    if not archives:
        parser.error("Original game directory contains no data*.ykc archives")
    if len({path.name.lower() for path in archives}) != len(archives):
        parser.error("Archive names differ only in letter case")
    (output / "bundled").mkdir(parents=True, exist_ok=True)
    manifest_path = output / "bundled-game.json"
    previous = {}
    if manifest_path.is_file():
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
        if manifest.get("id") == "loverable-cn" and manifest.get("preset") == PRESET:
            previous = {item["name"]: item for item in manifest.get("files", [])}
    files = []
    with ThreadPoolExecutor(max_workers=min(4, len(archives))) as executor:
        jobs = [executor.submit(prepare_archive, path, output, previous.get(path.name)) for path in archives]
        for job in as_completed(jobs):
            files.append(job.result())
    files.sort(key=lambda item: item["name"])
    if sum(item["encodedSize"] for item in files) > (1 << 32) - 32 * 1024 * 1024:
        raise ValueError("Compressed archives exceed the ZIP32 APK size budget")
    manifest = {"name": "情侣热恋中", "id": "loverable-cn", "preset": PRESET, "files": files}
    temporary = manifest_path.with_suffix(".json.part")
    with temporary.open("w", encoding="utf-8", newline="\n") as target:
        json.dump(manifest, target, ensure_ascii=False, indent=2)
        target.write("\n")
        target.flush()
        os.fsync(target.fileno())
    temporary.replace(manifest_path)
    print("Bundle ready at " + str(manifest_path), flush=True)


if __name__ == "__main__":
    main()
