"""Read-only testnet discovery. Does not load a key or submit any order."""
import json
import subprocess

result = subprocess.run(['cargo', 'run', '--locked', '-p', 'trading-testnet', '--', 'markets'],
                        text=True, capture_output=True, timeout=180, check=True)
markets = json.loads(result.stdout)
assert [m['metadata']['name'] for m in markets] == ['BTC', 'ETH', 'SOL', 'HYPE']
assert len({m['asset_index'] for m in markets}) == 4
assert all(not m['metadata']['isDelisted'] and m['metadata']['szDecimals'] <= 6 for m in markets)
print('PASS: public testnet discovery, four allowed perpetuals, dynamic asset indices; no signed writes')
