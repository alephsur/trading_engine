#!/usr/bin/env python3
"""Download five prior 30-day windows; abort rather than fill any missing candles/rates."""
import datetime as dt
import hashlib
import json
from pathlib import Path
import time
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / 'artifacts/prior-30d-2026-11-11'
API = 'https://api.hyperliquid.xyz/info'
HOUR, DAY = 3600000, 86400000
END = int(dt.datetime(2026, 10, 12, tzinfo=dt.timezone.utc).timestamp()*1000)
START = END - 150*DAY
SYMBOLS = ('BTC','ETH','SOL','HYPE')

def post(payload):
    for attempt in range(4):
        try:
            req=urllib.request.Request(API, data=json.dumps(payload).encode(), headers={'Content-Type':'application/json'})
            with urllib.request.urlopen(req,timeout=45) as r:
                result=json.load(r)
            time.sleep(.3)
            return result
        except (OSError, ValueError):
            if attempt == 3: raise
            time.sleep(2**attempt)

def save(path, value):
    raw=json.dumps(value,sort_keys=True,separators=(',',':')).encode()
    if path.exists():
        raise FileExistsError(path)
    path.write_bytes(raw)
    return hashlib.sha256(raw).hexdigest()

def iso(t):
    return dt.datetime.fromtimestamp(t/1000,dt.timezone.utc).isoformat()

def main():
    OUT.mkdir(parents=True,exist_ok=True)
    meta=post({'type':'meta'})
    bars, rates = {}, {}
    for coin in SYMBOLS:
        rows=post({'type':'candleSnapshot','req':{'coin':coin,'interval':'1h','startTime':START-100*HOUR,'endTime':END-1}})
        rows=sorted([r for r in rows if START-100*HOUR <= r['t'] < END and r['T']<END],key=lambda r:r['t'])
        assert [r['t'] for r in rows] == list(range(START-100*HOUR,END,HOUR)), f'gaps or duplicates: {coin}'
        bars[coin]=rows
        funding=[];cursor=START
        while cursor<END:
            page=post({'type':'fundingHistory','coin':coin,'startTime':cursor,'endTime':END-1})
            if not page: break
            funding.extend(page)
            next_cursor=max(r['time'] for r in page)+1
            assert next_cursor>cursor
            cursor=next_cursor
        funding=sorted([r for r in funding if START<=r['time']<END],key=lambda r:r['time'])
        assert [r['time']//HOUR*HOUR for r in funding]==list(range(START,END,HOUR)),f'funding gaps or duplicates: {coin}'
        rates[coin]=funding
        print(coin,len(rows),'candles;',len(funding),'funding rates',flush=True)
    manifest={'source':API,'interval':'1h','windows':[],'missing_15m_reason':'Insufficient public API retention for preceding full nonoverlapping 30-day window','current_metadata_used':True}
    for number in range(5):
        start=START+number*30*DAY;end=start+30*DAY
        name=iso(start)[:10]+'_'+iso(end)[:10]
        folder=OUT/name;folder.mkdir()
        hashes={}
        hashes['metadata.json']=save(folder/'metadata.json',meta)
        for coin in SYMBOLS:
            subset=[r for r in bars[coin] if start-100*HOUR<=r['t']<end]
            assert len(subset)==820
            filename=f'{coin}-1h.json';hashes[filename]=save(folder/filename,subset)
            subset=[r for r in rates[coin] if start<=r['time']<end]
            assert len(subset)==720
            filename=f'{coin}-funding.json';hashes[filename]=save(folder/filename,subset)
        manifest['windows'].append({'directory':name,'start_utc':iso(start),'end_exclusive_utc':iso(end),'candles_per_asset_including_warmup':820,'funding_rates_per_asset':720,'missing_candles':0,'missing_funding':0,'sha256':hashes})
    save(OUT/'coverage.json',manifest)
    print('Saved',len(manifest['windows']),'complete windows',flush=True)
if __name__=='__main__': main()
