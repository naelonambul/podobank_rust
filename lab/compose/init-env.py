"""Create local test credentials once without printing their values."""
import os
from pathlib import Path
import secrets

path = Path(__file__).with_name('.env')
try:
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
except FileExistsError:
    print('Existing lab/compose/.env preserved.')
else:
    with os.fdopen(fd, 'w') as stream:
        stream.write('TEST_DB_PASSWORD=' + secrets.token_hex(24) + '\n')
        stream.write('TEST_DB_ROOT_PASSWORD=' + secrets.token_hex(24) + '\n')
        stream.write('PYTHON_PORT=18001\nRUST_PORT=18101\n')
    print('Created lab/compose/.env (0600); credentials hidden.')
