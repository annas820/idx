# Forward Test v1.1 (asing per saham) — DRAF, belum dikunci

Status: **DRAF sampai Bos menulis "kunci v1.1"**. Setelah dikunci, aturan di bawah tidak boleh diubah; perubahan hanya lewat v1.2
dengan alasan tertulis. Sinyal dicatat sebagai uji maju (forward) hanya sejak tanggal kunci. Hasil sebelum kunci hanya
latihan dan tidak dihitung.

v1.1 BERBEDA dari v1.0 (akumulasi broker IPOT, yang belum bisa jalan). Keduanya dievaluasi terpisah dan tidak dicampur.
Cara menilai mengikuti `EVAL_SPEC.md` (metrik, hari sinyal sebagai unit independen, kontrol acak, larangan memilih-milih).

## Sumber data
Satu file per hari bursa: `manual/Ringkasan Saham-YYYYMMDD.xlsx` (unduhan Bos dari idx.co.id). Kolom yang dipakai: harga
sebelumnya/tertinggi/terendah/penutupan, volume, nilai, Foreign Sell, Foreign Buy (lembar), saham beredar.
Net asing per hari = (Foreign Buy − Foreign Sell) × harga penutupan. Ini skala perkiraan (hitungan sendiri dibanding angka
resmi IDX menyimpang sekitar 5–25% per hari), dipakai hanya untuk urutan dan rasio, bukan nilai rupiah pasti.

## Aturan sinyal (hari sinyal D = hari bursa terakhir yang ada datanya)
1. Universe: 100 saham dengan total nilai transaksi 5 sesi terakhir terbesar, yang bertransaksi pada hari D.
2. Rasio asing 5 sesi = jumlah net asing 5 sesi ÷ jumlah nilai transaksi 5 sesi. Sinyal bila rasio >= +10%.
3. Konsistensi: net asing positif pada minimal 3 dari 5 sesi.
4. Gerbang breadth, DUA syarat sekaligus: (a) porsi saham universe dengan penutupan D lebih tinggi daripada penutupan 4 sesi
   sebelumnya >= 40%, dan (b) porsi saham universe yang naik pada hari D (penutupan > harga sebelumnya) >= 40%.
   Di bawah itu tidak ada sinyal (aturan keras Clo: breadth rendah = tidak ada entri baru).
   Catatan transparansi: syarat (b) ditambahkan SEBELUM kunci, setelah uji awal pada data 27 Okt menunjukkan syarat (a)
   saja (47%) masih terbuka padahal pasar turun dua hari berturut-turut dan hanya sekitar 11% saham naik. Alasan konseptual:
   ukuran 5 sesi terlambat bereaksi. Bukan penyetelan angka untuk menaikkan hasil.
5. Data minimal: 14 sesi (untuk ATR 14). Kurang dari itu status "data_kurang", tidak ada sinyal.

## Eksekusi simulasi (sama dengan v1.0)
Limit beli = close − 0,5 × ATR(14), berlaku 3 sesi, terisi jika low <= limit. Stop 2 ATR, target 4 ATR, maks tahan 10 sesi,
biaya 0,7% pulang-pergi. Kontrol: saham acak dari universe yang sama pada hari yang sama (seed tanggal), jumlah minimal 5.

## Catatan kejujuran
- Ambang 10%, 3 dari 5 sesi, dan 40% adalah titik awal yang ditetapkan tanpa bukti. Tidak disetel dari data.
- Belum terbukti ada keunggulan. Filter yang tidak menambah nilai dibanding kontrol dibuang lewat v1.2.
- Sinyal satu hari saling berkorelasi: dihitung sebagai satu kelompok.
