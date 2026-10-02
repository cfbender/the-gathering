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
#   GATEWAY           gateway for a static IP (empty)
#   SSH_KEYS          public keys to authorize for root (the PVE host's /root/.ssh/authorized_keys)
#   PASSWORD          root password; leave empty to rely on keys and `pct enter`
#   VERSION           release tag to install, e.g. v0.1.0 (latest GitHub release)
#   ADMIN_USERNAME    with ADMIN_PASSWORD, create the first administrator (empty)
#   ADMIN_PASSWORD
#
# Inside the container:
#   /opt/the-gathering/releases/<tag>   Elixir releases from GitHub; `current` points at the live one
#   /etc/the-gathering.env              settings (copied from .env.example, SECRET_KEY_BASE generated)
#   /var/lib/the-gathering              DATA_DIR: SQLite database, recognizer bundles, image cache
#   the-gathering.service               systemd unit running bin/the_gathering as user the-gathering
#
# Later, from the PVE host:
#   bash the-gathering.sh update <CTID>             install the latest release and restart
#   bash the-gathering.sh update <CTID> v0.2.0      install a specific release
set -euo pipefail

APP="The Gathering"
REPO="cfbender/the-gathering"
APP_DIR="/opt/the-gathering"
DATA_DIR="/var/lib/the-gathering"
ENV_FILE="/etc/the-gathering.env"
SERVICE="the-gathering"
APP_USER="the-gathering"

CT_HOSTNAME="${CT_HOSTNAME:-the-gathering}"
STORAGE="${STORAGE:-local-lvm}"
TEMPLATE_STORAGE="${TEMPLATE_STORAGE:-local}"
DISK_GB="${DISK_GB:-8}"
CORES="${CORES:-4}"
RAM_MB="${RAM_MB:-2048}"
BRIDGE="${BRIDGE:-vmbr0}"
IP="${IP:-dhcp}"
GATEWAY="${GATEWAY:-}"
SSH_KEYS="${SSH_KEYS:-/root/.ssh/authorized_keys}"
PASSWORD="${PASSWORD:-}"
VERSION="${VERSION:-}"
ADMIN_USERNAME="${ADMIN_USERNAME:-}"
ADMIN_PASSWORD="${ADMIN_PASSWORD:-}"

info() { printf '\033[1;34m->\033[0m %s\n' "$*"; }
ok() { printf '\033[1;32mok\033[0m %s\n' "$*"; }
die() {
  printf '\033[1;31merror\033[0m %s\n' "$*" >&2
  exit 1
}

require_pve() {
  command -v pct >/dev/null || die "pct not found: run this on a Proxmox VE host"
  [[ $EUID -eq 0 ]] || die "run as root"
}

in_ct() {
  local ctid="$1"
  shift
  pct exec "$ctid" -- bash -lc "$*"
}

latest_debian_template() {
  pveam update >/dev/null
  pveam available --section system | awk '{print $2}' | grep '^debian-13-standard' | sort -V | tail -n1
}

# Resolves VERSION to a release tag, defaulting to the newest GitHub release.
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

