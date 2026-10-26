# CLO news filter: reads the raw archive news/<YYYY-MM>.csv and writes news/digest.json + news/digest.md.
# Rule-based and deterministic (no model, no network): every number in the digest can be traced to a rule below.
# Raw archive is never modified. Usage: python3 forward_test/news_score.py news data/universe.csv [watchlist.txt]
import csv, glob, html, json, os, re, sys
from datetime import datetime, timezone, timedelta

WIB = timezone(timedelta(hours=7))
WINDOW_HOURS = 36        # digest covers this long before the newest headline
MIN_SCORE = 3            # below this a headline is left out of the digest
TOP_N = 25
CORROB_JACCARD = 0.45    # title-token overlap that counts as "same story"
CORROB_HOURS = 48

# Source tier follows START.md: 1 authority, 2 wire (Reuters/Bloomberg), 3 local media, 4 group/social.
# These are all local media. Tier 1/2 are not fetchable for free from CI, so authority is only *mentioned*
# in headlines (category `kebijakan`), never assumed from the outlet.
SOURCE_TIER = {
    'bloomberg_technoz': 3, 'cnbc_indonesia_market': 3, 'cnbc_indonesia_news': 3, 'cnbc_indonesia_research': 3,
    'kontan_investasi': 3, 'kontan_keuangan': 3, 'kontan_industri': 3, 'kontan_insight': 3, 'bisnis_market': 3,
}

# category -> (weight, regex). A headline gets each matching category's weight once.
CATEGORIES = {
    'kebijakan': (3, r'\b(BI[- ]?Rate|RDG|Bank Indonesia|OJK|MSCI|FTSE|KSEI|BEI|GWM|suku bunga|Menteri Keuangan|Menkeu|'
                     r'trading halt|suspensi|suspend|UMA|auto reject|ARB|moratorium|regulasi|aturan baru|PMK|Perppu|'
                     r'ekspor gas|kuota ekspor|larangan ekspor|bea keluar|DMO|royalti|pangkas ekspor)\b'),
    'global': (3, r'\b(The Fed|Fed|FOMC|minyak|Brent|WTI|Iran|Hormuz|Houthi|perang|tarif|sanksi|Wall Street|Nasdaq|'
                  r'S&P 500|Dow Jones|Treasury|OPEC|gencatan senjata)\b'),
    'pasar': (3, r'\b(IHSG|net sell|net buy|asing|bursa|big cap|saham|emiten|free float|rebalancing)\b'),
    'kurs': (3, r'\b(rupiah|kurs|yield|SBN|obligasi|surat utang)\b'),
    'kredit': (3, r'\b(Fitch|Moody\'?s|Pefindo|S&P Global Ratings|rating(?! bursa)|peringkat utang|default|gagal bayar|PKPU|pailit)\b'),
    'makro': (2, r'\b(inflasi|PDB|APBN|defisit|neraca perdagangan|cadangan devisa|likuiditas|'
                 r'pertumbuhan ekonomi|resesi|stagflasi)\b'),
    'korporasi': (2, r'\b(laba|rugi|dividen|buyback|akuisisi|rights? issue|IPO|tender offer|PKPU|gagal bayar|stock split|'
                     r'private placement|merger|divestasi|kuartal|kinerja keuangan|pailit|delisting|relisting)\b'),
    'komoditas': (2, r'\b(nikel|batu ?bara|CPO|sawit|tembaga|timah|bauksit|litium|LNG|gas|emas|larangan ekspor|'
                     r'hilirisasi|ekspor|impor)\b'),
}
# Roundups of other people's calls: kept (weight 1) but labelled as a hint, never a basis (tier 4 rule).
HINT = (1, r'\b(rekomendasi saham|saham pilihan|prediksi IHSG|arah IHSG|proyeksi IHSG)\b')
# Lifestyle / service noise: pushes the score under the cutoff on its own.
NOISE = (-6, r'\b(tips?|trik|cara|resep|zodiak|shio|ramalan|model|outfit|bridesmaid|Canva|WhatsApp|HP Anda|ponsel|'
             r'NIK|KTP|plat|angsuran|cicilan|diskon|promo|giveaway|kuliner|wisata|skincare|drakor|selebriti|TKA|'
             r'sertifikat halal|daftar lengkap harga emas Antam)\b')
