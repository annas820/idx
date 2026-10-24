# CLO Forward Test v1.0 - signal generator (rules frozen; do not edit to improve results)
import json, sys, os, random, glob
import pandas as pd, numpy as np
SNAP = sys.argv[1]            # snapshots/<date>
HIST = sys.argv[2]            # hist/ directory with <SYMBOL>.csv
OUT  = sys.argv[3]            # signals/ directory
MIN_NET_PCT, BREADTH_MIN = 5.0, 0.40
LIMIT_ATR, STOP_ATR, TGT_ATR, VALID, HOLD, COST = 0.5, 2.0, 4.0, 3, 10, 0.007
d = json.load(open(os.path.join(SNAP, 'fnet_5d.json')))['data']
t = d['to']; rows = d['rows']
def feat(sym):
    f = os.path.join(HIST, sym + '.csv')
    if not os.path.exists(f): return None
    h = pd.read_csv(f); h['date'] = h['time'].str[:10]
    h = h[h.volume > 0].reset_index(drop=True)
    if len(h) < 60 or h.date.iloc[-1] != t: return None
    c, hi, lo = h.close, h.high, h.low
    pc = c.shift(1); tr = pd.concat([hi-lo, (hi-pc).abs(), (lo-pc).abs()], axis=1).max(axis=1)
    atr = tr.ewm(alpha=1/14, adjust=False, min_periods=14).mean().iloc[-1]
    return dict(close=float(c.iloc[-1]), atr=float(atr), ma50=float(c.rolling(50).mean().iloc[-1]))
F = {r['symbol']: feat(r['symbol']) for r in rows}
F = {k: v for k, v in F.items() if v}
breadth = sum(v['close'] > v['ma50'] for v in F.values()) / max(len(F), 1)
def plan(sym):
    v = F[sym]; lim = v['close'] - LIMIT_ATR * v['atr']
    return dict(symbol=sym, signal_date=t, close=v['close'], atr=round(v['atr'], 2), limit=round(lim, 2),
                valid_sessions=VALID, stop_atr=STOP_ATR, target_atr=TGT_ATR, hold=HOLD, cost=COST)
sig = []
if breadth >= BREADTH_MIN:
    for r in rows:
        s = r['symbol']
        if s in F and r['signal'] in ('accumulation', 'big_accumulation') and r['net_pct'] >= MIN_NET_PCT and r['foreign_net_value'] > 0:
            p = plan(s); p.update(broker_signal=r['signal'], net_pct=round(r['net_pct'], 2), foreign_net_value=r['foreign_net_value']); sig.append(p)
rng = random.Random(int(t.replace('-', '')))
pool = sorted(F); k = max(len(sig), 5) if breadth >= BREADTH_MIN else 5
ctrl = [plan(s) for s in rng.sample(pool, min(k, len(pool)))]
res = dict(version='CLO-FT-1.0', signal_date=t, breadth=round(breadth, 3), universe_with_data=len(F),
           regime_ok=breadth >= BREADTH_MIN, signals=sig, controls=ctrl)
os.makedirs(OUT, exist_ok=True)
json.dump(res, open(os.path.join(OUT, t + '.json'), 'w'), indent=1)
print(t, 'breadth', round(breadth, 3), 'universe', len(F), 'signals', len(sig), 'controls', len(ctrl))
