# Run: python3 -m unittest forward_test.test_news_score   (no network, no third-party packages)
import os, sys, unittest
sys.path.insert(0, os.path.dirname(__file__))
import news_score as n

UNIVERSE = {'BMRI', 'BBCA', 'ICBP', 'INDF', 'AADI', 'ADRO', 'ASII', 'AALI', 'FILM', 'BUKA', 'INCO', 'ANTM', 'TLKM', 'BBRI'}
WATCH = {'AADI', 'INCO', 'INDF', 'ASII'}


def run(title):
    return n.classify(title, UNIVERSE, WATCH)


class Keep(unittest.TestCase):
    """Headlines that moved or could move the market on 26 Oct 2026 must clear MIN_SCORE."""
    CASES = [
        'OJK Perpanjang Buyback Tanpa RUPS hingga Juni 2027',
        'Dasco Respons MSCI: Indonesia Akan Tentukan Rating Bursa Sendiri',
        'Harga Minyak Turun, Pasar Waspadai Eskalasi Konflik Iran',
        'Rupiah Hari Ini Buka Perdagangan di Rp18.110/US$',
        'Indonesia Pangkas Ekspor Gas, Singapura Serius Kaji Energi Nuklir',
        'Fitch Turunkan Rating Obligasi Pos Indonesia ke Default',
        'Investor Asing Masih Jual Saham Rp8 T Walau IHSG Hijau',
        'Bank Mandiri Cetak Laba Rp45,5 Triliun hingga Kuartal III',
        'Grup Indofood Siapkan Anggaran Buyback Rp2,25 T',
    ]

    def test_kept(self):
        for t in self.CASES:
            with self.subTest(t):
                self.assertGreaterEqual(run(t)[0], n.MIN_SCORE)


class Drop(unittest.TestCase):
    CASES = [
        'Daftar Trik WhatsApp di 2026 yang Perlu UKM Ketahui dan Coba',
        'Ini Arti 16 Digit NIK di KTP, Ada Perbedaan Laki-laki dan Wanita',
        'Tabel KUR BRI Oktober 2026 Pinjaman Rp60 Juta, Angsuran & Syarat',
        'Daftar Lengkap Harga Emas Antam Hari Ini, Senin 26 Oktober 2026',
        'Anggota BPK Fathan Subchi Meninggal di Rusia',
        '12 Model Bridesmaid Satin yang Simpel Tapi Terlihat Mewah',
    ]

    def test_dropped(self):
        for t in self.CASES:
            with self.subTest(t):
                self.assertLess(run(t)[0], n.MIN_SCORE)


class Details(unittest.TestCase):
    def test_ticker_and_watchlist(self):
        s, cats, tickers, flags = run('Saham AADI Dibeli Asing Rp100 M')
        self.assertEqual(tickers, ['AADI'])
        self.assertIn('watchlist', flags)

    def test_alias_does_not_confuse_astra_agro(self):
        self.assertEqual(run('Astra Agro Lestari Cetak Laba')[2], [])
        self.assertEqual(run('Astra Naikkan Dividen')[2], ['ASII'])

    def test_stoplist_blocks_non_issuers(self):
        self.assertEqual(run('IHSG Menguat, OPEC Tahan Produksi')[2], [])

    def test_hint_is_flagged_not_trusted(self):
        s, cats, tickers, flags = run('Rekomendasi Saham Hari Ini, 26 Oktober 2026')
        self.assertIn('petunjuk', flags)

    def test_corroboration_across_sources(self):
        rows = [
            dict(published='2026-10-26T09:00+07:00', fetched='2026-10-26T09:05+07:00', source='bloomberg_technoz',
                 title='OJK Perpanjang Buyback Tanpa RUPS hingga Juni 2027', link='a'),
            dict(published='2026-10-26T09:20+07:00', fetched='2026-10-26T09:25+07:00', source='kontan_keuangan',
                 title='Buyback Tanpa RUPS Diperpanjang OJK hingga Juni 2027', link='b'),
        ]
        for r in rows:
            r['_t'] = n.parse_time(r['published']).astimezone(n.WIB)
        d = n.build(rows, UNIVERSE, WATCH)
        self.assertTrue(all('terkonfirmasi' in i['flags'] for i in d['items']))

    def test_empty_archive(self):
        self.assertIsNone(n.build([], UNIVERSE, WATCH))
        self.assertIn('kosong', n.render_md(None))


if __name__ == '__main__':
    unittest.main()
