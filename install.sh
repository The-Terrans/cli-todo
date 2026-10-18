#!/bin/sh
# Install or update the GitHub release binary; never modifies task data.
set -eu

fail() { printf 'Error: %s\n' "$*" >&2; exit 1; }

fetch() {
    curl --fail --silent --show-error --location --proto '=https' --proto-redir '=https' \
        --connect-timeout 15 --max-time 300 --retry 2 "$@"
}

cleanup() {
    [ -z "$work" ] || rm -rf "$work"
    [ -z "$stage" ] || rm -f "$stage"
}

main() {
    tag=latest
    directory=${CLI_TODO_INSTALL_DIR:-${HOME:?HOME must be set}/.local/bin}
    while [ "$#" -gt 0 ]; do
        case "$1" in
            --version) [ "$#" -ge 2 ] || fail '--version needs an exact release tag'; tag=$2; shift 2 ;;
            --dir) [ "$#" -ge 2 ] || fail '--dir needs a directory'; directory=$2; shift 2 ;;
            -h|--help)
                printf '%s\n' 'Usage: sh install.sh [--version TAG] [--dir DIRECTORY]' \
                    'Installs or updates cli-todo. Default: latest release in ~/.local/bin.' \
                    'Use the exact tag (for example 0.1.0 or v0.1.0). No sudo required.'
                return ;;
            *) fail "Unknown option: $1" ;;
        esac
    done
    case "$directory" in /*) ;; *) fail 'Installation directory must be an absolute path' ;; esac
    for tool in curl uname tar awk mktemp chmod mv rm mkdir; do
        command -v "$tool" >/dev/null 2>&1 || fail "Required command not found: $tool"
    done
    if command -v sha256sum >/dev/null 2>&1; then
        hash_tool=sha256sum
    elif command -v shasum >/dev/null 2>&1; then
        hash_tool=shasum
    else
        fail 'Install sha256sum or shasum to verify the download'
    fi
    case "$(uname -s)" in
        Linux) platform=unknown-linux-musl ;;
        Darwin) platform=apple-darwin ;;
        *) fail 'Supported systems: Linux and macOS' ;;
    esac
    case "$(uname -m)" in
        x86_64|amd64) arch=x86_64 ;;
        aarch64|arm64) arch=aarch64 ;;
        *) fail 'Supported architectures: x86_64 and ARM64' ;;
    esac
    destination=$directory/cli-todo
    [ ! -L "$destination" ] || fail "Refusing to replace symlink $destination; update Nix/Home Manager installations through Nix"
    [ ! -e "$destination" ] || [ -f "$destination" ] || fail "$destination is not a regular file"
    base=https://github.com/The-Terrans/cli-todo/releases
    if [ "$tag" = latest ]; then
        resolved=$(fetch --output /dev/null --write-out '%{url_effective}' "$base/latest")
        case "$resolved" in
            "$base/tag/"*) tag=${resolved#"$base/tag/"} ;;
            *) fail 'Could not resolve the latest release tag' ;;
        esac
    fi
    case "$tag" in ''|*[!a-zA-Z0-9._-]*|.|..) fail 'Release tag must contain only letters, numbers, dots, underscores, or hyphens' ;; esac
    asset=cli-todo-$tag-$arch-$platform.tar.gz
    work=$(mktemp -d)
    printf 'Downloading cli-todo %s (%s-%s)...\n' "$tag" "$arch" "$platform"
    fetch --output "$work/SHA256SUMS" "$base/download/$tag/SHA256SUMS"
    fetch --output "$work/archive.tar.gz" "$base/download/$tag/$asset"
    expected=$(awk -v name="$asset" '$2 == name || $2 == "./" name { print $1; count++ } END { if (count != 1) exit 1 }' "$work/SHA256SUMS") || fail 'Missing or duplicate archive checksum'
    case "$expected" in ''|*[!a-fA-F0-9]*) fail 'Invalid SHA-256 checksum' ;; esac
    [ "${#expected}" -eq 64 ] || fail 'Invalid SHA-256 checksum length'
    if [ "$hash_tool" = sha256sum ]; then
        actual=$(sha256sum "$work/archive.tar.gz")
    else
        actual=$(shasum -a 256 "$work/archive.tar.gz")
    fi
    actual=${actual%% *}
    [ "$actual" = "$expected" ] || fail 'Checksum mismatch; existing installation was not changed'
    [ "$(tar -tzf "$work/archive.tar.gz")" = cli-todo ] || fail 'Unexpected archive contents'
    case "$(tar -tvzf "$work/archive.tar.gz")" in
        -*) ;; *) fail 'Archive must contain a regular binary, not a link' ;;
    esac
    mkdir -p "$directory"
    stage=$(mktemp "$directory/.cli-todo.XXXXXXXX")
    tar -xOzf "$work/archive.tar.gz" cli-todo > "$stage"
    [ -s "$stage" ] || fail 'Downloaded binary is empty'
    chmod 755 "$stage"
    # Same-directory rename atomically replaces an old binary, even if it is running.
    mv -f "$stage" "$destination"
    stage=
    printf 'Installed cli-todo %s to %s\n' "$tag" "$destination"
    printf '%s\n' 'Run this installer again to update. Task data is unchanged.'
    case ":${PATH:-}:" in
        *":$directory:"*) ;;
        *) printf 'Add this directory to your PATH: %s\n' "$directory" ;;
    esac
    found=$(command -v cli-todo || true)
    if [ -n "$found" ] && [ "$found" != "$destination" ]; then
        printf 'PATH currently selects %s instead; run %s or adjust PATH.\n' "$found" "$destination"
    fi
}

work=
stage=
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
main "$@"
