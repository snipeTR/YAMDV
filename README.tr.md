# YAMDV — Yet Another Markdown Viewer

[English](README.md) | **Türkçe**

Windows için Rust ile sıfırdan yazılmış, yerel bir Markdown görüntüleyici ve düzenleyici. Önce
okuma; tamamen çevrimdışı, küçük (tek bir `yamdv.exe`, birkaç MiB çalışma belleği), çalışma
zamanı (runtime) gerektirmez, kurulum altyapısı (installer framework) kullanmaz.

![Ana hat kenar çubuğuyla bir belgeyi gösteren YAMDV](tests/golden/v1a/doc-sidebar-light.png)

**Sürüm 2.1.0.** Özellikler:

- **Okuma:** CommonMark + GFM (tablolar, görev listeleri, dipnotlar, bilgi kutuları (callout),
  `<details>`), 30'dan fazla kod sözcük çözümleyicisi (lexer), yerleşik matematik (makrolarla) ve
  Mermaid (flowchart, sequence, gantt, pie, state, class, ER, mindmap, `%%{init}%%`), SVG, GIF ve
  ICO dahil yerel görseller; 11 renk şeması, açık / koyu / Yüksek Karşıtlık, İngilizce ve Türkçe
  arayüz.
- **Gezinme:** Dosyalar / Ana hat / Sık kullanılanlar / Geçmiş kenar çubuğu, başlık paleti,
  zihin haritası, katlama düğmeleri, ipuçları, görsel / diyagram / tablo görüntüleyicileri.
- **Düzenleme:** WYSIWYG Canlı Düzenleme (`F2`) ve Bölünmüş Düzenleme (`F3`), tablo düzenleme,
  bul ve değiştir, görsel akış şeması düzenleme (bir düğüme veya kenara tıklayın: etiketini
  düzenleyin, `Delete` ile silin, `Tab` yeni düğüm ekler, bağlamak için bir düğümü diğerinin
  üzerine sürükleyin; her değişiklik kaynağa yazılır ve geri alınabilir).
- **Düzenleyici eklentileri:** VS Code ve Sublime Text için "YAMDV: Open preview",
  [`integrations/`](integrations/) klasöründe (yerel başlatıcılar; derleme ve kurulum adımları
  kendi README dosyalarında).
- **Araç çubuğu ve ayarlar:** simgeli bir araç çubuğu (*Ayarlar ▸ Araç çubuğu*: Kapalı /
  Önemliler / Hepsi hazır ayarları, öğeleri açma veya kapatma, ▲ / ▼ ile sıralama), kısayol
  düzenleyicili, aranabilir bir ayarlar paneli (⚙ veya `Ctrl+,`).
- **Dışa aktarma:** seçilebilir metin ve yer imleri içeren PDF, etkileşimli veya statik HTML,
  CSV, PNG.
- **Erişilebilirlik:** her yerde klavye (`F6` başlık çubuğu, araç çubuğu, kenar çubuğu ve belge
  arasında dolaşır), ekran okuyucular için UI Automation, Yüksek Karşıtlık.

| Kenar çubuğuyla koyu mod | Canlı Düzenlemede tablo | Zihin haritası |
|---|---|---|
| ![](tests/golden/v1a/doc-sidebar-dark.png) | ![](tests/golden/v1a/top-v2f-table-dark.png) | ![](tests/golden/v1a/top-v2d-mindmap-light.png) |

## Kurulum, taşınabilir kullanım, kaldırma

