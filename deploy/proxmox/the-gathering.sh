#!/usr/bin/env bash
# Creates a Debian LXC on a Proxmox VE host that runs The Gathering natively (no Docker),
# in the spirit of the community-scripts.org helper scripts. Run it as root on the PVE host:
#
#   bash -c "$(curl -fsSL https://raw.githubusercontent.com/cfbender/the-gathering/main/deploy/proxmox/the-gathering.sh)"
#
# Settings are environment variables (defaults in parentheses):
#
#   CTID              container id (next free id)
#   CT_HOSTNAME       hostname (the-gathering)
#   STORAGE           rootfs storage (local-lvm)
#   TEMPLATE_STORAGE  storage that holds CT templates (local)
#   DISK_GB           rootfs size (8)
#   CORES             vCPUs (4)
#   RAM_MB            memory in MiB (2048)
#   BRIDGE            network bridge (vmbr0)
#   IP                dhcp, or CIDR such as 192.168.1.50/24 (dhcp)
#   PIN_IP            with IP=dhcp, store the address DHCP hands out as a static address in the
#                     container config, so it never changes (true). Keep that address out of the
#                     DHCP pool or reserve it; the DHCP server no longer knows it is in use.
#   GATEWAY           gateway for a static IP (empty)
#   TIMEZONE          container time zone: host, or a zone such as America/New_York (host).
#                     The AUTO_UPDATE cron schedule runs in this time zone.
#   SSH_KEYS          public keys to authorize for root (the PVE host's /root/.ssh/authorized_keys)
#   PASSWORD          root password; leave empty for automatic root login on the Proxmox web
#                     console (plus SSH keys and `pct enter`)
#   VERSION           release tag to install, e.g. v0.1.0, nightly for the newest build of main,
#                     or preview for the newest pre-release build of a branch (latest GitHub release)
#
# When run from a terminal the script asks for these; set them to skip the questions:
#
#   PUBLIC_URL        address players use, e.g. https://games.example.com (http://<container-ip>:4000)
#   ADMIN_USERNAME    first administrator account (blank skips; the app then offers its setup page)
#   ADMIN_PASSWORD    at least 12 characters
#   AUTO_UPDATE       cron schedule for automatic updates, or "off" (0 4 * * *: daily at 04:00)
#
# Inside the container:
#   /opt/the-gathering/releases/<tag>   releases from GitHub (the Rust server and the built web app);
#                                       `current` points at the live one
#   /etc/the-gathering.env              settings (copied from .env.example, SECRET_KEY_BASE generated)
#   /var/lib/the-gathering              DATA_DIR: SQLite database, recognizer bundles, image cache
#   the-gathering.service               systemd unit running bin/the_gathering as user the-gathering
#   /usr/local/bin/update               `update [tag]` installs the latest (or given) release, like
#                                       the community-scripts helpers: it runs the current copy of
#                                       this script from GitHub, so updater fixes reach old containers
#                                       (from main, or from the preview tag while a preview build is
#                                       installed, so the script that knows the channel updates it)
#   /etc/cron.d/the-gathering-update    runs `update` on the AUTO_UPDATE schedule (absent when off)
#   the-gathering-update.path           runs `update` when the app creates
#                                       /var/lib/the-gathering/update-request, which is what the
#                                       "Update now" button under Administration > Server settings
#                                       does (the app itself may only write to its data directory)
#
# The container is unprivileged with nesting=1: Debian 13's systemd (257) needs it to boot in an
# LXC (Proxmox warns "Systemd 257 detected. You may need to enable nesting" otherwise), and it is
# the Proxmox GUI default for unprivileged containers. Docker-only keyctl is not enabled.
#
# Later, from the PVE host (or inside the container, without the <CTID>):
#   bash the-gathering.sh update <CTID>                   install the latest release (or nightly build,
#                                                         if that is what the container runs) and restart
#   bash the-gathering.sh update <CTID> v0.2.0            install a specific release
#   bash the-gathering.sh update <CTID> nightly           follow the newest build of main from now on
#   bash the-gathering.sh update <CTID> preview           follow the preview pre-release (a branch build
#                                                         published by release.yml's manual run) from now
#                                                         on; vX.Y.Z or nightly leaves it again
#   bash the-gathering.sh auto-update <CTID> '0 3 * * 0'  change the automatic update schedule
#   bash the-gathering.sh auto-update <CTID> off          disable automatic updates
set -euo pipefail

