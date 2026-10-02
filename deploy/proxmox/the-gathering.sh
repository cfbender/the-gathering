#!/usr/bin/env bash
# Creates a Debian LXC on a Proxmox VE host that runs The Gathering with Docker Compose,
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
#   DISK_GB           rootfs size (16)
#   CORES             vCPUs (4)
#   RAM_MB            memory in MiB (4096)
#   BRIDGE            network bridge (vmbr0)
#   IP                dhcp, or CIDR such as 192.168.1.50/24 (dhcp)
#   GATEWAY           gateway for a static IP (empty)
#   SSH_KEYS          public keys to authorize for root (the PVE host's /root/.ssh/authorized_keys)
#   PASSWORD          root password; leave empty to rely on keys and `pct enter`
#   REPO_REF          branch or tag whose docker-compose.yml/.env.example to install (main)
#
# Inside the container the app lives in /opt/the-gathering: docker-compose.yml, .env, and
# data/ (SQLite, recognizer bundles, image cache). Docker needs nesting and keyctl, which the
# script enables; on ZFS-backed storage Docker falls back to the slower fuse-overlayfs driver,
# so prefer LVM-thin or a directory storage for the rootfs.
#
# Later, from the PVE host:
#   bash the-gathering.sh update <CTID>   pulls the latest image and restarts the app
set -euo pipefail

APP="The Gathering"
REPO="cfbender/the-gathering"
REPO_REF="${REPO_REF:-main}"
RAW="https://raw.githubusercontent.com/${REPO}/${REPO_REF}"
APP_DIR="/opt/the-gathering"

CT_HOSTNAME="${CT_HOSTNAME:-the-gathering}"
STORAGE="${STORAGE:-local-lvm}"
TEMPLATE_STORAGE="${TEMPLATE_STORAGE:-local}"
DISK_GB="${DISK_GB:-16}"
CORES="${CORES:-4}"
RAM_MB="${RAM_MB:-4096}"
BRIDGE="${BRIDGE:-vmbr0}"
IP="${IP:-dhcp}"
GATEWAY="${GATEWAY:-}"
SSH_KEYS="${SSH_KEYS:-/root/.ssh/authorized_keys}"
PASSWORD="${PASSWORD:-}"

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

# Runs a command inside the container with a login shell, so PATH includes Docker.
in_ct() {
  local ctid="$1"
  shift
  pct exec "$ctid" -- bash -lc "$*"
}

latest_debian_template() {
  pveam update >/dev/null
  pveam available --section system | awk '{print $2}' | grep '^debian-13-standard' | sort -V | tail -n1
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

create() {
  require_pve
  local ctid="${CTID:-$(pvesh get /cluster/nextid)}"
  pct status "$ctid" >/dev/null 2>&1 && die "container $ctid already exists; set CTID to a free id"

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
    --features nesting=1,keyctl=1 \
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

  info "Installing Docker and $APP (this takes a few minutes)"
  local setup
  setup="$(mktemp)"
  cat >"$setup" <<EOF
set -euo pipefail
export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y -qq curl ca-certificates rsync openssh-server >/dev/null
curl -fsSL https://get.docker.com | sh >/dev/null 2>&1
systemctl enable -q --now docker
mkdir -p ${APP_DIR}/data
curl -fsSL ${RAW}/docker-compose.yml -o ${APP_DIR}/docker-compose.yml
if [ ! -f ${APP_DIR}/.env ]; then
  curl -fsSL ${RAW}/.env.example -o ${APP_DIR}/.env
  secret="\$(openssl rand -base64 64 | tr -d '\n')"
  sed -i "s|^SECRET_KEY_BASE=.*|SECRET_KEY_BASE=\${secret}|" ${APP_DIR}/.env
fi
cd ${APP_DIR}
docker compose pull -q
docker compose up -d
EOF
  pct push "$ctid" "$setup" /root/the-gathering-setup.sh --perms 0700
  rm -f "$setup"
  in_ct "$ctid" "/root/the-gathering-setup.sh && rm -f /root/the-gathering-setup.sh"
  ok "$(in_ct "$ctid" "docker --version")"
  ok "$APP is starting"

  local ip
  ip="$(ct_ip "$ctid")"
  cat <<EOF

${APP} is running in container ${ctid} at http://${ip}:4000

Next steps:
  * Edit ${APP_DIR}/.env in the container (PHX_HOST, Discord, TURN) and run
      pct exec ${ctid} -- bash -lc 'cd ${APP_DIR} && docker compose up -d'
  * Point your reverse proxy at http://${ip}:4000.
  * Update later with:  bash the-gathering.sh update ${ctid}
EOF
}

update() {
  require_pve
  local ctid="${1:-}"
  [[ -n "$ctid" ]] || die "usage: the-gathering.sh update <CTID>"
  in_ct "$ctid" "test -f ${APP_DIR}/docker-compose.yml" || die "no $APP installation in container $ctid"
  info "Pulling the latest image"
  in_ct "$ctid" "cd ${APP_DIR} && docker compose pull -q && docker compose up -d && docker image prune -f >/dev/null"
  ok "$APP updated in container $ctid"
}

usage() {
  cat <<EOF
usage: the-gathering.sh [create | update <CTID>]

  create          create a Debian LXC running ${APP} (default; settings via env vars, see the
                  comment at the top of this script)
  update <CTID>   pull the latest image in an existing container and restart the app
EOF
}

case "${1:-create}" in
create) create ;;
update) update "${2:-}" ;;
-h | --help | help) usage ;;
*)
  usage >&2
  die "unknown command '$1'"
  ;;
esac
