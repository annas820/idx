# CLO news archive: appends new headlines from RSS feeds to news/<YYYY-MM>.csv (de-duplicated by link)
import csv, os, sys, urllib.request, xml.etree.ElementTree as ET
from datetime import datetime, timezone, timedelta
from email.utils import parsedate_to_datetime
FEEDS = {
    'bloomberg_technoz': 'https://www.bloombergtechnoz.com/rss',
    'cnbc_indonesia_market': 'https://www.cnbcindonesia.com/market/rss',
}
if len(sys.argv) > 2: FEEDS = dict(a.split('=', 1) for a in sys.argv[2:])
OUT = sys.argv[1]; WIB = timezone(timedelta(hours=7))
os.makedirs(OUT, exist_ok=True)
now = datetime.now(WIB); path = os.path.join(OUT, now.strftime('%Y-%m') + '.csv')
seen = set()
if os.path.exists(path):
    with open(path, newline='', encoding='utf-8') as f:
        seen = {r['link'] for r in csv.DictReader(f)}
new = []
for src, url in FEEDS.items():
    try:
        req = urllib.request.Request(url, headers={'User-Agent': 'Mozilla/5.0 (clo-news-archive)'})
        root = ET.fromstring(urllib.request.urlopen(req, timeout=30).read())
    except Exception as e:
        print('FAIL', src, e); continue
    n = 0
    for it in root.iter('item'):
        link = (it.findtext('link') or '').strip(); title = (it.findtext('title') or '').strip()
        if not link or not title or link in seen: continue
        try: pub = parsedate_to_datetime(it.findtext('pubDate')).astimezone(WIB).isoformat(timespec='minutes')
        except Exception: pub = ''
        seen.add(link); new.append(dict(fetched=now.isoformat(timespec='minutes'), published=pub, source=src, title=title, link=link)); n += 1
    print('OK', src, n, 'new')
if new:
    write_header = not os.path.exists(path)
    with open(path, 'a', newline='', encoding='utf-8') as f:
        w = csv.DictWriter(f, fieldnames=['fetched', 'published', 'source', 'title', 'link'])
        if write_header: w.writeheader()
        w.writerows(new)
print('total new', len(new), '->', path)