# Company names that headlines use instead of the ticker. Deliberately short: only names unambiguous in Indonesian news.
ALIASES = [
    (r'\bBank Mandiri\b', ['BMRI']), (r'\bBCA\b|\bBank Central Asia\b', ['BBCA']), (r'\bBRI\b|\bBank Rakyat Indonesia\b', ['BBRI']),
    (r'\bBNI\b|\bBank Negara Indonesia\b', ['BBNI']), (r'\bTelkom(sel)?\b', ['TLKM']), (r'\bAstra\b(?! Agro| Otoparts| Graphicia)', ['ASII']),
    (r'\bIndofood\b', ['INDF', 'ICBP']), (r'\bAdaro\b', ['AADI', 'ADRO']), (r'\bVale\b', ['INCO']), (r'\bAntam\b', ['ANTM']),
    (r'\bGoTo\b|\bGojek\b', ['GOTO']), (r'\bBukalapak\b', ['BUKA']), (r'\bUnited Tractors\b', ['UNTR']),
]
TICKER_STOP = {'IHSG', 'OPEC', 'MSCI', 'FOMC', 'KSEI', 'FTSE', 'BUMN', 'IPOT', 'OJKS', 'APBN', 'PMKS'}
WATCH_BONUS = 2
TICKER_WEIGHT = 2
CORROB_BONUS = 2         # per extra independent source, capped at 2 extra sources
STOPWORDS = set('yang dan di ke dari untuk pada dengan akan atau ini itu juga saat usai jadi bakal hingga sebagai '
                'dalam oleh tak tidak lebih ada kini hari tahun pekan lalu baru bisa agar kata ungkap'.split())


def parse_time(s):
    for fmt in ('%Y-%m-%dT%H:%M%z', '%Y-%m-%dT%H:%M:%S%z'):
        try:
            return datetime.strptime(s, fmt)
        except (ValueError, TypeError):
            pass
    try:
        return datetime.fromisoformat(s)
    except (ValueError, TypeError):
        return None


def load_universe(path):
    out = set()
    if path and os.path.exists(path):
        with open(path, newline='', encoding='utf-8') as f:
            rows = (l for l in f if not l.startswith('#'))
            for r in csv.DictReader(rows):
                if r.get('symbol'):
                    out.add(r['symbol'].strip().upper())
    return out


def load_watch(path):
    if not path or not os.path.exists(path):
        return set()
    with open(path, encoding='utf-8') as f:
        return {l.split('#')[0].strip().upper() for l in f if l.split('#')[0].strip()}


def tokens(title):
    words = re.findall(r'[a-z0-9]+', title.lower())
    return {w for w in words if len(w) >= 4 and w not in STOPWORDS}


def jaccard(a, b):
    return len(a & b) / len(a | b) if a and b else 0.0


def classify(title, universe, watch):
    """Return (score, categories, tickers, flags) for one headline. Pure function: the tests call it directly."""
    cats, score, flags = [], 0, []
    for name, (w, rx) in CATEGORIES.items():
        if re.search(rx, title, re.I):
            cats.append(name); score += w
    if re.search(HINT[1], title, re.I):
        flags.append('petunjuk'); score += HINT[0]
    if re.search(NOISE[1], title, re.I):
        flags.append('noise'); score += NOISE[0]
    found = {t for t in re.findall(r'\b[A-Z]{4}\b', title) if t in universe and t not in TICKER_STOP}
    for rx, syms in ALIASES:
        if re.search(rx, title):
            found.update(s for s in syms if not universe or s in universe)
    tickers = sorted(found)
    if tickers:
        score += TICKER_WEIGHT; cats.append('emiten')
    hit = [t for t in tickers if t in watch]
    if hit:
        score += WATCH_BONUS; flags.append('watchlist')
    return score, cats, tickers, flags


def load_rows(news_dir):
    rows = []
    for path in sorted(glob.glob(os.path.join(news_dir, '20??-??.csv'))):
        with open(path, newline='', encoding='utf-8') as f:
            for r in csv.DictReader(f):
                t = parse_time(r.get('published') or '') or parse_time(r.get('fetched') or '')
                if t is None or not r.get('title'):
                    continue
                r['_t'] = t.astimezone(WIB)
                r['title'] = html.unescape(r['title'])   # feeds ship "&amp;"; the raw archive is left as fetched
                rows.append(r)
    return rows


