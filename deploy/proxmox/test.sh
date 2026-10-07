#!/usr/bin/env bash
# Tests for the-gathering.sh's channel handling, run without Proxmox or a container: in_ct runs
# commands locally against a scratch APP_DIR, and the install helpers are stubbed.
#
#   bash deploy/proxmox/test.sh
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

THE_GATHERING_SH_LIBRARY=1
# shellcheck source=deploy/proxmox/the-gathering.sh
source "$here/the-gathering.sh"
set +e

scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT
APP_DIR="$scratch/app"
mkdir -p "$APP_DIR"

failures=0
check() {
  local name="$1" expected="$2" actual="$3"
  if [[ "$expected" == "$actual" ]]; then
    echo "ok   $name"
  else
    echo "FAIL $name: expected '$expected', got '$actual'"
    failures=$((failures + 1))
  fi
}

# --- installed_channel follows the VERSION file -------------------------------------------
in_ct() {
  shift
  bash -c "$*"
}
for case in "v0.1.0:" "nightly-0123456789ab:nightly" "preview-0123456789ab:preview" ":"; do
  version="${case%%:*}"
  expected="${case#*:}"
  if [[ -n "$version" ]]; then echo "$version" >"$APP_DIR/VERSION"; else rm -f "$APP_DIR/VERSION"; fi
  check "installed_channel for '${version:-missing}'" "$expected" "$(installed_channel "")"
done

# --- the in-container helpers ---------------------------------------------------------------
helpers="$scratch/helpers"
mkdir -p "$helpers"
put_file() { cat >"$helpers/$(basename "$2")"; }
push_update_hook() { :; }
NETWORK_DROPIN="$scratch/dropin/wait-for-ipv4.conf"
push_helpers ""
grep -q '^ExecStartPre=-/bin/sh -c .*ip -4 -o addr show scope global' "$helpers/wait-for-ipv4.conf" &&
  grep -q '^After=network-online.target' "$helpers/wait-for-ipv4.conf"
check "the service waits for a global IPv4 address before starting" 0 $?
grep -q 'ref=preview' "$helpers/update"
check "update helper fetches the script from the preview tag on preview installs" 0 $?
bash -n "$helpers/update" && bash -n "$helpers/the-gathering-install"
check "generated helpers are valid bash" 0 $?
# shellcheck disable=SC2016 # the literal line in the generated script
grep -q 'version="${tag}-${sum:0:12}"' "$helpers/the-gathering-install"
check "installer names preview builds by checksum like nightly" 0 $?

# The update helper picks its ref from VERSION.
# shellcheck disable=SC2016 # rewrites the generated script literally
sed -e "s#curl -fsSL#echo#" -e 's#^exec bash -c.*#echo "$script"#' "$helpers/update" >"$scratch/update-dry"
echo "preview-0123456789ab" >"$APP_DIR/VERSION"
check "helper on a preview install reads the preview tag's script" \
  "https://raw.githubusercontent.com/cfbender/the-gathering/preview/deploy/proxmox/the-gathering.sh" \
  "$(bash "$scratch/update-dry" | tr -d '"')"
echo "v0.1.0" >"$APP_DIR/VERSION"
check "helper on a release install reads main's script" \
  "https://raw.githubusercontent.com/cfbender/the-gathering/main/deploy/proxmox/the-gathering.sh" \
  "$(bash "$scratch/update-dry" | tr -d '"')"

# --- update: which tag an explicit or untagged update installs ------------------------------
installed=""
target_container() {
  shift
  TARGET=""
  ARGS=("$@")
}
push_helpers() { :; }
in_ct() {
  shift
  case "$*" in
  "the-gathering-install "*) installed="${*#the-gathering-install }" ;;
  "test -f "*) return 0 ;;
  "readlink -f "*) echo /opt/the-gathering/releases/x ;;
  *) bash -c "$*" ;;
  esac
}
curl() { printf '{\n  "tag_name": "v9.9.9"\n}\n'; }
run_update() {
  installed=""
  VERSION=""
  (update "$@" >/dev/null 2>&1 && echo "$installed") || echo "rejected"
}

echo "preview-0123456789ab" >"$APP_DIR/VERSION"
check "untagged update keeps a preview install on preview" "preview" "$(run_update)"
echo "nightly-0123456789ab" >"$APP_DIR/VERSION"
check "untagged update keeps a nightly install on nightly" "nightly" "$(run_update)"
echo "v0.1.0" >"$APP_DIR/VERSION"
check "untagged update of a release installs the latest release" "v9.9.9" "$(run_update)"
echo "preview-0123456789ab" >"$APP_DIR/VERSION"
check "update vX.Y.Z leaves the preview channel" "v0.1.0" "$(run_update v0.1.0)"
check "update nightly leaves the preview channel" "nightly" "$(run_update nightly)"
check "update preview switches to the preview channel" "preview" "$(run_update preview)"
check "update rejects other names" "rejected" "$(run_update previewish)"
check "update rejects branch names" "rejected" "$(run_update rust-backend)"
check "update accepts release candidates" "v1.0.0-rc.1" "$(run_update v1.0.0-rc.1)"

# --- PUBLIC_URL becomes THE_GATHERING_PUBLIC_URL (and the PHX_* parts for older releases) ---
for case in "https://games.example.com|https://games.example.com|games.example.com|443" \
  "https://games.example.com:8443/|https://games.example.com:8443|games.example.com|8443" \
  "http://10.0.0.5:4000|http://10.0.0.5:4000|10.0.0.5|4000" \
  "http://10.0.0.5|http://10.0.0.5|10.0.0.5|80"; do
  IFS='|' read -r input origin host port <<<"$case"
  PUBLIC_URL="$input" URL_HOST="" URL_PORT="" PUBLIC_ORIGIN=""
  parse_public_url
  check "parse_public_url $input" "$origin $host $port" "$PUBLIC_ORIGIN $URL_HOST $URL_PORT"
done
PUBLIC_URL=""

if ((failures)); then
  echo "$failures check(s) failed"
  exit 1
fi
echo "all checks passed"
