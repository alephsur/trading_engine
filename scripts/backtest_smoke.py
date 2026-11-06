#!/usr/bin/env python3
"""Offline CLI acceptance using synthetic data; never a profitability demonstration."""
import copy
import json
import pathlib
import subprocess
import tempfile
from decimal import Decimal

ROOT = pathlib.Path(__file__).resolve().parents[1]
START = 6_000_000
END = START + 180 * 60_000
SYMBOLS = ('BTC', 'ETH', 'SOL', 'HYPE')


def price(n):
    phase = n % 40
    return Decimal('100') + Decimal(min(phase, 40 - phase)) / 10


def generate():
    records = [{'available_at': 0, 'event': {'kind': 'metadata', 'markets': [
        {'asset_index': i, 'metadata': {'name': s, 'szDecimals': 3, 'maxLeverage': 10}}
        for i, s in enumerate(SYMBOLS)]}}]
    for symbol in SYMBOLS:
        for n in range(280):
            opened, closed = price(n - 1), price(n)
            at = (n + 1) * 60_000
            if at >= END:
                continue
            records.append({'available_at': at, 'event': {'kind': 'candle', 'candle': {
                't': n * 60_000, 'T': at - 1, 's': symbol, 'i': '1m',
                'o': str(opened), 'h': str(max(opened, closed) + Decimal('.05')),
                'l': str(min(opened, closed) - Decimal('.05')), 'c': str(closed), 'v': '1000', 'n': 10}}})
        for at in range(START, END, 5000):
            mid = price(at // 60_000)
            records.append({'available_at': at, 'event': {'kind': 'book', 'book': {
                'coin': symbol, 'time': at, 'levels': [
                    [{'px': str(mid - Decimal('.005')), 'sz': '1000', 'n': 10}],
                    [{'px': str(mid + Decimal('.005')), 'sz': '1000', 'n': 10}]]}}})
            records.append({'available_at': at, 'event': {'kind': 'oracle', 'symbol': symbol,
                                                          'observed_at': at, 'price': str(mid)}})
            if at % 3_600_000 == 0:
                records.append({'available_at': at, 'event': {'kind': 'funding', 'symbol': symbol,
                                                              'hour': at, 'rate': '0.0001'}})
    records.sort(key=lambda r: r['available_at'])
    strategies = json.loads((ROOT / 'config/strategies.example.json').read_text())
    second = copy.deepcopy(strategies)
    second['version'] = 'synthetic-alternative'
    second['mean_reversion']['atr_multiple'] = '3'
    config = {'train_start': START, 'validation_start': START + 60 * 60_000,
              'test_start': START + 120 * 60_000, 'end': END, 'interval': '1m',
              'latency_ms': 1000, 'minimum_training_trades': 1,
              'risk': json.loads((ROOT / 'config/paper.example.json').read_text()),
              'candidates': [strategies, second]}
    return {'version': 1, 'source': 'SYNTHETIC_CLI_TEST_ONLY', 'capture_start': START,
            'capture_end': END, 'records': records}, config


def main():
    data, config = generate()
    with tempfile.TemporaryDirectory(prefix='trading-backtest-') as temp:
        root = pathlib.Path(temp)
        dataset, settings = root / 'data.json', root / 'config.json'
        dataset.write_text(json.dumps(data))
        settings.write_text(json.dumps(config))
        command = ['cargo', 'run', '--locked', '--quiet', '-p', 'trading-backtest', '--', 'run',
                   str(dataset), str(settings)]
        for name in ('first', 'second'):
            subprocess.run(command + [str(root / f'{name}.json')], cwd=ROOT, check=True)
        original_bytes = (root / 'first.json').read_bytes()
        assert original_bytes == (root / 'second.json').read_bytes()
        report = json.loads(original_bytes)
        assert report['selected_strategy_id'] is not None, report['selection_status']
        assert all(p['data_ready_ticks'] == p['ticks'] for p in report['training'])
        assert report['validation']['strategy_id'] == report['test']['strategy_id'] == report['selected_strategy_id']
        assert report['validation']['final_account']['activated_at'] == config['validation_start']
        assert report['test']['final_account']['activated_at'] == config['test_start']
        assert report['training'][0]['orders']
        # Alter only the holdout; all training results and the selected identity must remain identical.
        for record in data['records']:
            if record['available_at'] >= config['validation_start']:
                event = record['event']
                if event['kind'] == 'book':
                    for side in event['book']['levels']:
                        for level in side:
                            level['px'] = str(Decimal(level['px']) * 2)
                elif event['kind'] == 'oracle':
                    event['price'] = str(Decimal(event['price']) * 2)
        dataset.write_text(json.dumps(data))
        subprocess.run(command + [str(root / 'changed.json')], cwd=ROOT, check=True)
        changed = json.loads((root / 'changed.json').read_text())
        assert changed['training'] == report['training']
        assert changed['selected_strategy_id'] == report['selected_strategy_id']
        assert changed['dataset_hash'] != report['dataset_hash']
        # Existing reports cannot be overwritten by a later run.
        result = subprocess.run(command + [str(root / 'first.json')], cwd=ROOT, capture_output=True)
        assert result.returncode != 0
        assert (root / 'first.json').read_bytes() == original_bytes
        print('PASS: offline CLI, identical reports, frozen training selection, independent holdouts, no overwrite')


if __name__ == '__main__':
    main()
