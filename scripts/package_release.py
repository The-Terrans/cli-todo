"""Package checked release binaries with a hash-pinned, prebuilt Nix flake."""
import argparse
import base64
import hashlib
import json
from pathlib import Path
import re
import shutil
import tarfile
import tomllib

ROOT = Path(__file__).resolve().parents[1]
TARGETS = {
    "x86_64-linux": "x86_64-unknown-linux-musl",
    "aarch64-linux": "aarch64-unknown-linux-musl",
    "x86_64-darwin": "x86_64-apple-darwin",
    "aarch64-darwin": "aarch64-apple-darwin",
}


def validate_release(tag, repository):
    version = tomllib.loads((ROOT / "Cargo.toml").read_text())["package"]["version"]
    if not re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?", version):
        raise ValueError("Cargo version must be a release or prerelease version")
    if tag != f"v{version}":
        raise ValueError(f"Release tag must match Cargo.toml: v{version}")
    if not re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", repository) or any(part in (".", "..") for part in repository.split("/")):
        raise ValueError("Repository must be OWNER/REPO")
    return version


def digest(path):
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").digest()


def package_release(tag, repository, artifacts, output):
    version = validate_release(tag, repository)
    sources = {
        system: artifacts / f"cli-todo-{tag}-{target}.tar.gz"
        for system, target in TARGETS.items()
    }
    for source in sources.values():
        with tarfile.open(source, "r:gz") as archive:
            members = archive.getmembers()
            if len(members) != 1 or members[0].name != "cli-todo" or not members[0].isfile() or not members[0].mode & 0o111:
                raise ValueError(f"Expected one executable named cli-todo: {source.name}")
    output.mkdir(parents=True, exist_ok=True)
    metadata = {"repository": repository, "tag": tag, "version": version, "artifacts": {}}
    for system, source in sources.items():
        destination = output / source.name
        if source.resolve() != destination.resolve():
            shutil.copyfile(source, destination)
        metadata["artifacts"][system] = {
            "file": source.name,
            "hash": "sha256-" + base64.b64encode(digest(destination)).decode("ascii"),
        }
    flake = output / "cli-todo-flake"
    flake.mkdir(exist_ok=True)
    shutil.copyfile(ROOT / ".nix/release/flake.nix", flake / "flake.nix")
    shutil.copyfile(ROOT / ".nix/release/flake.lock", flake / "flake.lock")
    (flake / "release.json").write_text(json.dumps(metadata, indent=2) + "\n")
    flake_archive = output / "cli-todo-flake.tar.gz"
    with tarfile.open(flake_archive, "w:gz") as archive:
        archive.add(flake, arcname="cli-todo-flake")
    assets = [output / source.name for source in sources.values()] + [flake_archive]
    (output / "SHA256SUMS").write_text("".join(f"{digest(asset).hex()}  {asset.name}\n" for asset in sorted(assets)))
    return flake


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tag", required=True)
    parser.add_argument("--repository", required=True)
    parser.add_argument("--artifacts", type=Path, default=Path("dist"))
    parser.add_argument("--output", type=Path, default=Path("dist"))
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    if args.check:
        validate_release(args.tag, args.repository)
    else:
        package_release(args.tag, args.repository, args.artifacts, args.output)


if __name__ == "__main__":
    main()
