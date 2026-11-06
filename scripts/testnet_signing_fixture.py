"""Offline golden vectors. Run with hyperliquid-python-sdk==0.24.0 in an isolated venv.
Only a public dummy key is used. Never contacts an exchange.
"""
import json
from pathlib import Path
from importlib.metadata import version
from eth_account import Account
from hyperliquid.utils.signing import sign_l1_action

nonce = 1700000000000
expiry = nonce + 10000
key = Account.from_key(bytes.fromhex('00' * 31 + '01'))
# Explicit field insertion order matches the exchange wire protocol.
parent = dict(a=3, b=True, p='100.01', s='0.2', r=False,
              t=dict(limit=dict(tif='Ioc')), c='0x'+'11'*16)
stop = dict(a=3, b=False, p='97.02', s='0.2', r=True,
            t=dict(trigger=dict(isMarket=True, triggerPx='99', tpsl='sl')), c='0x'+'22'*16)
target = dict(a=3, b=False, p='100.94', s='0.2', r=True,
              t=dict(trigger=dict(isMarket=True, triggerPx='103', tpsl='tp')), c='0x'+'33'*16)
actions = [dict(type='order', orders=[parent, stop, target], grouping='normalTpsl'),
           dict(type='cancelByCloid', cancels=[dict(asset=3, cloid=parent['c'])])]
cases = [dict(action=action, nonce=nonce, expires=expiry,
              signature=sign_l1_action(key, action, None, nonce, expiry, False)) for action in actions]
Path('crates/testnet/tests/fixtures/signing.json').write_text(json.dumps(dict(source='hyperliquid-python-sdk '+version('hyperliquid-python-sdk'), cases=cases), indent=2)+'\n')
