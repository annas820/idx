# EVAL SPEC — Forward Test v1.0 (dikunci 2026-10-26, SEBELUM hasil pertama dilihat)

Dokumen ini menetapkan cara menilai. Tidak boleh diubah setelah hasil terlihat. Perubahan hanya lewat versi baru (EVAL_SPEC v1.1) dengan alasan tertulis dan tanggal, dan hasil lama tetap dilaporkan dengan spesifikasi lama.

## Unit analisis
- Unit utama: HARI SINYAL (rata-rata hasil semua sinyal pada tanggal itu). Unit sekunder: transaksi.
- Jumlah pengamatan independen = jumlah hari sinyal, bukan jumlah transaksi.

## Metrik (selalu dilaporkan semuanya)
- Return kotor dan bersih (biaya 0,7%); skenario biaya 1,0% sebagai uji sensitivitas.
- Expectancy bersih per transaksi dalam R (1R = 2 ATR jarak stop).
- Win rate, rata-rata menang/kalah, drawdown maksimum, rentetan rugi terpanjang.
- Terpisah: kualitas SINYAL (beli di open berikutnya) dan kualitas EKSEKUSI (hanya order terisi); jumlah order tidak terisi dilaporkan.
- Pembanding: kontrol acak tanggal yang sama, mekanisme dan biaya identik. Selisih sinyal minus kontrol dihitung PER HARI SINYAL.

## Aturan eksekusi simulasi (sudah di kode)
- Open <= limit: isi di open. Low <= limit: isi di limit. Berlaku 3 sesi.
- Stop dan target di candle yang sama: stop dianggap lebih dulu (konservatif). Gap melewati stop: keluar di open.
- Order dianggap terisi penuh (asumsi optimistis, dicatat; tidak memodelkan pengisian sebagian).

## Pemicu dan tingkat klaim
- >= 30 sinyal: evaluasi awal, jawabannya hanya "layak diteliti lagi / tidak".
- < 20 hari sinyal: hanya angka mentah per hari dan perbandingan dengan kontrol; TANPA klaim statistik.
- >= 20 hari sinyal: boleh bootstrap berkelompok per hari; laporkan selang kepercayaan.
- Klaim keunggulan menuntut: expectancy bersih > 0 dalam R, lebih baik dari kontrol, bertahan pada biaya 1,0%, tidak bergantung pada satu hari atau satu saham, dan konsisten di lebih dari satu kondisi pasar.
- Filter yang tidak menambah nilai dibanding kontrol dibuang lewat versi baru, bukan diam-diam.

## Larangan
- Tidak mengubah aturan sinyal, metrik, atau biaya setelah melihat hasil.
- Tidak memilih saham/tanggal terbaik secara selektif. Semua sinyal dilaporkan.
