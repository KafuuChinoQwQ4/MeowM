#!/usr/bin/env bash
set -euo pipefail
repo_dir="$(cd "$(dirname "$0")/.." && pwd)"
: "${ANDROID_HOME:?Set ANDROID_HOME to the Android SDK directory}"
export NDK_HOME="${NDK_HOME:-${ANDROID_NDK_ROOT:?Set NDK_HOME or ANDROID_NDK_ROOT}}"
case "${1:-arm64-v8a}" in
    arm64-v8a) android_target=aarch64 ;;
    x86_64) android_target=x86_64 ;;
    *) echo "Supported ABI: arm64-v8a or x86_64" >&2; exit 1 ;;
esac
cd "$repo_dir/apps/tauri-ui"
npm ci
npm run tauri -- android build --debug --apk --target "$android_target" --ci