APP="The Gathering"
REPO="cfbender/the-gathering"
# Release tags this script installs: vX.Y.Z, or the rolling nightly (main) and preview (a branch).
TAG_PATTERN='^(nightly$|preview$|v[0-9]+\.[0-9]+\.[0-9]+)'
APP_DIR="/opt/the-gathering"
DATA_DIR="/var/lib/the-gathering"
ENV_FILE="/etc/the-gathering.env"
SERVICE="the-gathering"
APP_USER="the-gathering"
UNIT_FILE="/etc/systemd/system/${SERVICE}.service"
CRON_FILE="/etc/cron.d/${SERVICE}-update"
REQUEST_FILE="${DATA_DIR}/update-request"
HOOK_DROPIN="/etc/systemd/system/${SERVICE}.service.d/self-update.conf"
NETWORK_DROPIN="/etc/systemd/system/${SERVICE}.service.d/wait-for-ipv4.conf"
AUTO_UPDATE_DEFAULT="0 4 * * *"

CT_HOSTNAME="${CT_HOSTNAME:-the-gathering}"
STORAGE="${STORAGE:-local-lvm}"
TEMPLATE_STORAGE="${TEMPLATE_STORAGE:-local}"
DISK_GB="${DISK_GB:-8}"
CORES="${CORES:-4}"
RAM_MB="${RAM_MB:-2048}"
BRIDGE="${BRIDGE:-vmbr0}"
IP="${IP:-dhcp}"
PIN_IP="${PIN_IP:-true}"
GATEWAY="${GATEWAY:-}"
TIMEZONE="${TIMEZONE:-host}"
SSH_KEYS="${SSH_KEYS:-/root/.ssh/authorized_keys}"
PASSWORD="${PASSWORD:-}"
VERSION="${VERSION:-}"
PUBLIC_URL="${PUBLIC_URL:-}"
ADMIN_USERNAME="${ADMIN_USERNAME:-}"
ADMIN_PASSWORD="${ADMIN_PASSWORD:-}"
AUTO_UPDATE="${AUTO_UPDATE:-}"
PHX_HOST="" PHX_SCHEME="" PHX_URL_PORT=""

info() { printf '\033[1;34m->\033[0m %s\n' "$*"; }
ok() { printf '\033[1;32mok\033[0m %s\n' "$*"; }
die() {
  printf '\033[1;31merror\033[0m %s\n' "$*" >&2
  exit 1
}

# The script runs in two places: on the PVE host (create, and update/auto-update with a CTID)
# and inside the container (update/auto-update without a CTID, which is what the `update`
# command does). Functions that work on a container take a CTID; an empty CTID means "here".
on_pve() { command -v pct >/dev/null; }

require_pve() {
  on_pve || die "pct not found: run this on a Proxmox VE host"
  [[ $EUID -eq 0 ]] || die "run as root"
  [[ "$(dpkg --print-architecture)" == amd64 ]] || die "release tarballs are built for amd64 only"
}

require_container() {
  [[ $EUID -eq 0 ]] || die "run as root"
  [[ -f "$UNIT_FILE" ]] || die "no $APP installation here: run this on the PVE host or inside the container"
}

in_ct() {
  local ctid="$1"
  shift
  if [[ -n "$ctid" ]]; then
    pct exec "$ctid" -- bash -lc "$*"
  else
    bash -lc "$*"
  fi
}

# Writes stdin to a file in the container with the given mode.
put_file() {
  local ctid="$1" path="$2" mode="$3" tmp
  tmp="$(mktemp)"
  cat >"$tmp"
  if [[ -n "$ctid" ]]; then
    pct push "$ctid" "$tmp" "$path" --perms "$mode"
  else
    install -m "$mode" "$tmp" "$path"
  fi
  rm -f "$tmp"
}

# Newest Debian 13 template for the host's architecture. pveam also lists arm64 templates,
# which an x86 host cannot run.
latest_debian_template() {
  local arch
  arch="$(dpkg --print-architecture)"
  pveam update >/dev/null
  pveam available --section system | awk '{print $2}' |
    grep "^debian-13-standard_.*_${arch}\.tar" | sort -V | tail -n1
}

# Resolves VERSION to a release tag (vX.Y.Z, nightly, or preview), defaulting to the newest GitHub
# release.
resolve_version() {
  if [[ -n "$VERSION" ]]; then
    printf '%s\n' "$VERSION"
    return
  fi
  local tag
  tag="$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" |
    sed -n 's/^[[:space:]]*"tag_name":[[:space:]]*"\([^"]*\)".*/\1/p' | head -n1)"
  [[ -n "$tag" ]] || die "could not determine the latest release of ${REPO}; set VERSION=vX.Y.Z"
  printf '%s\n' "$tag"
}

