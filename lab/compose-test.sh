#!/usr/bin/env bash
# 이 Compose 프로젝트의 테스트 DB만 중단합니다. 정상/장애/복구 응답을 비교합니다.
set -euo pipefail
cd "$(dirname "$0")/.."
compose=(docker compose --env-file lab/compose/.env)
[ -f lab/compose/.env ] || { echo 'Create lab/compose/.env first; see README.md.' >&2; exit 1; }
check() { "${compose[@]}" exec -T python python /checks/check.py "$1"; }
restore() { "${compose[@]}" start mariadb mongodb >/dev/null; }
"${compose[@]}" up -d --build --wait --wait-timeout 180
trap restore EXIT
check healthy
"${compose[@]}" stop mongodb
check mongo-down
"${compose[@]}" start --wait mongodb
check healthy
"${compose[@]}" stop mariadb
check maria-down
"${compose[@]}" start --wait mariadb
check healthy
echo 'PASS: normal, missing routes, database failures and recovery'
