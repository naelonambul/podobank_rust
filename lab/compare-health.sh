#!/usr/bin/env bash
# 빌드 VM 안에서 실행: Python(8001)과 Rust(8101) 서버를 같은 테스트 DB로 띄우고 같은 요청의 응답을 비교한다.
#   bash lab/compare-health.sh
set -euo pipefail
cd "$(dirname "$0")/.."
set -a; . ~/.podobank-test.env; set +a
export MARIADB_URL="$TEST_MARIADB_URL" MONGODB_URL="$TEST_MONGODB_URL" MONGODB_DB=podobank_test
mkdir -p var/log

(cd backend && INSTANCE_NAME=py APP_LOG_PATH=../var/log/py-app.log EVENT_LOG_PATH=../var/log/py-events.log \
  .venv/bin/uvicorn app.main:create_app --factory --port 8001 > ../var/log/py-server.out 2>&1) &
py=$!
INSTANCE_NAME=rs APP_LOG_PATH=var/log/rs-app.log EVENT_LOG_PATH=var/log/rs-events.log \
  backend-rs/target/release/podobank-api --port 8101 > var/log/rs-server.out 2>&1 &
rs=$!
trap 'kill $py $rs 2>/dev/null || true' EXIT

for port in 8001 8101; do
  for _ in $(seq 1 50); do curl -s -o /dev/null "http://127.0.0.1:$port/api/v1/health" && break; sleep 0.2; done
done

show() {  # 상태 코드, X-Request-ID 유무, 본문(request_id 값은 가린다)
  local port=$1 path=$2
  curl -s -D - "http://127.0.0.1:$port$path" -o /tmp/body.$$ | tr -d '\r' \
    | awk 'NR==1{printf "%s ", $2} tolower($1)=="x-request-id:"{printf "x-request-id=yes "} tolower($1)=="content-type:"{printf "%s ", $2}'
  sed -E 's/"request_id":"[^"]+"/"request_id":"<uuid>"/' /tmp/body.$$; echo
  rm -f /tmp/body.$$
}

for path in /api/v1/health /api/v1/nope /nope; do
  echo "== GET $path"
  printf 'python: '; show 8001 "$path"
  printf 'rust:   '; show 8101 "$path"
done

echo "== stop MongoDB, then GET /api/v1/health"
sudo systemctl stop mongod
printf 'python: '; show 8001 /api/v1/health
printf 'rust:   '; show 8101 /api/v1/health
sudo systemctl start mongod
