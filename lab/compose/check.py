"""Compare only the endpoints implemented in both backends, using real databases."""
import json
import sys
import time
import uuid
from urllib.error import HTTPError
from urllib.request import urlopen


def response(server, path):
    try:
        result = urlopen(f'http://{server}:8000{path}', timeout=15)
    except HTTPError as error:
        result = error
    with result:
        body = json.load(result)
        request_id = result.headers.get('X-Request-ID')
        assert str(uuid.UUID(request_id)) == request_id, 'invalid request ID'
        assert result.headers.get_content_type() == 'application/json'
        if 'request_id' in body:
            assert body['request_id'] == request_id, 'header/body request ID mismatch'
            body['request_id'] = '<uuid>'
        return result.status, body


def check(mode):
    health = {
        'healthy': (200, {'mariadb': 'ok', 'mongodb': 'ok'}),
        'mongo-down': (200, {'mariadb': 'ok', 'mongodb': 'down'}),
        'maria-down': (503, {'code': 'database_unavailable', 'message': '데이터베이스에 연결할 수 없습니다', 'request_id': '<uuid>'}),
    }[mode]
    cases = [('/api/v1/health', health)]
    if mode == 'healthy':
        missing = (404, {'code': 'not_found', 'message': '요청한 주소를 찾을 수 없습니다', 'request_id': '<uuid>'})
        cases += [('/api/v1/nope', missing), ('/nope', missing)]
    for path, expected in cases:
        python = response('python', path)
        rust = response('rust', path)
        assert python == expected, f'python {path}: {python!r}'
        assert rust == expected, f'rust {path}: {rust!r}'
        print(f'PASS {mode} {path}: Python == Rust ({expected[0]})', flush=True)


if __name__ == '__main__':
    mode = sys.argv[1]
    deadline = time.monotonic() + 60
    while True:
        try:
            check(mode)
            break
        except Exception:
            if time.monotonic() >= deadline:
                raise
            time.sleep(1)
