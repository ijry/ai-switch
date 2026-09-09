#!/bin/sh
set -eu

if [ -z "${AI_SWITCH_TOKEN:-}" ]; then
  if [ -r /dev/urandom ] && command -v openssl >/dev/null 2>&1; then
    AI_SWITCH_TOKEN="$(openssl rand -hex 32)"
  else
    AI_SWITCH_TOKEN="docker-$(date +%s)-$(od -An -N32 -tx1 /dev/urandom | tr -d ' \n')"
  fi
  echo "AI_SWITCH_TOKEN was not set; generated a container-local token: ${AI_SWITCH_TOKEN}"
fi

if [ -z "${AI_SWITCH_HOST:-}" ]; then
  AI_SWITCH_HOST=0.0.0.0
fi
if [ -z "${AI_SWITCH_PORT:-}" ]; then
  AI_SWITCH_PORT=19527
fi
if [ -z "${AI_SWITCH_STATIC_DIR:-}" ]; then
  AI_SWITCH_STATIC_DIR=/app/web
fi
export AI_SWITCH_HOST AI_SWITCH_PORT AI_SWITCH_STATIC_DIR AI_SWITCH_TOKEN

exec /app/ai-switch-server