# syntax=docker/dockerfile:1

ARG RUST_VERSION=1.99.0
ARG ALPINE_VERSION=3.24
ARG NODE_VERSION=26.9.0
ARG AUBE_VERSION=1.21.0

FROM node:${NODE_VERSION}-alpine${ALPINE_VERSION} AS assets

ARG AUBE_VERSION
RUN apk add --no-cache curl ca-certificates tar

WORKDIR /app

# aube is the JavaScript package manager used by the repo (pnpm-compatible).
# Pin the release and verify its checksum. Update AUBE_VERSION and both
# checksums together (musl builds, since the builder is Alpine).
ARG TARGETARCH
ARG AUBE_SHA256_AMD64=6761c69514475a87b375d02a3782ebbab1dfdf181a584ee6b6c91814a882cb37
ARG AUBE_SHA256_ARM64=07da9245c5ac2ef540ed59f795aeee6986d8a9ed5127d4d9c97cbfd1cc05c308
RUN set -eu; \
  arch="${TARGETARCH:-$(uname -m)}"; \
  case "$arch" in \
    amd64|x86_64) aube_arch=x86_64; aube_sha="$AUBE_SHA256_AMD64" ;; \
    arm64|aarch64) aube_arch=aarch64; aube_sha="$AUBE_SHA256_ARM64" ;; \
    *) echo "unsupported build arch: ${arch}" >&2; exit 1 ;; \
  esac; \
  curl -fsSL "https://github.com/aubepkg/aube/releases/download/v${AUBE_VERSION}/aube-v${AUBE_VERSION}-${aube_arch}-unknown-linux-musl.tar.gz" -o /tmp/aube.tar.gz; \
  echo "${aube_sha}  /tmp/aube.tar.gz" | sha256sum -c -; \
  tar -xzf /tmp/aube.tar.gz -C /tmp aube; \
  install /tmp/aube /usr/local/bin/aube; \
  rm -f /tmp/aube /tmp/aube.tar.gz; \
  aube --version

COPY package.json aube-lock.yaml ./
RUN aube install --frozen-lockfile

COPY vite.config.ts tsconfig.json ./
COPY assets assets
COPY priv/static priv/static
RUN NODE_ENV=production aube run build

FROM rust:${RUST_VERSION}-alpine${ALPINE_VERSION} AS server

# build-base compiles the bundled SQLite and aws-lc (the SFU's certificate generation).
RUN apk add --no-cache build-base cmake perl git

WORKDIR /app/rust
COPY rust ./
# Queries are checked against the committed .sqlx metadata (SQLX_OFFLINE in .cargo/config.toml).
# Release builds are not incremental by default; the variable keeps it that way explicitly.
ENV CARGO_INCREMENTAL=0
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/rust/target \
    cargo build --release --locked --bin the-gathering \
    && install -D target/release/the-gathering /out/the-gathering

FROM alpine:${ALPINE_VERSION} AS runner

RUN apk upgrade --no-cache \
  && apk add --no-cache ca-certificates su-exec font-dejavu tzdata

ENV LANG=C.UTF-8
ENV LANGUAGE=C.UTF-8
ENV LC_ALL=C.UTF-8

WORKDIR /app
RUN addgroup -S app && adduser -S -G app -h /home/app -s /bin/sh app \
  && mkdir -p /data /app/bin /app/priv && chown -R app:app /app /data

ENV THE_GATHERING_ENV=prod
ENV PORT=4000
ENV DATA_DIR=/data
ENV PRIV_DIR=/app/priv

COPY --from=server --chown=app:app /out/the-gathering /app/bin/the-gathering
COPY --chown=app:app rust/release/the_gathering /app/bin/the_gathering
COPY --from=assets --chown=app:app /app/priv/static /app/priv/static
# The admin UI shows this version and compares it with GitHub (vX.Y.Z for tags, nightly-<commit>
# for main); container.yml passes it. Empty means a local development build.
ARG APP_VERSION=""
RUN printf '%s\n' "$APP_VERSION" > /app/priv/VERSION && chown app:app /app/priv/VERSION
COPY docker-entrypoint.sh /usr/local/bin/docker-entrypoint.sh
RUN chmod 755 /usr/local/bin/docker-entrypoint.sh /app/bin/the_gathering

EXPOSE 4000
VOLUME ["/data"]
# BusyBox wget ships with Alpine, so no extra healthcheck binary is needed.
HEALTHCHECK --interval=30s --timeout=5s --start-period=30s --retries=3 \
  CMD wget -qO /dev/null "http://127.0.0.1:${PORT}/api/health" || exit 1
ENTRYPOINT ["docker-entrypoint.sh"]
CMD ["/app/bin/the_gathering", "start"]
