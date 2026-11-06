#!/usr/bin/env bash
# wsl-vmware-lab 호스트(WSL)에서 빌드 VM으로 코드를 보내거나 명령을 실행한다.
#   bash lab/vm.sh sync            # 이 저장소를 VM의 ~/podobank_rust 로 복사
#   bash lab/vm.sh run '<명령>'     # VM에서 명령 실행 (~/podobank_rust 에서)
# 접속 정보는 실습의 인벤토리(inventory.json)에서 읽는다.
set -euo pipefail
repo=$(cd "$(dirname "$0")/.." && pwd)
host=${PODOBANK_VM:-podobank-rust}
inventory=${LAB_INVENTORY:-$HOME/.config/wsl-vmware-lab/inventory.json}

read -r addr user key extra < <(python3 - "$inventory" "$host" <<'PY'
import json, sys
h = json.load(open(sys.argv[1]))["all"]["hosts"][sys.argv[2]]
print(h["ansible_host"], h["ansible_user"], h["ansible_ssh_private_key_file"], h["ansible_ssh_common_args"])
PY
)
ssh_cmd=(ssh -i "$key" -o BatchMode=yes)
eval "ssh_cmd+=($extra)"

case "${1:-}" in
  sync)
    # rsync -e는 따옴표 안의 공백(ProxyCommand)을 다루지 못해 ssh를 감싼 임시 스크립트를 쓴다.
    wrapper=$(mktemp)
    trap 'rm -f "$wrapper"' EXIT
    printf '#!/usr/bin/env bash\nexec %s "$@"\n' "$(printf '%q ' "${ssh_cmd[@]}")" > "$wrapper"
    chmod 700 "$wrapper"
    rsync -a --delete --exclude target --exclude .git --exclude node_modules --exclude .venv --exclude var \
      -e "$wrapper" "$repo/" "$user@$addr:podobank_rust/"
    ;;
  run)
    shift
    "${ssh_cmd[@]}" "$user@$addr" "cd ~/podobank_rust && $*"
    ;;
  *)
    echo "usage: $0 sync | run '<command>'" >&2
    exit 2
    ;;
esac