- **Geçerli kullanıcı için kurulum (yönetici izni gerekmez):** `yamdv.exe --install` komutunu
  çalıştırın ya da `yamdv.exe`'yi başlatıp karşılama sayfasındaki **YAMDV'yi bu kullanıcı için
  yükle** düğmesine tıklayın. Program kendini `%LOCALAPPDATA%\Programs\YAMDV\` klasörüne
  kopyalar, Markdown dosya türlerini kaydeder, bir Başlat menüsü kısayolu ve bir *Uygulamalar ve
  özellikler* girdisi ekler, ardından kurulan kopyayı başlatır. Onun karşılama sayfasında
  **Varsayılan yap** düğmesine tıklayın; Windows Ayarları açılır ve YAMDV'yi kendiniz seçersiniz.
  `--install` komutunu yeniden çalıştırmak güvenlidir (aynı dosyaları ve anahtarları yeniler).
- **Taşınabilir:** `yamdv.exe`'nin yanına `yamdv.portable` adlı boş bir dosya koyun. Ayarlar,
  geçmiş ve sık kullanılanlar bu durumda exe'nin yanındaki `YAMDV-data\` klasöründe tutulur ve
  hiçbir şey kaydedilmez.
- **Kaldırma:** *Ayarlar ▸ Uygulamalar ▸ YAMDV ▸ Kaldır* veya `yamdv.exe --uninstall`. Bir
  iletişim kutusu ayarlarınızın saklanıp saklanmayacağını sorar; kayıt defteri değerleri, kısayol
  ve kurulum klasörü kaldırılır (tam olarak `--install`'ın yazdıkları).

## Okuma ve düzenleme

| Tuş | Ne yapar |
|---|---|
| `F2` | **Canlı Düzenleme**: imlecin altındaki bloğu yerinde düzenler; geri kalanı işlenmiş görünür |
| `F3` | **Bölünmüş Düzenleme**: bir tarafta kaynak, diğer tarafta canlı önizleme |
| `F4`, `Ctrl+E` | dosyayı harici düzenleyicinizde açar (VS Code, Notepad++, Not Defteri veya özel bir komut) |
| sağ tık, `Shift+F10` | bağlam menüsü: kopyalama, görünüm, dosya, dışa aktarma, ayarlar |

Diğer kısayollar: `Ctrl+O` aç · `F5` yeniden yükle · `Ctrl+F` bul (`Ctrl+G` / `Ctrl+Shift+G`) ·
`Ctrl+\` ana hat · `Ctrl+D` sık kullanılan · `Ctrl+C` / `Ctrl+Shift+C` kopyala / Markdown olarak
kopyala · `Ctrl+=` / `Ctrl+-` / `Ctrl+0` yazı boyutu · `Alt+↑` / `Alt+↓` sütun genişliği ·
`Alt+←` / `Alt+→` geri / ileri · `Ctrl+S` kaydet (düzenleme kiplerinde) · `Ctrl+P` yazdır ·
`Ctrl+Shift+E` PDF olarak dışa aktar · `F1` tüm kısayollar · `F11` tam ekran · `Ctrl+Shift+O`
başlık paleti · `Ctrl+Shift+M` zihin haritası · `F6` / `Shift+F6` sonraki / önceki bölge (başlık
çubuğu, araç çubuğu, kenar çubuğu, belge) · `Esc`, `Ctrl+W` kapat.
Tüm kısayollar *Ayarlar ▸ Kısayollar* bölümünden değiştirilebilir.

## Dışa aktarma ve yazdırma

- **Dışa aktar ▸ HTML olarak dışa aktar…** tek ve kendi kendine yeten bir `.html` dosyası yazar:
  satır içi CSS olarak geçerli renk şeması, bir içindekiler tablosu, vurgulanmış kod ve `data:`
  URI olarak gömülü görseller, formüller ve diyagramlar. Etkileşimli sürüm, sıkı bir
  Content-Security-Policy altında karması (hash) alınmış tek bir betik ekler (kenar çubuğu,
  katlama, tema, bulma); `export.html_interactive = false` betiksiz sürümü verir.
- **Dışa aktar ▸ PDF olarak dışa aktar…** (`Ctrl+Shift+E`) seçilebilir, aranabilir metin,
  başlıklardan yer imleri ve çalışan bağlantılar içeren bir PDF yazar; **Yazdır…** (`Ctrl+P`)
  herhangi bir yazıcıyı kullanır. Sayfalar bloklar veya satırlar arasında bölünür; tablo
  satırları, kod satırları ve diyagramlar asla bölünmez.
- Bir tabloya sağ tıklayın: **Tabloyu CSV olarak kopyala** / **Tabloyu CSV olarak dışa aktar…**
  (BOM'lu UTF-8, CRLF, RFC 4180; `= + - @` veya sekme ile başlayan hücrelerin başına `'` eklenir,
  böylece hesap tabloları bunları formül olarak çalıştırmaz).
- Arayüzsüz (headless): `yamdv --export-html in.md [-o out.html] [--theme github] [--mode light|dark]`,
  `yamdv --export-pdf in.md [-o out.pdf]`. Çıkış kodları: 0 başarılı, 1 hata, 2 kullanım hatası.

## Gizlilik ve nasıl doğrulanır

YAMDV hiçbir ağ kodu içermez. Bir belgenin işaret ettiği hiçbir şeyi indirmez (uzak görseller
yer tutucu olarak gösterilir), telemetri göndermez ve çökme raporu yüklemez. Ayarlar
`%APPDATA%\YAMDV` klasöründe (taşınabilir kipte `YAMDV-data`) kalır. Bunu kendiniz doğrulamak
için:

1. Windows Güvenlik Duvarı'nda `yamdv.exe`'yi (giden bağlantılar için) engelleyin. Hiçbir şey
   değişmez.
2. Uzak görseller içeren bir belge açın. Belge açıkken `netstat -ano` ve Kaynak İzleyicisi
   `yamdv.exe`'ye ait hiçbir bağlantı göstermez.
3. `cargo xtask check-imports yamdv.exe` içe aktarılan DLL'leri listeler. `ws2_32`, `winhttp`,
   `wininet`, `urlmon` veya benzerleri yer almaz; CI bunu her derlemede çalıştırır.

Bağlantılar yalnızca siz tıkladığınızda, bir onaydan sonra, varsayılan tarayıcınızda açılır.

## Bilinen sınırlamalar

Planlanan çalışmalar issue olarak izlenir (https://github.com/snipeTR/YAMDV/issues). Örnekler:
çok büyük tablo satırlarının tembel (lazy) yerleşimi (PERF-08); WebP görseller yer tutucu olarak
gösterilir; ilk boyama ve dosya açma iletişim kutusu süreleri gerçek donanımda doğrulanıyor
(PERF-02).

## Kaynaktan derleme

Windows 10/11 x64 ve `rust-toolchain.toml` dosyasında sabitlenmiş Rust araç zinciri (1.99.0,
MSVC hedefi) gerekir.

```
cargo build --release --locked -p yamdv-app      # target\release\yamdv.exe
cargo test --workspace --locked
cargo xtask ci                                   # fmt, clippy, testler, spesifikasyon denetimleri
```

Belgeler: [`docs/SPEC.md`](docs/SPEC.md), [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md),
[`docs/SECURITY.md`](docs/SECURITY.md), [`docs/PROCESS.md`](docs/PROCESS.md),
[`CHANGELOG.md`](CHANGELOG.md) (belgeler İngilizcedir).

## Lisans

Lisans: AGPL-3.0-or-later (GNU Affero Genel Kamu Lisansı sürüm 3 veya sonrası, bkz.
[`LICENSE`](LICENSE)).
Kaynak kod: https://github.com/snipeTR/YAMDV (*Ayarlar → Hakkında*, derlemenizin tam etiketini
gösterir).
Üçüncü taraf lisansları: sürüm zip dosyasındaki `THIRD_PARTY_NOTICES.txt` (`cargo xtask notices`).
