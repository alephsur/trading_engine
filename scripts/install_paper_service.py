#!/usr/bin/env python3
"""Install PAPER service. Does not start, enable, or reset an account."""
from pathlib import Path
import subprocess


def quote(value):
    # systemd unit quoting, including specifiers (not shell quoting).
    return '"' + str(value).replace('\\', '\\\\').replace('"', '\\"').replace('%', '%%') + '"'


def main():
    root = Path(__file__).resolve().parents[1]
    binary = root / 'target/debug/trading-server'
    if not binary.is_file():
        raise SystemExit('Build trading-server before installing the service.')
    target = Path.home() / '.config/systemd/user/trading-paper.service'
    content = (root / 'deploy/trading-paper.service.in').read_text()
    if any(c in str(root) for c in '\n\r\\'):
        raise SystemExit('Unsupported newline or backslash in project path.')
    content = content.replace('@PROJECT@', str(root).replace('%', '%%')).replace('@LAUNCHER@', quote(root / 'scripts/run_paper_service.sh'))
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(content)
    subprocess.run(['systemd-analyze', '--user', 'verify', str(target)], check=True)
    subprocess.run(['systemctl', '--user', 'daemon-reload'], check=True)
    print(f'Installed {target}. No session started or enabled.')


if __name__ == '__main__':
    main()