# Installs /usr/local/bin/the-gathering-install inside the container. It downloads a
# release tarball from GitHub, verifies its checksum, unpacks it next to the previous
# releases, repoints `current`, and restarts the service. Shared by create and update.
push_installer() {
  local ctid="$1" installer
  installer="$(mktemp)"
  cat >"$installer" <<EOF
#!/usr/bin/env bash
# Usage: the-gathering-install <tag>
set -euo pipefail
tag="\${1:?usage: the-gathering-install <tag>}"
archive="the_gathering-\${tag}-linux-amd64.tar.gz"
base="https://github.com/${REPO}/releases/download/\${tag}"
release_dir="${APP_DIR}/releases/\${tag}"

if [ -e "\$release_dir" ] && [ "\$(readlink -f ${APP_DIR}/current)" = "\$release_dir" ]; then
  echo "\${tag} is already installed"
  exit 0
fi

tmp="\$(mktemp -d)"
trap 'rm -rf "\$tmp"' EXIT
curl -fsSL "\${base}/\${archive}" -o "\${tmp}/\${archive}"
curl -fsSL "\${base}/\${archive}.sha256" -o "\${tmp}/\${archive}.sha256"
(cd "\$tmp" && sha256sum -c --quiet "\${archive}.sha256")

rm -rf "\$release_dir"
mkdir -p "\$release_dir"
tar -xzf "\${tmp}/\${archive}" -C "\$release_dir" --strip-components=1
chown -R root:${APP_USER} "\$release_dir"
ln -sfn "\$release_dir" "${APP_DIR}/current.new"
mv -T "${APP_DIR}/current.new" "${APP_DIR}/current"
echo "\$tag" >"${APP_DIR}/VERSION"

# Keep the previous release for a quick rollback (ln -sfn it back to current), drop older ones.
ls -1dt ${APP_DIR}/releases/*/ | tail -n +3 | xargs -r rm -rf

if systemctl is-enabled -q ${SERVICE} 2>/dev/null; then
  systemctl restart ${SERVICE}
fi
echo "installed \${tag}"
EOF
  pct push "$ctid" "$installer" /usr/local/bin/the-gathering-install --perms 0755
  rm -f "$installer"
}

create() {
  require_pve
  local ctid="${CTID:-$(pvesh get /cluster/nextid)}"
  pct status "$ctid" >/dev/null 2>&1 && die "container $ctid already exists; set CTID to a free id"
  if [[ -n "$ADMIN_USERNAME" || -n "$ADMIN_PASSWORD" ]]; then
    [[ -n "$ADMIN_USERNAME" && -n "$ADMIN_PASSWORD" ]] || die "ADMIN_USERNAME and ADMIN_PASSWORD must be set together"
  fi

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
    --cores "$CORES" \
    --memory "$RAM_MB" \
    --swap 512 \
    --rootfs "${STORAGE}:${DISK_GB}" \
    --net0 "$net" \
    --onboot 1 \
    --tags the-gathering \
    "${auth[@]}" >/dev/null
  pct start "$ctid"
  wait_for_network "$ctid"
  ok "container $ctid is up"

  info "Installing runtime packages and $APP $tag"
  push_installer "$ctid"
  local setup
  setup="$(mktemp)"
  # Runtime libraries match the Dockerfile runner image: OpenSSL/ncurses for ERTS,
  # libstdc++ and libsctp for NIFs, rsvg-convert + DejaVu for game summary images.
  cat >"$setup" <<EOF
set -euo pipefail
export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y -qq curl ca-certificates openssl rsync openssh-server \\
  libstdc++6 libssl3t64 libncurses6 libsctp1 librsvg2-bin fonts-dejavu-core >/dev/null

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
Environment=PHX_SERVER=true
Environment=PORT=4000
Environment=DATA_DIR=${DATA_DIR}
Environment=RELEASE_TMP=${DATA_DIR}/tmp
Environment=LANG=C.UTF-8 LC_ALL=C.UTF-8
WorkingDirectory=${APP_DIR}/current
ExecStart=${APP_DIR}/current/bin/the_gathering start
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
      DATA_DIR=${DATA_DIR} RELEASE_TMP=${DATA_DIR}/tmp \
      setpriv --reuid=${APP_USER} --regid=${APP_USER} --init-groups \
      ${APP_DIR}/current/bin/the_gathering eval 'TheGathering.Release.bootstrap_admin()'"
  fi

  in_ct "$ctid" "systemctl start ${SERVICE}"
  ok "$APP $tag is starting"

  local ip
  ip="$(ct_ip "$ctid")"
  cat <<EOF

${APP} ${tag} is running in container ${ctid} at http://${ip}:4000

Next steps:
  * Edit ${ENV_FILE} in the container (PHX_HOST, Discord, TURN), then
      pct exec ${ctid} -- systemctl restart ${SERVICE}
  * Point your reverse proxy at http://${ip}:4000.
  * Logs:   pct exec ${ctid} -- journalctl -fu ${SERVICE}
  * Update: bash the-gathering.sh update ${ctid}
EOF
  if [[ -z "$ADMIN_USERNAME" ]]; then
    cat <<EOF
  * Create the first administrator (or open the app and use the setup page):
      ADMIN_USERNAME=... ADMIN_PASSWORD=... bash the-gathering.sh bootstrap-admin ${ctid}
EOF
  fi
}

update() {
  require_pve
  local ctid="${1:-}"
  [[ -n "$ctid" ]] || die "usage: the-gathering.sh update <CTID> [tag]"
  VERSION="${2:-$VERSION}"
  in_ct "$ctid" "test -f /etc/systemd/system/${SERVICE}.service" || die "no $APP installation in container $ctid"
  local tag
  tag="$(resolve_version)"
  info "Installing $APP $tag"
  push_installer "$ctid"
  in_ct "$ctid" "the-gathering-install ${tag}"
  ok "$APP $tag is running in container $ctid"
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
    DATA_DIR=${DATA_DIR} RELEASE_TMP=${DATA_DIR}/tmp \
    setpriv --reuid=${APP_USER} --regid=${APP_USER} --init-groups \
    ${APP_DIR}/current/bin/the_gathering eval 'TheGathering.Release.bootstrap_admin()'"
  in_ct "$ctid" "systemctl start ${SERVICE}"
  ok "administrator $ADMIN_USERNAME is ready in container $ctid"
}

usage() {
  cat <<EOF
usage: the-gathering.sh [create | update <CTID> [tag] | bootstrap-admin <CTID>]

  create                  create a Debian LXC running ${APP} (default; settings via env vars,
                          see the comment at the top of this script)
  update <CTID> [tag]     install the latest (or given) release in an existing container
  bootstrap-admin <CTID>  create the first administrator from ADMIN_USERNAME/ADMIN_PASSWORD
EOF
}

case "${1:-create}" in
create) create ;;
update) update "${2:-}" ;;
bootstrap-admin) bootstrap_admin "${2:-}" ;;
-h | --help | help) usage ;;
*)
  usage >&2
  die "unknown command '$1'"
  ;;
esac
