# STATUS — kondisi terakhir (Kamis 2026-11-12, ~13:00 WIB)
Baca setelah START.md. Perbarui tiap ada perubahan besar. `latest/health.md` memperingatkan bila file ini lebih tua dari 3 hari.

## Yang berjalan
- Arsip berita per jam (`news/`, `news/digest.md`) jalan, dengan saringan deterministik (`forward_test/news_score.py`, ada tes). Hanya 4 dari 7 sumber hidup: `kontan_investasi`, `kontan_industri` (sejak 5 Nov) dan `bisnis_market` (selalu) dijawab HTTP 403 dari server GitHub. Job tetap melapor sukses; jumlah sumber hidup kini tampil di `latest/health.md`.
- Robot harian (`daily.yml`, cron 17:30 WIB hari kerja): harga, kalender, berita jalan. Arus asing resmi dan daftar indeks tidak ada (idx.co.id memblokir jaringan cloud).
- Data broker IPOT putus-sambung (HTTP 502 dari server GitHub): berhasil 22–23 Okt dan 2–9 Nov, gagal 24–30 Okt dan 10–11 Nov. Pesan "no session with trades" menyesatkan, itu 502 yang tertelan.
- Forward Test v1.0 (BEKU): berjalan 6 hari sinyal dan menghasilkan 0 sinyal karena breadth MA50 hanya 7–9% (batas 40%). Gerbang sudah dihitung ulang dan benar, bukan bug.
- Jalur manual: Bos mengunggah `Ringkasan Saham-YYYYMMDD.xlsx` ke `manual/` tiap sore. `ringkasan_ingest.py` -> `data/ringkasan/`, `ft11_signal.py` -> `signals_v11/`. v1.1 masih DRAF (`FT11_SPEC.md`) sampai Bos menulis "kunci v1.1"; hasilnya hanya latihan.
- Penilai hasil (baru, 12 Nov): `forward_test/ft_eval.py` memutar ulang rencana v1.1 dan kontrolnya, tulis `latest/eval_v11.md`. Stop/target dari harga isi. Hanya melaporkan, di bawah 20 hari sinyal tanpa klaim.
- Gerbang kesehatan (`forward_test/health.py`): `latest/health.md|json`, kini juga sumber berita hidup dan umur STATUS.md.
- Pengerasan: installer terkunci v0.8.1, checkout dikunci ke SHA, batas waktu tiap job, push diulang bila bentrok.

## Tidak mengganggu briefing
- Workflow `ownership data` gagal sejak 2 Nov. Rancangannya: cermin daftar KSEI >1% butuh runner sendiri (idx.co.id memblokir cloud) dan sengaja gagal bila data >45 hari.
- `ft_signal.py` ditulis ulang tiap hari dari teks di dalam `daily.yml` (isi identik). Mengedit file di repo tidak ada gunanya; sumber aslinya di `daily.yml`.

## Jalur yang sudah dicoba dan DITUTUP
- Kurir skrip dari komputer lokal ke idx.co.id: 403 (Cloudflare).
- Sectors.app API: berbayar.
- daily v4 (retry + cron cadangan): ditarik atas permintaan Bos.

## Keputusan yang menunggu Bos
1. "Kunci v1.1" (saran Clo: tunggu penilai punya beberapa hari sinyal yang bisa dinilai).
2. Batas rugi harian dan mingguan belum ditetapkan (usulan awal: risiko 0,5% per transaksi, ~1,5% harian, ~3% mingguan). Kerangka risiko belum lengkap.
3. `forward_test/watchlist.txt` (AADI, INCO, INDF, ASII) hanya memberi bonus skor pada berita; bukan daftar kandidat harian. Ganti dengan saham yang Bos pegang atau pantau.
4. Bot Telegram: belum diputuskan.

## Belum terpecahkan
- IPOT 502 dan blokir idx.co.id dari server GitHub. Tiga sumber berita 403 belum ada perbaikan yang teruji.

## Catatan
- Backtest lama: tidak ada strategi stabil positif; sinyal broker tidak bisa diuji ke belakang. Katakan ini bila relevan.