# Splits PUBLIC_URL into PHX_HOST/PHX_SCHEME/PHX_URL_PORT. Empty PUBLIC_URL leaves them
# empty, and the .env.example defaults (http://localhost:4000) are patched with the
# container IP after it is known.
parse_public_url() {
  [[ -n "$PUBLIC_URL" ]] || return 0
  local rest
  case "$PUBLIC_URL" in
  https://*) PHX_SCHEME=https rest="${PUBLIC_URL#https://}" ;;
  http://*) PHX_SCHEME=http rest="${PUBLIC_URL#http://}" ;;
  *) die "PUBLIC_URL must start with http:// or https:// (got '$PUBLIC_URL')" ;;
  esac
  rest="${rest%%/*}"
  if [[ "$rest" == *:* ]]; then
    PHX_HOST="${rest%%:*}"
    PHX_URL_PORT="${rest##*:}"
  else
    PHX_HOST="$rest"
    [[ "$PHX_SCHEME" == https ]] && PHX_URL_PORT=443 || PHX_URL_PORT=80
  fi
  [[ -n "$PHX_HOST" && "$PHX_URL_PORT" =~ ^[0-9]+$ ]] || die "could not parse PUBLIC_URL '$PUBLIC_URL'"
}

# Asks for the settings the installer cannot discover, unless they came in as env vars
# or stdin is not a terminal.
ask_settings() {
  [[ -t 0 ]] || return 0
  if [[ -z "$PUBLIC_URL" ]]; then
    read -rp "Public URL players will use (e.g. https://games.example.com; blank for http://<container-ip>:4000): " PUBLIC_URL
  fi
  if [[ -z "$ADMIN_USERNAME" ]]; then
    read -rp "First administrator username (blank to skip and use the in-app setup page): " ADMIN_USERNAME
  fi
  if [[ -n "$ADMIN_USERNAME" && -z "$ADMIN_PASSWORD" ]]; then
    local confirm
    while :; do
      read -rsp "Administrator password (12+ characters): " ADMIN_PASSWORD
      echo
      read -rsp "Confirm password: " confirm
      echo
      if [[ ${#ADMIN_PASSWORD} -lt 12 ]]; then
        echo "password must be at least 12 characters" >&2
      elif [[ "$ADMIN_PASSWORD" != "$confirm" ]]; then
        echo "passwords do not match" >&2
      else
        break
      fi
    done
  fi
  if [[ -z "$AUTO_UPDATE" ]]; then
    read -rp "Automatic update schedule (cron expression, 'off' to disable; blank for daily at 04:00): " AUTO_UPDATE
  fi
}

# Accepts "off", a cron nickname such as @daily, or a five-field cron expression.
valid_schedule() {
  local -a fields
  [[ "$1" == off || "$1" =~ ^@(hourly|daily|midnight|weekly|monthly|yearly|annually)$ ]] && return 0
  read -ra fields <<<"$1"
  [[ ${#fields[@]} -eq 5 ]]
}

wait_for_network() {
  local ctid="$1" tries=0
  until in_ct "$ctid" "getent hosts deb.debian.org >/dev/null"; do
    tries=$((tries + 1))
    [[ $tries -lt 30 ]] || die "container has no network after 60s; check BRIDGE/IP/GATEWAY"
    sleep 2
  done
}

ct_ip() {
  in_ct "$1" "hostname -I" | awk '{print $1}'
}

ct_timezone() {
  in_ct "$1" "readlink -f /etc/localtime" | sed 's|^/usr/share/zoneinfo/||'
}

# Turns the address DHCP gave eth0 into a static address in the container config (keeping the
# rest of net0, in particular the MAC) and carries the DHCP nameservers over, since a static
# container otherwise inherits the host's resolver. Proxmox applies the new net0 live but leaves
# the guest's DHCP client running, so the caller reboots the container afterwards.
pin_dhcp_ip() {
  local ctid="$1" net0 cidr gw nameservers
  net0="$(pct config "$ctid" | sed -n 's/^net0: //p')"
  [[ "$net0" == *,ip=dhcp* ]] || return 0
  cidr="$(in_ct "$ctid" "ip -4 -o addr show dev eth0 scope global" | awk '{print $4; exit}')"
  gw="$(in_ct "$ctid" "ip -4 route show default dev eth0" | awk '{print $3; exit}')"
  nameservers="$(in_ct "$ctid" "awk '/^nameserver/ {print \$2}' /etc/resolv.conf" | paste -sd' ')"
  [[ -n "$cidr" && -n "$gw" ]] || die "could not read the DHCP address of container $ctid to make it static; set PIN_IP=false or a static IP"
  local -a opts=(--net0 "${net0/,ip=dhcp/,ip=${cidr},gw=${gw}}")
  [[ -n "$nameservers" ]] && opts+=(--nameserver "$nameservers")
  pct set "$ctid" "${opts[@]}"
  ok "pinned ${cidr} via ${gw} as the container's static address"
}

# Installs the in-container helpers, shared by create and update:
#   /usr/local/bin/the-gathering-install <tag>  downloads a release tarball from GitHub, verifies
#                                               its checksum, unpacks it next to the previous
#                                               releases, repoints `current`, restarts the service
#   /usr/local/bin/update [tag]                 runs this script's `update` from GitHub main
#   the-gathering-update.path                   runs `update` when the app creates REQUEST_FILE
#                                               (the admin UI's "Update now" button); a drop-in
#                                               tells the app which file to create
push_helpers() {
  local ctid="$1"
  push_network_wait "$ctid"
  push_update_hook "$ctid"
  put_file "$ctid" /usr/local/bin/update 0755 <<EOF
#!/usr/bin/env bash
# Updates ${APP} in this container (usage: update [tag]). Runs the current
# deploy/proxmox/the-gathering.sh from GitHub so the updater itself stays current: main's copy,
# or the copy at the preview tag while a preview build is installed (main's may predate the
# preview channel and would fall back to the latest release).
set -euo pipefail
ref=main
if grep -qs '^preview' ${APP_DIR}/VERSION; then ref=preview; fi
script="\$(curl -fsSL "https://raw.githubusercontent.com/${REPO}/\${ref}/deploy/proxmox/the-gathering.sh")"
exec bash -c "\$script" the-gathering.sh update "\$@"
EOF
  put_file "$ctid" /usr/local/bin/the-gathering-install 0755 <<EOF
#!/usr/bin/env bash
# Usage: the-gathering-install <tag>   (a vX.Y.Z release tag, nightly for the latest main build, or
# preview for the latest branch pre-release)
set -euo pipefail
tag="\${1:?usage: the-gathering-install <tag>}"
archive="the_gathering-\${tag}-linux-amd64.tar.gz"
base="https://github.com/${REPO}/releases/download/\${tag}"

tmp="\$(mktemp -d)"
trap 'rm -rf "\$tmp"' EXIT
curl -fsSL "\${base}/\${archive}.sha256" -o "\${tmp}/\${archive}.sha256"
# The nightly and preview tags are republished for every build, so their builds are told apart by
# checksum; VERSION then reads nightly-<checksum prefix> or preview-<checksum prefix>, which is
# how \`update\` knows to keep following that channel.
if [ "\$tag" = nightly ] || [ "\$tag" = preview ]; then
  sum="\$(awk '{print \$1}' "\${tmp}/\${archive}.sha256")"
  version="\${tag}-\${sum:0:12}"
else
  version="\$tag"
fi
release_dir="${APP_DIR}/releases/\${version}"

if [ -e "\$release_dir" ] && [ "\$(readlink -f ${APP_DIR}/current)" = "\$release_dir" ]; then
  echo "\${version} is already installed"
  exit 0
fi

curl -fsSL "\${base}/\${archive}" -o "\${tmp}/\${archive}"
(cd "\$tmp" && sha256sum -c --quiet "\${archive}.sha256")

rm -rf "\$release_dir"
mkdir -p "\$release_dir"
tar -xzf "\${tmp}/\${archive}" -C "\$release_dir" --strip-components=1
chown -R root:${APP_USER} "\$release_dir"
ln -sfn "\$release_dir" "${APP_DIR}/current.new"
mv -T "${APP_DIR}/current.new" "${APP_DIR}/current"
echo "\$version" >"${APP_DIR}/VERSION"

# Keep the previous release for a quick rollback (ln -sfn it back to current), drop older ones.
ls -1dt ${APP_DIR}/releases/*/ | tail -n +3 | xargs -r rm -rf

if systemctl is-enabled -q ${SERVICE} 2>/dev/null; then
  systemctl restart ${SERVICE}
fi
echo "installed \${tag}"
EOF
}

# Inside an LXC container network-online.target can be reached before DHCP has assigned the
# address, so the service waits (up to 30 s) for a global IPv4 address before starting. The
# server gathers webcam media addresses for every room it creates, so a late address is picked
# up by the next room anyway; this only spares the first rooms after a boot. Written on install
# and on every update; push_update_hook's daemon-reload applies it.
push_network_wait() {
  local ctid="$1"
  in_ct "$ctid" "mkdir -p $(dirname "$NETWORK_DROPIN")"
  put_file "$ctid" "$NETWORK_DROPIN" 0644 <<'EOF'
# Written by deploy/proxmox/the-gathering.sh: wait for DHCP before starting, so webcam rooms
# can announce the container's address from the first one on.
[Unit]
After=network-online.target
Wants=network-online.target

[Service]
ExecStartPre=-/bin/sh -c 'command -v ip >/dev/null || exit 0; timeout 30 sh -c "until ip -4 -o addr show scope global | grep -q inet; do sleep 1; done" || echo "no global IPv4 address after 30s; starting anyway" >&2'
EOF
}

# The service runs as an unprivileged user on a read-only system, so the app cannot run `update`
# itself: it creates REQUEST_FILE in its data directory, the path unit notices, and systemd runs
# `update` as root (following the installed channel, like the cron job). The file is removed when
# `update` has finished, whatever the outcome, so the admin UI can tell the update is still running
# and the path unit does not fire again for the same request.
push_update_hook() {
  local ctid="$1"
  put_file "$ctid" "/etc/systemd/system/${SERVICE}-update.service" 0644 <<EOF
[Unit]
Description=${APP} update requested from the admin UI

[Service]
Type=oneshot
ExecStart=/bin/bash -o pipefail -c 'trap "rm -f ${REQUEST_FILE}" EXIT; update 2>&1 | logger -t ${SERVICE}-update'
EOF
  put_file "$ctid" "/etc/systemd/system/${SERVICE}-update.path" 0644 <<EOF
[Unit]
Description=Watches for update requests from the ${APP} admin UI

[Path]
PathExists=${REQUEST_FILE}
Unit=${SERVICE}-update.service

[Install]
WantedBy=multi-user.target
EOF
  in_ct "$ctid" "mkdir -p $(dirname "$HOOK_DROPIN")"
  put_file "$ctid" "$HOOK_DROPIN" 0644 <<EOF
# Written by deploy/proxmox/the-gathering.sh: lets the admin UI request updates through
# ${SERVICE}-update.path.
[Service]
Environment=SELF_UPDATE_REQUEST_FILE=${REQUEST_FILE}
EOF
  in_ct "$ctid" "systemctl daemon-reload && systemctl enable -q --now ${SERVICE}-update.path"
}

# Writes (or, for "off", removes) the cron.d entry that runs `update` on a schedule. Output goes
# to the journal under the tag the-gathering-update.
set_auto_update() {
  local ctid="$1" schedule="$2"
  valid_schedule "$schedule" || die "AUTO_UPDATE must be 'off', @daily-style, or a five-field cron expression (got '$schedule')"
  if [[ "$schedule" == off ]]; then
    in_ct "$ctid" "rm -f ${CRON_FILE}"
    ok "automatic updates are off"
    return
  fi
  # Containers created before the `update` command existed need it and cron installed.
  in_ct "$ctid" "test -x /usr/local/bin/update" || push_helpers "$ctid"
  in_ct "$ctid" "command -v cron >/dev/null || (apt-get update -qq && DEBIAN_FRONTEND=noninteractive apt-get install -y -qq cron >/dev/null)"
  put_file "$ctid" "$CRON_FILE" 0644 <<EOF
# Automatic updates for ${APP}, written by deploy/proxmox/the-gathering.sh.
# Change the schedule with \`the-gathering.sh auto-update <CTID> '<cron expression>'\` on the PVE
# host (or edit the line below); \`auto-update <CTID> off\` removes this file. Logs:
#   journalctl -t the-gathering-update
SHELL=/bin/bash
PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin
${schedule} root update 2>&1 | logger -t the-gathering-update
EOF
  ok "automatic updates run on '${schedule}' in the container's time zone, $(ct_timezone "$ctid") (${CRON_FILE})"
}

create() {
  require_pve
  local ctid="${CTID:-$(pvesh get /cluster/nextid)}"
  pct status "$ctid" >/dev/null 2>&1 && die "container $ctid already exists; set CTID to a free id"
  ask_settings
  if [[ -n "$ADMIN_USERNAME" || -n "$ADMIN_PASSWORD" ]]; then
    [[ -n "$ADMIN_USERNAME" && -n "$ADMIN_PASSWORD" ]] || die "ADMIN_USERNAME and ADMIN_PASSWORD must be set together"
    [[ ${#ADMIN_PASSWORD} -ge 12 ]] || die "ADMIN_PASSWORD must be at least 12 characters"
  fi
  parse_public_url
  AUTO_UPDATE="${AUTO_UPDATE:-$AUTO_UPDATE_DEFAULT}"
  valid_schedule "$AUTO_UPDATE" || die "AUTO_UPDATE must be 'off', @daily-style, or a five-field cron expression (got '$AUTO_UPDATE')"

  local tag
  tag="$(resolve_version)"
  ok "installing $APP $tag"

  info "Downloading Debian 13 template"
  local template
  template="$(latest_debian_template)"
  [[ -n "$template" ]] || die "no debian-13-standard template offered by pveam"
  pveam download "$TEMPLATE_STORAGE" "$template" >/dev/null || true
  ok "$template"

  info "Creating container $ctid ($CORES cores, ${RAM_MB} MiB, ${DISK_GB} GiB on $STORAGE)"
  local net="name=eth0,bridge=${BRIDGE},ip=${IP}"
  [[ -n "$GATEWAY" ]] && net+=",gw=${GATEWAY}"
  local -a auth=()
  [[ -n "$PASSWORD" ]] && auth+=(--password "$PASSWORD")
  [[ -f "$SSH_KEYS" ]] && auth+=(--ssh-public-keys "$SSH_KEYS")
  pct create "$ctid" "${TEMPLATE_STORAGE}:vztmpl/${template}" \
    --hostname "$CT_HOSTNAME" \
    --ostype debian \
    --unprivileged 1 \
    --features nesting=1 \
    --cores "$CORES" \
    --memory "$RAM_MB" \
    --swap 512 \
    --rootfs "${STORAGE}:${DISK_GB}" \
    --net0 "$net" \
    --timezone "$TIMEZONE" \
    --onboot 1 \
    --tags the-gathering \
    "${auth[@]}" >/dev/null
  pct start "$ctid"
  wait_for_network "$ctid"
  if [[ "$IP" == dhcp && "$PIN_IP" == true ]]; then
    pin_dhcp_ip "$ctid"
    # Boot once more so the guest's DHCP client is gone and resolv.conf comes from the new config.
    pct reboot "$ctid"
    wait_for_network "$ctid"
  fi
  local ip
  ip="$(ct_ip "$ctid")"
  ok "container $ctid is up at $ip"

  local trust_proxy="" AUTOLOGIN=false
  [[ -z "$PASSWORD" ]] && AUTOLOGIN=true
  if [[ -z "$PHX_HOST" ]]; then
    PHX_HOST="$ip" PHX_SCHEME=http PHX_URL_PORT=4000
  else
    # A public URL implies a reverse proxy in front of the container.
    trust_proxy=true
  fi

  info "Installing runtime packages and $APP $tag"
  push_helpers "$ctid"
  local setup
  setup="$(mktemp)"
  # The server needs only glibc; DejaVu is the font for game summary images, iproute2 lets the
  # service wait for its IPv4 address (push_network_wait), and cron runs the automatic updates.
  cat >"$setup" <<EOF
set -euo pipefail
export DEBIAN_FRONTEND=noninteractive
# pct exec inherits the host's LANG, which the template has not generated; use the built-in locale.
export LC_ALL=C.UTF-8 LANG=C.UTF-8
apt-get update -qq
apt-get install -y -qq curl ca-certificates openssl rsync openssh-server cron iproute2 \\
  fonts-dejavu-core >/dev/null

if [ "${AUTOLOGIN}" = true ]; then
  # No root password was set, so log root in automatically on the Proxmox web console
  # (which already requires Proxmox authentication), as the community-scripts helpers do.
  mkdir -p /etc/systemd/system/container-getty@1.service.d
  cat >/etc/systemd/system/container-getty@1.service.d/override.conf <<'GETTY'
[Service]
ExecStart=
ExecStart=-/sbin/agetty --autologin root --noclear --keep-baud tty%I 115200,38400,9600 \$TERM
GETTY
  systemctl daemon-reload
  systemctl restart container-getty@1.service
fi

if ! id -u ${APP_USER} >/dev/null 2>&1; then
  useradd --system --home-dir ${DATA_DIR} --shell /usr/sbin/nologin ${APP_USER}
fi
mkdir -p ${APP_DIR}/releases ${DATA_DIR}
chown ${APP_USER}:${APP_USER} ${DATA_DIR}
chmod 750 ${DATA_DIR}

if [ ! -f ${ENV_FILE} ]; then
  curl -fsSL https://raw.githubusercontent.com/${REPO}/${tag}/.env.example -o ${ENV_FILE}
  secret="\$(openssl rand -base64 64 | tr -d '\n')"
  sed -i "s|^SECRET_KEY_BASE=.*|SECRET_KEY_BASE=\${secret}|" ${ENV_FILE}
  sed -i "s|^PHX_HOST=.*|PHX_HOST=${PHX_HOST}|; s|^PHX_SCHEME=.*|PHX_SCHEME=${PHX_SCHEME}|; s|^PHX_URL_PORT=.*|PHX_URL_PORT=${PHX_URL_PORT}|; s|^TRUST_PROXY_HEADERS=.*|TRUST_PROXY_HEADERS=${trust_proxy}|" ${ENV_FILE}
  chown root:${APP_USER} ${ENV_FILE}
  chmod 640 ${ENV_FILE}
fi

cat >/etc/systemd/system/${SERVICE}.service <<'UNIT'
[Unit]
Description=${APP}
After=network-online.target
Wants=network-online.target

[Service]
Type=exec
User=${APP_USER}
Group=${APP_USER}
EnvironmentFile=${ENV_FILE}
WorkingDirectory=${APP_DIR}/current
# Paths are fixed here, after the env file, so a settings file copied from a Docker install
# (DATA_DIR=/data, DATABASE_PATH=/data/..., PORT=...) cannot point the service elsewhere.
ExecStart=/usr/bin/env THE_GATHERING_ENV=prod PORT=4000 DATA_DIR=${DATA_DIR} DATABASE_PATH=${DATA_DIR}/the_gathering.db LANG=C.UTF-8 LC_ALL=C.UTF-8 ${APP_DIR}/current/bin/the_gathering start
Restart=on-failure
RestartSec=5
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=${DATA_DIR}

[Install]
WantedBy=multi-user.target
UNIT

the-gathering-install ${tag}
systemctl daemon-reload
systemctl enable -q ${SERVICE}
EOF
  pct push "$ctid" "$setup" /root/the-gathering-setup.sh --perms 0700
  rm -f "$setup"
  in_ct "$ctid" "/root/the-gathering-setup.sh && rm -f /root/the-gathering-setup.sh"

  if [[ -n "$ADMIN_USERNAME" ]]; then
    info "Creating administrator $ADMIN_USERNAME"
    in_ct "$ctid" "set -a; . ${ENV_FILE}; set +a; \
      THE_GATHERING_ADMIN_USERNAME=$(printf %q "$ADMIN_USERNAME") \
      THE_GATHERING_ADMIN_PASSWORD=$(printf %q "$ADMIN_PASSWORD") \
      DATA_DIR=${DATA_DIR} DATABASE_PATH=${DATA_DIR}/the_gathering.db \
      setpriv --reuid=${APP_USER} --regid=${APP_USER} --init-groups \
      ${APP_DIR}/current/bin/the_gathering bootstrap-admin"
  fi

  in_ct "$ctid" "systemctl start ${SERVICE}"
  ok "$APP $tag is starting"
  set_auto_update "$ctid" "$AUTO_UPDATE"

  cat <<EOF

${APP} ${tag} is running in container ${ctid} at http://${ip}:4000
EOF
  [[ -n "$PUBLIC_URL" ]] && echo "Configured for ${PHX_SCHEME}://${PHX_HOST}:${PHX_URL_PORT}; point your reverse proxy at http://${ip}:4000."
  if [[ "$IP" == dhcp && "$PIN_IP" == true ]]; then
    echo "${ip} came from DHCP and is now the container's static address: keep it out of the DHCP pool or reserve it."
  fi
  cat <<EOF

Next steps:
  * Optional settings (Discord, TURN, ManaVault) live in ${ENV_FILE} in the container; after editing run
      pct exec ${ctid} -- systemctl restart ${SERVICE}
  * Logs:   pct exec ${ctid} -- journalctl -fu ${SERVICE}
  * Update: the "Update now" button under Administration > Server settings, \`update\` inside
            the container, or bash the-gathering.sh update ${ctid}
EOF
  if [[ "$AUTO_UPDATE" == off ]]; then
    echo "  * Automatic updates are off; enable with bash the-gathering.sh auto-update ${ctid} '${AUTO_UPDATE_DEFAULT}'"
  else
    echo "  * Automatic updates run on '${AUTO_UPDATE}' ($(ct_timezone "$ctid") time); change with bash the-gathering.sh auto-update ${ctid} '<cron>' (or off)"
  fi
  if [[ -z "$ADMIN_USERNAME" ]]; then
    cat <<EOF
  * Create the first administrator (or open the app and use the setup page):
      ADMIN_USERNAME=... ADMIN_PASSWORD=... bash the-gathering.sh bootstrap-admin ${ctid}
EOF
  fi
}

# Resolves the container that update/auto-update act on: sets TARGET to the CTID argument on
# the PVE host (empty inside the container) and ARGS to the remaining arguments.
target_container() {
  local usage="$1"
  shift
  if on_pve; then
    require_pve
    TARGET="${1:-}"
    [[ -n "$TARGET" ]] || die "usage: the-gathering.sh $usage"
    in_ct "$TARGET" "test -f ${UNIT_FILE}" || die "no $APP installation in container $TARGET"
    ARGS=("${@:2}")
  else
    require_container
    TARGET=""
    ARGS=("$@")
  fi
}

# Prints "nightly" or "preview" when the container runs a build of that rolling channel, so an
# untagged `update` (the cron job and the admin UI's button) follows the installed channel
# instead of dropping back to the newest tagged release.
installed_channel() {
  local version
  version="$(in_ct "$1" "cat ${APP_DIR}/VERSION 2>/dev/null" || true)"
  case "$version" in
  nightly*) echo nightly ;;
  preview*) echo preview ;;
  esac
}

update() {
  target_container "update <CTID> [tag]" "$@"
  VERSION="${ARGS[0]:-$VERSION}"
  if [[ -n "$VERSION" && ! "$VERSION" =~ $TAG_PATTERN ]]; then
    local hint=""
    [[ -n "$TARGET" ]] || hint="; inside the container run \`update [tag]\` without a CTID"
    die "'$VERSION' is not a release tag (vX.Y.Z, nightly, or preview)$hint"
  fi
  [[ -n "$VERSION" ]] || VERSION="$(installed_channel "$TARGET")"
  local tag
  tag="$(resolve_version)"
  info "Installing $APP $tag"
  # A container from before the admin UI could request updates gets the hook now; the app only
  # sees it after a restart, which the install does anyway unless this version is already live.
  local had_hook=true current_before current_after
  in_ct "$TARGET" "test -f ${HOOK_DROPIN}" || had_hook=false
  current_before="$(in_ct "$TARGET" "readlink -f ${APP_DIR}/current" || true)"
  push_helpers "$TARGET"
  in_ct "$TARGET" "the-gathering-install ${tag}"
  current_after="$(in_ct "$TARGET" "readlink -f ${APP_DIR}/current")"
  if [[ "$had_hook" == false && "$current_before" == "$current_after" ]]; then
    in_ct "$TARGET" "systemctl try-restart ${SERVICE}"
    ok "restarted $APP so the admin UI can request updates"
  fi
  ok "$APP $tag is running${TARGET:+ in container $TARGET}"
  if ! in_ct "$TARGET" "test -f ${CRON_FILE}"; then
    info "Automatic updates are off; enable with the-gathering.sh auto-update ${TARGET:+$TARGET }'${AUTO_UPDATE_DEFAULT}'"
  fi
}

auto_update() {
  target_container "auto-update <CTID> <cron expression | off>" "$@"
  [[ -n "${ARGS[0]:-}" ]] || die "usage: the-gathering.sh auto-update ${TARGET:+<CTID> }<cron expression | off>"
  set_auto_update "$TARGET" "${ARGS[0]}"
}

bootstrap_admin() {
  require_pve
  local ctid="${1:-}"
  [[ -n "$ctid" ]] || die "usage: ADMIN_USERNAME=... ADMIN_PASSWORD=... the-gathering.sh bootstrap-admin <CTID>"
  [[ -n "$ADMIN_USERNAME" && -n "$ADMIN_PASSWORD" ]] || die "set ADMIN_USERNAME and ADMIN_PASSWORD"
  in_ct "$ctid" "systemctl stop ${SERVICE}"
  in_ct "$ctid" "set -a; . ${ENV_FILE}; set +a; \
    THE_GATHERING_ADMIN_USERNAME=$(printf %q "$ADMIN_USERNAME") \
    THE_GATHERING_ADMIN_PASSWORD=$(printf %q "$ADMIN_PASSWORD") \
    DATA_DIR=${DATA_DIR} DATABASE_PATH=${DATA_DIR}/the_gathering.db \
    setpriv --reuid=${APP_USER} --regid=${APP_USER} --init-groups \
    ${APP_DIR}/current/bin/the_gathering bootstrap-admin"
  in_ct "$ctid" "systemctl start ${SERVICE}"
  ok "administrator $ADMIN_USERNAME is ready in container $ctid"
}

usage() {
  cat <<EOF
usage: the-gathering.sh [create | update <CTID> [tag] | auto-update <CTID> <cron | off> | bootstrap-admin <CTID>]

  create                  create a Debian LXC running ${APP} (default; settings via env vars,
                          see the comment at the top of this script)
  update <CTID> [tag]     install the latest (or given) release in an existing container; the tag
                          nightly switches it to the newest build of main, preview to the preview
                          pre-release of a branch, vX.Y.Z back to releases
  auto-update <CTID> <cron expression | off>
                          schedule automatic updates (${AUTO_UPDATE_DEFAULT} by default) or turn them off
  bootstrap-admin <CTID>  create the first administrator from ADMIN_USERNAME/ADMIN_PASSWORD

Inside the container, \`update [tag]\` is on PATH and update/auto-update take no <CTID>.
EOF
}

# deploy/proxmox/test.sh sources this file for its functions without running a command.
[[ -n "${THE_GATHERING_SH_LIBRARY:-}" ]] && return 0

case "${1:-create}" in
create) create ;;
update) update "${@:2}" ;;
auto-update) auto_update "${@:2}" ;;
bootstrap-admin) bootstrap_admin "${2:-}" ;;
-h | --help | help) usage ;;
*)
  usage >&2
  die "unknown command '$1'"
  ;;
esac
