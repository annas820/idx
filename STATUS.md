# STATUS — kondisi terakhir (Selasa 2026-10-27, ~15:50 WIB)
Baca setelah START.md. Perbarui tiap ada perubahan besar.

## Yang berjalan
- Arsip berita per jam (`news/`, `news/digest.md`) jalan normal.
- Robot harian (`daily.yml`, v3, cron 10:30 UTC hari kerja) jalan, tapi data broker IPOT gagal.
- `diag.yml` kini manual saja (jadwal dihapus 28 Okt: probe 27 Okt menunjukkan IPOT 502 di semua jam). Log tetap di `diag/`.
- Jalur data manual (28 Okt): Bos mengunduh `Ringkasan Saham-YYYYMMDD.xlsx` dari idx.co.id tiap sore dan mengunggahnya ke `manual/` (cukup file itu; nama asli IDX). `forward_test/ringkasan_ingest.py` menormalkan ke `data/ringkasan/`, lalu `forward_test/ft11_signal.py` menulis `signals_v11/`. Aturan di `forward_test/FT11_SPEC.md` masih DRAF sampai Bos menulis "kunci v1.1"; sinyal ATR butuh 14 sesi (baru 5, isi mundur 9 hari lagi atau tunggu).
- Gerbang kesehatan data (`forward_test/health.py`) jalan di `daily.yml`, hasilnya `latest/health.md` dan `latest/health.json`. Hanya melaporkan; tidak mengubah aturan Forward Test v1.0.
- Pengerasan (28 Okt): installer tidak lagi diunduh lewat `curl | sh` dari jaringan; dipakai `install.sh` milik repo ini dengan versi terkunci v0.8.1 (checksum diverifikasi). `actions/checkout` dikunci ke SHA. Ada batas waktu tiap job, `concurrency` di daily, dan push daily otomatis diulang bila bentrok dengan news-bot.

## Masalah utama: data broker IPOT
- Kamis 22 dan Jumat 23 Okt berhasil. Sabtu 24, Senin 26, Selasa 27 gagal.
- Penyebab sebenarnya (dari log diag): HTTP 502 nginx di endpoint IPOT (brokersummary dan corporateaction) dari tiga IP runner. Beranda IPOT normal (200). Pesan "tidak ada sesi transaksi" dari `bandar screen` menyesatkan, itu error yang tertelan.
- Berarti: sisi server IPOT atau blokir untuk cloud, bukan kode kita.
- Akibat: tidak ada `fnet_5d`, `hist/`, `signals/`. Forward Test v1.0 BELUM bisa mulai. Jangan klaim sinyal.

## Jalur yang sudah dicoba dan DITUTUP
- Kurir skrip dari komputer lokal ke idx.co.id: 403 (Cloudflare). Jangan dilanjutkan.
- Sectors.app API: hanya paket Insider ($59/bulan). Gratis = tanpa API. Jangan daftar quest.
- daily v4 (retry + cron cadangan): ditarik atas permintaan Bos, tidak dipasang. Cari penyebab dulu, jangan asal ganti.

## Keputusan yang masih menggantung
1. Bila IPOT tetap 502: (a) tunggu pulih, (b) Bos unduh manual `stocksummary_YYYYMMDD.json` dari Chrome ke GitHub (~2 menit per hari; memberi net asing per saham; itu sinyal berbeda, jadi versi v1.1, jangan dicampur dengan v1.0), atau (c) jalan tanpa data per saham (harga, breadth, berita tersaring, arus asing level pasar).
2. Batas rugi harian dan mingguan belum ditetapkan Bos (usulan awal: risiko 0,5% per transaksi, ~1,5% harian, ~3% mingguan).
3. `forward_test/watchlist.txt` masih AADI/INCO/INDF; ganti dengan kandidat sinyal setelah data pulih.

## Yang harus dibaca berikutnya
- `diag/log_27_*.txt` (probe 15:00, 18:00, 20:00 WIB) dan `snapshots/2026-10-27/status.txt` (run 17:30 WIB).
- Setelah penyebab jelas: hapus jadwal probe di `diag.yml`.

## Catatan lain
- Backtest lama: tidak ada strategi stabil positif; sinyal broker tidak bisa diuji ke belakang (tanpa riwayat). Katakan ini bila relevan.
- 