def build(rows, universe, watch):
    if not rows:
        return None
    newest = max(r['_t'] for r in rows)
    cutoff = newest - timedelta(hours=WINDOW_HOURS)
    recent = [r for r in rows if r['_t'] >= cutoff]
    for r in rows:
        r['_tok'] = tokens(r['title'])
    items = []
    for r in recent:
        score, cats, tickers, flags = classify(r['title'], universe, watch)
        # Same story in other sources within CORROB_HOURS (compared against the whole archive, not just the window).
        others = {o['source'] for o in rows if o['source'] != r['source'] and abs((o['_t'] - r['_t']).total_seconds()) <= CORROB_HOURS * 3600
                  and jaccard(r['_tok'], o['_tok']) >= CORROB_JACCARD}
        if others and 'noise' not in flags:
            score += CORROB_BONUS * min(len(others), 2); flags.append('terkonfirmasi')
        items.append(dict(published=r['_t'].isoformat(timespec='minutes'), source=r['source'],
                          tier=SOURCE_TIER.get(r['source'], 4), title=r['title'], link=r['link'], score=score,
                          categories=cats, tickers=tickers, flags=flags, also_in=sorted(others)))
    kept = [i for i in items if i['score'] >= MIN_SCORE]
    kept.sort(key=lambda i: (i['score'], i['published']), reverse=True)  # best score first, newest first on ties
    return dict(schema='clo.news.digest.v1', generated_for=newest.isoformat(timespec='minutes'),
                window_hours=WINDOW_HOURS, min_score=MIN_SCORE, scanned=len(recent), kept=len(kept),
                dropped_noise=sum('noise' in i['flags'] for i in items), items=kept[:TOP_N],
                watchlist_hits=[i for i in kept if 'watchlist' in i['flags']][:10])


def render_md(d):
    if not d:
        return '# Digest berita\n\nArsip kosong.\n'
    L = ['# Digest berita (disaring otomatis)', '',
         f"Berita terbaru: {d['generated_for']} WIB | jendela {d['window_hours']} jam | dipindai {d['scanned']}, lolos {d['kept']}, "
         f"dibuang sebagai noise {d['dropped_noise']}.",
         'Skor = bobot kategori + kode saham + watchlist + konfirmasi lintas sumber. Tier 3 = media lokal. '
         '`petunjuk` = rekomendasi pihak lain, bukan dasar keputusan. Judul saja; buka tautan untuk isi.', '']
    def line(i):
        extra = ', '.join(i['flags'] + (['juga di ' + '/'.join(i['also_in'])] if i['also_in'] else []))
        tk = (' [' + ','.join(i['tickers']) + ']') if i['tickers'] else ''
        return f"- **{i['score']}** T{i['tier']} {i['published'][5:16].replace('T', ' ')} {i['source']}: {i['title']}{tk} " \
               f"({'/'.join(i['categories'])}{'; ' + extra if extra else ''}) <{i['link']}>"
    if d['watchlist_hits']:
        L += ['## Menyentuh watchlist'] + [line(i) for i in d['watchlist_hits']] + ['']
    L += ['## Teratas'] + [line(i) for i in d['items']] + ['']
    return '\n'.join(L)


def main(argv):
    news_dir = argv[1] if len(argv) > 1 else 'news'
    universe = load_universe(argv[2] if len(argv) > 2 else 'data/universe.csv')
    watch = load_watch(argv[3] if len(argv) > 3 else os.path.join(os.path.dirname(__file__), 'watchlist.txt'))
    d = build(load_rows(news_dir), universe, watch)
    with open(os.path.join(news_dir, 'digest.json'), 'w', encoding='utf-8') as f:
        json.dump(d, f, ensure_ascii=False, indent=1)
    with open(os.path.join(news_dir, 'digest.md'), 'w', encoding='utf-8') as f:
        f.write(render_md(d))
    print('digest:', (d or {}).get('scanned'), 'scanned,', (d or {}).get('kept'), 'kept')


if __name__ == '__main__':
    main(sys.argv)
