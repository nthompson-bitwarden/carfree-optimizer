#!/usr/bin/env bash
# E01-S02: start the local workbench and its workspace API.
set -euo pipefail

if [[ ${1:-} == --help || ${1:-} == -h ]]; then
    cat <<'EOF'
Usage: ./dev.sh [port]

Start the workbench and workspace API on 127.0.0.1. The default port is
8047. Pass 0 for an available port. Press Ctrl-C to stop the server.
Set DEV_WORKSPACE_DIR to use a store other than .dev/workspace.
EOF
    exit 0
fi

port=${1-8047}
if [[ $# -gt 1 || ! $port =~ ^[0-9]{1,5}$ ]] || (( 10#$port > 65535 )); then
    echo 'Usage: ./dev.sh [port from 0 to 65535]' >&2
    exit 2
fi

cd "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
for dependency in rustup python3 curl "${PROTOC:-protoc}"; do
    if ! command -v "$dependency" >/dev/null 2>&1; then
        printf 'Missing %s. See docs/DEVELOPMENT.md for prerequisites.\n' "$dependency" >&2
        exit 1
    fi
done

toolchain=$(python3 - <<'PY'
try:
    import tomllib
except ModuleNotFoundError:
    raise SystemExit("Python 3.11 or newer is required.")

with open("rust-toolchain.toml", "rb") as config:
    print(tomllib.load(config)["toolchain"]["channel"])
PY
)
if ! rustup run "$toolchain" rustc --version >/dev/null; then
    printf 'Install the pinned toolchain with: rustup toolchain install %s --profile minimal\n' "$toolchain" >&2
    exit 1
fi

workspace_dir=${DEV_WORKSPACE_DIR:-.dev/workspace}
if [[ $workspace_dir != /* ]]; then
    workspace_dir="$PWD/$workspace_dir"
fi
owner_file="$workspace_dir/owner"

umask 077
mkdir -p -- "$workspace_dir"
python3 - "$owner_file" <<'PY'
import os
import secrets
import sys
from pathlib import Path
from tempfile import NamedTemporaryFile

owner = Path(sys.argv[1])
if not owner.exists():
    if (owner.parent / "workspace-v1.json").exists():
        raise SystemExit(f"Restore the owner credential at {owner} before opening this workspace.")
    # Publish a complete credential atomically, even if two launchers start together.
    with NamedTemporaryFile(dir=owner.parent) as token:
        token.write(secrets.token_hex(32).encode("ascii") + b"\n")
        token.flush()
        try:
            os.link(token.name, owner)
        except FileExistsError:
            pass
PY

startup_log=$(mktemp "${TMPDIR:-/tmp}/carfree-dev.XXXXXXXX")
server_pid=
cleanup() {
    if [[ -n $server_pid ]]; then
        kill "$server_pid" 2>/dev/null || true
        wait "$server_pid" 2>/dev/null || true
    fi
    rm -f -- "$startup_log"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

rustup run "$toolchain" cargo build --locked --message-format=json-render-diagnostics \
    -p mobility-api --bin spatial-workbench >"$startup_log"
executable=$(python3 - "$startup_log" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as messages:
    for line in messages:
        message = json.loads(line)
        if (message.get("reason") == "compiler-artifact"
                and message["target"]["name"] == "spatial-workbench"
                and message.get("executable")):
            print(message["executable"])
            break
    else:
        raise SystemExit("Cargo did not report a spatial-workbench executable.")
PY
)

"$executable" "$port" "$workspace_dir" "$owner_file" >"$startup_log" &
server_pid=$!
api_path=/api/e19/v1
startup_pattern='^Spatial workbench: (http://127\.0\.0\.1:[0-9]+)$'

deadline=$((SECONDS + 30))
while (( SECONDS < deadline )); do
    if ! kill -0 "$server_pid" 2>/dev/null; then
        echo 'The spatial workbench exited before it was ready.' >&2
        status=1
        wait "$server_pid" || status=$?
        server_pid=
        exit "$status"
    fi
    if IFS= read -r line <"$startup_log" && [[ $line =~ $startup_pattern ]]; then
        origin=${BASH_REMATCH[1]}
        if curl --disable --noproxy '*' --fail --silent --max-time 1 \
            "$origin$api_path/openapi.json" >/dev/null; then
            printf 'Workbench: %s/\nWorkbench API (areas): %s/api/e17/v1/areas\n' "$origin" "$origin"
            printf 'Workspace API (cities): %s%s/cities\nOpenAPI: %s%s/openapi.json\n' \
                "$origin" "$api_path" "$origin" "$api_path"
            printf 'Owner credential file: %s\nPress Ctrl-C to stop.\n' "$owner_file" >&2
            status=0
            wait "$server_pid" || status=$?
            server_pid=
            exit "$status"
        fi
    fi
    sleep 0.1
done
echo 'The spatial workbench did not become ready within 30 seconds.' >&2
exit 1
