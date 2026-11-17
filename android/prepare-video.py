"""Transcode the supplied game's MPEG-1 OP for Android, without changing the original file.

The prepared H.264/AAC MP4 is played directly from the APK, not extracted at first launch.
FFmpeg is a build-time dependency only; it is not embedded in the Android application.
"""
# SPDX-License-Identifier: GPL-3.0-only
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess


def digest(path):
    result = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            result.update(chunk)
    return result.hexdigest()


def ffmpeg_default():
    found = shutil.which("ffmpeg")
    if found:
        return found
    try:
        import imageio_ffmpeg
        return imageio_ffmpeg.get_ffmpeg_exe()
    except ImportError:
        return None


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("game", type=Path)
    parser.add_argument("output", type=Path, help="Build-only APK assets directory")
    parser.add_argument("--ffmpeg", default=ffmpeg_default())
    arguments = parser.parse_args()
    game = arguments.game.resolve(strict=True)
    output = arguments.output.resolve()
    if output == game or game in output.parents:
        parser.error("Output must be outside the original game directory")
    if not arguments.ffmpeg:
        parser.error("FFmpeg is required; pass --ffmpeg or install imageio-ffmpeg")
    original = game / "avi" / "lave_op.mpg"
    original_hash = digest(original)
    manifest_path = output / "movies.json"
    destination = output / "movies" / "lave_op.mp4"
    if manifest_path.is_file() and destination.is_file():
        previous = json.loads(manifest_path.read_text(encoding="utf-8"))
        for movie in previous.get("movies", []):
            if (movie.get("sourceSha256") == original_hash
                    and movie.get("asset") == "movies/lave_op.mp4"
                    and movie.get("encodedSize") == destination.stat().st_size
                    and movie.get("encodedSha256") == digest(destination)):
                print("Verified cached Android OP: " + str(destination), flush=True)
                return
    destination.parent.mkdir(parents=True, exist_ok=True)
    temporary = destination.with_suffix(".part.mp4")
    try:
        subprocess.run([
            arguments.ffmpeg, "-hide_banner", "-nostdin", "-y", "-i", str(original),
            "-map", "0:v:0", "-map", "0:a:0", "-c:v", "libx264", "-preset", "fast",
            "-crf", "18", "-profile:v", "baseline", "-level", "3.1", "-pix_fmt", "yuv420p",
            "-c:a", "aac", "-b:a", "192k", "-ar", "48000", "-ac", "2",
            "-movflags", "+faststart", "-threads", "2", str(temporary)
        ], check=True)
        # Decode every video/audio packet, not just the MP4 header or its first frame.
        subprocess.run([arguments.ffmpeg, "-v", "error", "-nostdin", "-i", str(temporary),
                        "-map", "0:v:0", "-map", "0:a:0", "-f", "null", "-"], check=True)
        if digest(original) != original_hash:
            raise ValueError("Original OP changed while transcoding")
        movie = {
            "resource": "avi\\lave_op.mpg", "asset": "movies/lave_op.mp4",
            "sourceSize": original.stat().st_size, "sourceSha256": original_hash,
            "encodedSize": temporary.stat().st_size, "encodedSha256": digest(temporary),
            "videoCodec": "H.264 Baseline Level 3.1", "audioCodec": "AAC-LC 192k stereo",
            "quality": "CRF 18", "width": 800, "height": 600,
        }
        temporary.replace(destination)
        manifest_temp = manifest_path.with_suffix(".json.part")
        with manifest_temp.open("w", encoding="utf-8", newline="\n") as target:
            json.dump({"version": 1, "movies": [movie]}, target, ensure_ascii=False, indent=2)
            target.write("\n")
            target.flush()
            os.fsync(target.fileno())
        manifest_temp.replace(manifest_path)
        print("Prepared Android OP: " + str(destination), flush=True)
    finally:
        temporary.unlink(missing_ok=True)


if __name__ == "__main__":
    main()
