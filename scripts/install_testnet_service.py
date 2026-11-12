#!/usr/bin/env python3
"""Install the user service template. Does not enable, start or create a session."""
from pathlib import Path
import subprocess


def quote(value):
    # systemd unit quoting, including specifiers (not shell quoting).
    return '"' + str(value).replace('\\', '\\\\').replace('"', '\\"').replace('%', '%%') + '"'


def main():
    root = Path(__file__).resolve().parents[1]
    binary = root / 'target/debug/trading-testnet'
    if not binary.is_file():
        raise SystemExit('Build trading-testnet before installing the service.')
    target = Path.home() / '.config/systemd/user/trading-testnet@.service'
    content = (root / 'deploy/trading-testnet@.service.in').read_text()
    if any(c in str(root) for c in '\n\r\\'):
        raise SystemExit('Unsupported newline or backslash in project path.')
    content = content.replace('@PROJECT@', str(root).replace('%', '%%')).replace('@BINARY@', quote(binary))
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(content)
    subprocess.run(['systemd-analyze', '--user', 'verify', str(target)], check=True)
    subprocess.run(['systemctl', '--user', 'daemon-reload'], check=True)
    print(f'Installed {target}. No session started or enabled.')


if __name__ == '__main__':
    main()
