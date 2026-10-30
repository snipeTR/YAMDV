# YAMDV V2-I — owner real-hardware QA (QA-01, HARD-03)

Pack: `YAMDV-v2i-qa.zip` (draft pre-release `qa-v2i`). Unzip anywhere; no admin rights needed.
Write each result into **`qa-results.txt`** next to this file, one line per item:
`<no> PASS|FAIL|SKIP <note>` (e.g. `7 FAIL AltGr+Q gives nothing in F3`). Screenshots of
failures go into the `failures\` folder. Send `test-results.txt`, `qa-results.txt` and `failures\`.

`yamdv.exe` is the release build; use it for every item except 1.

| # | Do | You should see |
|---|---|---|
| 1 | Double-click `run-tests.cmd`; wait until it says *Done*. | `test-results.txt` exists; note the PASS/FAIL/SKIP counts. |
| 2 | Open `tests\corpus\002\typical.md`. Task Manager → Details → add column "Memory (active private working set)": note it now and after 10 min idle. While it is open, run `netstat -ano \| findstr <PID>` in a console (S-NET-05). | Memory ≤ ~5 MiB and not growing; **netstat prints nothing**; no Windows Firewall prompt ever appears. |
| 3 | Display scaling 100 / 150 / 200 %; with two monitors at different scaling, drag the window across. | Text, formulas, diagrams and split view stay sharp and correctly sized; no blurry frame. |
| 4 | Switch Windows dark ↔ light (Settings → Personalization → Colors) with YAMDV open. | YAMDV follows immediately (theme = System). |
| 5 | Settings → Default apps → `.md` → YAMDV; Explorer right-click → *Open with* → YAMDV; double-click a `.md`. | Each opens the file in YAMDV. |
| 6 | Drag files and a folder from Explorer onto the window. | Files open; a folder opens in the sidebar. |
| 7 | Turkish Q layout: type `ğüşıöç İ` and AltGr `@ € { [ ] } \ |` plus a dead key (e.g. `^` then `a`) into the F2 and F3 fields. | Exactly those characters appear. |
| 8 | Japanese or Chinese IME: compose and commit a word in F2 and F3. | Composition window near the field; committed text appears. |
| 9 | File → Open in a OneDrive / Dropbox / TortoiseGit folder (shell extensions); drag a file from that dialog. | Dialog shows overlays, does not hang or crash (ACG, ADR-0001); the drag works. |
| 10 | Edit a file without saving; change it in Notepad and save there. | Conflict banner; *Save as* writes your version to a new file. |
| 11 | Print a document to a real printer. | Pages print; text is crisp (ClearType/vector), no missing glyphs. |
| 12 | High Contrast on/off (`Left Alt+Left Shift+Print Screen`), try both black and white themes. | Every screen (document, settings, menus, banners, viewer) is readable; focus ring visible. |
| 13 | Narrator (`Ctrl+Win+Enter`): Tab and arrow through title buttons, menus, settings; read the document (`Caps Lock+M`). | Narrator says the button/menu/setting names and reads the document text. |
| 14 | Export PDF and open it in Edge and Acrobat; export interactive HTML and open it in a browser. | PDF text selectable/searchable; HTML theme toggle and copy buttons work offline. |
| 15 | Taskbar jump list (recent/pinned files); Settings → custom icon; F4 external editor. | Jump list opens files; the icon shows; F4 opens the editor with the file. |

---

# YAMDV V2-I — sahibin gerçek donanım QA listesi (QA-01, HARD-03)

Paket: `YAMDV-v2i-qa.zip` (taslak ön sürüm `qa-v2i`). Herhangi bir yere açın; yönetici hakkı gerekmez.
Her sonucu bu dosyanın yanındaki **`qa-results.txt`** dosyasına, madde başına bir satır yazın:
`<no> PASS|FAIL|SKIP <not>` (ör. `7 FAIL F3'te AltGr+Q bir şey yazmıyor`). Hataların ekran
görüntüleri `failures\` klasörüne. `test-results.txt`, `qa-results.txt` ve `failures\` gönderin.

`yamdv.exe` sürüm derlemesidir; 1. madde dışında hep onu kullanın.

| # | Yapın | Görmeniz gereken |
|---|---|---|
| 1 | `run-tests.cmd` dosyasına çift tıklayın; *Done/Bitti* yazana kadar bekleyin. | `test-results.txt` oluşur; PASS/FAIL/SKIP sayılarını not edin. |
| 2 | `tests\corpus\002\typical.md` açın. Görev Yöneticisi → Ayrıntılar → "Bellek (etkin özel çalışma kümesi)" sütunu: şimdi ve 10 dk boşta bekledikten sonra not edin. Açıkken bir konsolda `netstat -ano \| findstr <PID>` çalıştırın (S-NET-05). | Bellek ≈ 5 MiB'i aşmaz ve büyümez; **netstat hiçbir şey yazmaz**; Windows Güvenlik Duvarı uyarısı hiç çıkmaz. |
| 3 | Ekran ölçeği %100 / %150 / %200; farklı ölçekli iki monitörde pencereyi sürükleyin. | Metin, formüller, diyagramlar ve bölünmüş görünüm net ve doğru boyutta kalır. |
| 4 | YAMDV açıkken Windows koyu ↔ açık (Ayarlar → Kişiselleştirme → Renkler). | YAMDV hemen uyar (tema = Sistem). |
| 5 | Ayarlar → Varsayılan uygulamalar → `.md` → YAMDV; Gezgin sağ tık → *Birlikte aç* → YAMDV; `.md` dosyasına çift tık. | Her biri dosyayı YAMDV'de açar. |
| 6 | Gezgin'den dosyaları ve bir klasörü pencereye sürükleyin. | Dosyalar açılır; klasör kenar çubuğunda açılır. |
| 7 | Türkçe Q klavye: F2 ve F3 alanlarına `ğüşıöç İ`, AltGr ile `@ € { [ ] } \ |` ve bir ölü tuş (ör. `^` sonra `a`) yazın. | Tam olarak bu karakterler görünür. |
| 8 | Japonca veya Çince IME: F2 ve F3'te bir sözcük oluşturup onaylayın. | Oluşturma penceresi alanın yanında; onaylanan metin görünür. |
| 9 | Dosya → Aç ile OneDrive / Dropbox / TortoiseGit klasörü (kabuk eklentileri); iletişim kutusundan bir dosya sürükleyin. | İletişim kutusu simgeleri gösterir, donmaz/çökmez (ACG, ADR-0001); sürükleme çalışır. |
| 10 | Bir dosyayı kaydetmeden düzenleyin; Not Defteri'nde değiştirip kaydedin. | Çakışma şeridi çıkar; *Farklı kaydet* sizin sürümünüzü yeni dosyaya yazar. |
| 11 | Gerçek bir yazıcıya yazdırın. | Sayfalar basılır; metin keskin, eksik karakter yok. |
| 12 | Yüksek Karşıtlık aç/kapat (`Sol Alt+Sol Shift+Print Screen`), siyah ve beyaz temaları deneyin. | Her ekran (belge, ayarlar, menüler, şeritler, görüntüleyici) okunur; odak çerçevesi görünür. |
| 13 | Ekran Okuyucu (`Ctrl+Win+Enter`): başlık düğmeleri, menüler, ayarlar arasında Tab/oklarla gezin; belgeyi okutun (`Caps Lock+M`). | Düğme/menü/ayar adlarını söyler ve belge metnini okur. |
| 14 | PDF dışa aktarıp Edge ve Acrobat'ta açın; etkileşimli HTML dışa aktarıp tarayıcıda açın. | PDF metni seçilebilir/aranabilir; HTML tema düğmesi ve kopyala düğmeleri çevrimdışı çalışır. |
| 15 | Görev çubuğu atlama listesi (son/sabit dosyalar); Ayarlar → özel simge; F4 dış düzenleyici. | Atlama listesi dosyaları açar; simge görünür; F4 düzenleyiciyi dosyayla açar. |
