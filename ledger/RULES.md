# Buku simulasi Rp1 juta — ATURAN (ditulis 2026-11-13, SEBELUM pick pertama; jangan diubah)

Ini latihan dengan uang bayangan, bukan nasihat investasi dan bukan bukti strategi. Pelaksana: `forward_test/ledger.py`
(tes: `test_ledger.py`). Hasil: `latest/ledger_1jt.md|json`. Pick tersimpan di `ledger/picks/pick_<hari sinyal>.json`.

## Sumber
Sinyal v1.1 (`signals_v11/`), yang masih DRAF (`FT11_SPEC.md`). Jadi buku ini LATIHAN. Hanya hari sinyal >= 2026-11-13 yang dipakai;
sinyal 12 Nov dan sebelumnya sudah diketahui sebelum aturan ini ditulis, jadi tidak dihitung.

## Aturan pick (semua mekanis)
1. Urutan kandidat: rasio asing 5 sesi (`ratio_5d`) terbesar lebih dulu. Satu ukuran saja; ini bukan rekomendasi.
2. Order: limit beli, berlaku 3 sesi, stop 2 ATR, target 4 ATR dari harga isi, maks tahan 10 sesi (sama dengan spec).
3. Ukuran posisi, ambil yang terkecil dari tiga batas, dibulatkan ke bawah dalam lot (1 lot = 100 lembar):
   a. nilai posisi maksimal 15% dari nilai buku (kerangka risiko START.md: 10–15% per saham),
   b. rugi bila kena stop (termasuk biaya) maksimal 2% dari nilai buku. Ini lebih longgar daripada usulan awal 0,5% karena
      0,5% (Rp5.000) tidak bisa dipenuhi satu lot pun dari kandidat tipikal pada modal sebesar ini. Angka ini ditetapkan Clo
      atas izin Bos ("atur kamu dulu") dan terbuka untuk diubah lewat versi baru,
   c. kas bebas (setelah dikurangi cadangan untuk order yang menunggu).
   Kalau hasilnya kurang dari 1 lot, kandidat dilewati dan alasannya dicatat.
4. Maksimal 4 slot: posisi terbuka ditambah order menunggu.
5. Biaya 0,7% pulang-pergi (0,35% per sisi). Itu asumsi spec; biaya IPOT sebenarnya belum diverifikasi. Tick harga IDX dan
   pengisian sebagian tidak dimodelkan (optimistis).
6. Hari tanpa sinyal (gerbang breadth tutup) = tidak ada order. Tidak ada pick manual dan tidak ada perubahan setelah hasil terlihat.
7. Nilai buku = kas + posisi terbuka pada penutupan terakhir.

## Pencatatan yang tidak bisa dibantah
- File pick hanya ditulis sekali dan tidak pernah ditimpa. Isinya `created_at` (UTC) dan commit git memberi cap waktu kedua.
- Pick yang dibuat setelah sesi pertama (hari bursa berikutnya) dibuka diberi tanda "terlambat" di laporan.

## Yang BELUM ada
- Batas rugi harian dan mingguan belum ditetapkan Bos, jadi belum ada pemutus otomatis.
- Di bawah 20 hari sinyal tidak ada klaim apa pun (EVAL_SPEC). Satu saham beruntung bisa menggerakkan seluruh hasil.
- Saran saham berbasis berita dari Clo (diskresi) TIDAK masuk buku ini; kalau dicatat, harus terpisah dan berlabel diskresi.
