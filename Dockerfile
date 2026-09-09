# syntax=docker/dockerfile:1.7

ARG AI_SWITCH_REPOSITORY=ijry/ai-switch
ARG AI_SWITCH_VERSION=latest

FROM debian:bookworm-slim AS download
ARG AI_SWITCH_REPOSITORY
ARG AI_SWITCH_VERSION
ARG TARGETARCH

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
      ca-certificates \
      curl \
      jq \
      unzip \
    && rm -rf /var/lib/apt/lists/*

RUN set -eux; \
    case "${TARGETARCH}" in \
      amd64) ARCH=x86_64 ;; \
      arm64) ARCH=aarch64 ;; \
      *) echo "Unsupported Docker architecture: ${TARGETARCH}" >&2; exit 1 ;; \
    esac; \
    case "${AI_SWITCH_REPOSITORY}" in \
      */*) ;; \
      *) echo "AI_SWITCH_REPOSITORY must be owner/repository" >&2; exit 1 ;; \
    esac; \
    if [ "${AI_SWITCH_VERSION}" = "latest" ]; then \
      VERSION="$(curl -fsSL --retry 3 --retry-delay 1 \
        "https://api.github.com/repos/${AI_SWITCH_REPOSITORY}/releases/latest" \
        | jq -r '.tag_name')"; \
    else \
      VERSION="${AI_SWITCH_VERSION}"; \
    fi; \
    case "${VERSION}" in \
      v*) ;; \
      *) VERSION="v${VERSION}" ;; \
    esac; \
    ARCHIVE="ai-switch-server_${VERSION}_linux-${ARCH}.zip"; \
    ASSET="$(curl -fsSL --retry 3 --retry-delay 1 \
      "https://api.github.com/repos/${AI_SWITCH_REPOSITORY}/releases/tags/${VERSION}" \
      | jq -r --arg name "${ARCHIVE}" \
        '.assets[] | select(.name == $name) | [.browser_download_url, .digest] | @tsv')"; \
    [ -n "${ASSET}" ] || { echo "Release ${VERSION} has no ${ARCHIVE} asset" >&2; exit 1; }; \
    DOWNLOAD_URL="$(printf '%s' "${ASSET}" | cut -f1)"; \
    DIGEST="$(printf '%s' "${ASSET}" | cut -f2)"; \
    case "${DIGEST}" in \
      sha256:*) EXPECTED="${DIGEST#sha256:}" ;; \
      *) echo "GitHub did not report a sha256 digest for ${ARCHIVE}" >&2; exit 1 ;; \
    esac; \
    curl -fL --retry 3 --retry-delay 1 -o "/tmp/${ARCHIVE}" "${DOWNLOAD_URL}"; \
    printf '%s  /tmp/%s\n' "${EXPECTED}" "${ARCHIVE}" | sha256sum -c -; \
    unzip -q "/tmp/${ARCHIVE}" -d /package; \
    test -x /package/ai-switch-server; \
    test -x /package/ai-switch-tsnet; \
    test -f /package/web/index.html

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
      ca-certificates \
      curl \
      libssl3 \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --home-dir /home/ai-switch --create-home ai-switch \
    && install -d -o ai-switch -g ai-switch -m 0750 /home/ai-switch/.ai-switch

COPY --from=download /package/ai-switch-server /app/ai-switch-server
COPY --from=download /package/ai-switch-tsnet /app/ai-switch-tsnet
COPY --from=download /package/web /app/web
COPY deploy/docker-entrypoint.sh /app/docker-entrypoint.sh
RUN chmod 0755 /app/ai-switch-server /app/ai-switch-tsnet /app/docker-entrypoint.sh

ENV HOME=/home/ai-switch \
    AI_SWITCH_HOST=0.0.0.0 \
    AI_SWITCH_PORT=19527 \
    AI_SWITCH_ALLOW_INSECURE_HTTP=1 \
    AI_SWITCH_STATIC_DIR=/app/web \
    AI_SWITCH_TSNET_PATH=/app/ai-switch-tsnet
USER ai-switch
EXPOSE 19527
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
  CMD curl -fsS http://127.0.0.1:19527/health >/dev/null || exit 1
VOLUME ["/home/ai-switch/.ai-switch"]
ENTRYPOINT ["/app/docker-entrypoint.sh"]
