#!/usr/bin/env bash
# 이 프로젝트의 격리된 DB에만 마이그레이션과 비교용 계정을 추가한다.
set -euo pipefail
cd "$(dirname "$0")/.."
compose=(docker compose --env-file lab/compose/.env)
"${compose[@]}" up -d --build --wait --wait-timeout 180
"${compose[@]}" exec -T python python -m app.db.migrate
if [ "${1:-}" = "--faults" ]; then
    shift
    restore() { "${compose[@]}" start --wait mongodb >/dev/null; }
    trap restore EXIT
    "${compose[@]}" stop mongodb
    "${compose[@]}" exec -T -e PYTHONPATH=/app python python /checks/faults.py "$@"
else
    "${compose[@]}" exec -T -e PYTHONPATH=/app python python /checks/business.py "$@"
fi
