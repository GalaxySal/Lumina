# Lumina

Lumina, son kullanıcı için yazılmış özgün bir masaüstü web tarayıcısıdır. Chrome, Firefox ya da başka bir tarayıcının kopyası, çatalı veya yeniden kaplaması değildir.

Bu dosya projenin değişmez kararlarını tutar. Buradaki bir kuralla çelişen bir istek gelirse önce çelişkiyi kullanıcıya söyle; sessizce kuralı esnetme.

## Değişmez kurallar

1. **Gerçek tarayıcı.** Mock, sahte veri, yer tutucu ekran, "TODO: sonra gerçek yapılacak" ile geçiştirilmiş özellik yok. Bir özellik ya gerçekten çalışır ya da arayüzde hiç görünmez. Henüz desteklenmeyen bir şeyi (ör. Widevine) kullanıcıya dürüstçe söyle, çalışıyormuş gibi gösterme.
2. **Chromium ve Gecko yok.** Kaynak kodları, çatalları ve onları saran her şey yasak: CEF, Qt WebEngine, WebView2, WebKitGTK/WPE, Electron, Tauri, wry, herhangi bir sistem WebView'ı. Web motoru Servo'dur ve kütüphane olarak gömülür.
3. **Tek dil: Rust.** Uygulama kodu, derleme betikleri (`build.rs`, `xtask`), araçlar ve testler Rust ile yazılır. Kabuk betiği, Python, JS/TS yardımcı aracı ekleme. Arayüz HTML/CSS/JS ile yazılmaz. İstisna yalnızca veri dosyalarıdır (TOML, filtre listeleri, fontlar, GLSL/WGSL gölgelendiriciler).
4. **Panik yok.** Aşağıdaki "Hata yönetimi" bölümü bağlayıcıdır.
5. **Yalnızca HTML değil.** Hedef tam web platformudur: CSS, JavaScript, WebAssembly, canvas/WebGL, medya, WebRTC, EME. Bir özelliği "sadece statik sayfalar için" tasarlama.
6. **Hafiflik bir özelliktir.** Her yeni bağımlılık ve her arka plan işi gerekçesini taşımak zorundadır (bkz. "Performans bütçesi").
7. **Özgün tasarım.** Bkz. "Tasarım". Varsayılan egui görünümü hiçbir ekranda görünmemelidir.

## Mimari kararlar

| Konu | Karar |
| --- | --- |
| Web motoru | Servo, `libservo` gömme API'si üzerinden |
| Dil | Rust (kararlı sürüm kanalı; Servo zorunlu kılarsa `rust-toolchain.toml` ile sabitlenmiş sürüm) |
| Pencere ve girdi | `winit` |
| Tarayıcı arayüzü | `egui` 0.36 (`egui-winit`, `egui_glow`), tamamen Lumina'ya özel tema ve bileşenlerle |
| Reklam engelleme | `adblock` crate'i (adblock-rust); EasyList/uBlock uyumlu listeler |
| DRM | EME altyapısı + ClearKey şimdi; Widevine, lisans alındığında takılacak bir CDM arayüzünün arkasında |
| WebRTC | Servo'nun WebRTC desteği; eksikler Servo'ya katkı ya da Rust tarafında tamamlanarak kapatılır |
| Ayarlar ve temalar | TOML dosyaları, XDG dizinlerinde |
| Platform | Önce Linux (Wayland birincil, X11 destekli). Windows ve macOS sonra; platforma özgü kod `platform` modülünün arkasında kalır |
| Lisans | MPL-2.0 |

### egui sürümü neden 0.36'da sabit

egui 0.37, glif çizimi için `vello_cpu` 0.2 kullanıyor; Servo 0.8 de aynı crate'i `multithreading` özelliğiyle açıyor. Cargo özellikleri birleştirdiği için egui çok iş parçacıklı kipte çalışmaya zorlanıyor ve ilk karede panikliyor ("attempted to rasterize before flushing"). egui 0.36, `vello_cpu` 0.1 kullandığı için Servo'nunkinden ayrı bir kopya alıyor. Servo ya da egui'yi yükseltirken bunu yeniden sına; `glow` ana sürümünün (0.17) ikisinde de aynı kalması da şart, çünkü tek bir GL bağlamını paylaşıyorlar.

### Dürüst sınırlar

Bunları bil ve kullanıcıya sorulduğunda olduğu gibi söyle:

- **Widevine** Google'ın lisansına bağlıdır ve bağımsız motorlara verilmesi garanti değildir. Lisans gelene kadar Netflix, Spotify, Disney+ gibi siteler çalışmaz. Kapalı bir CDM ikilisini izinsiz paketleme ya da indirme.
- **Servo** hâlâ olgunlaşan bir motordur; bazı siteler bozuk görünür ya da hiç çalışmaz. Bir site bozuksa önce sorunun Servo'da mı Lumina'da mı olduğunu ayır. Servo kaynaklıysa siteye özel yama yazma; sorunu kaydet, mümkünse Servo'ya bildir.
- **Site uyumluluğu için User-Agent'ı Chrome/Firefox gibi gösterme** kararı kullanıcıya aittir; kendiliğinden yapma.

### Çalışma alanı düzeni

Cargo workspace. Crate'ler küçük ve tek sorumlu kalır; arayüz crate'i motorun iç tiplerini görmez.

```
crates/
  lumina/          ikili; başlatma, olay döngüsü, girdinin arayüz ile sayfa arasında dağıtımı, kare çizimi
  lumina-engine/   Servo gömme katmanı: sayfa yaşam döngüsü, gezinme, girdi çevirisi, IME
  lumina-ui/       egui arayüzü: araç çubuğu, adres çubuğu, ayarlar ve ilk kurulum ekranı, çeviri tabloları
  lumina-theme/    tasarım belirteçleri, açık/koyu paletler, vurgu renkleri, gömülü fontlar
  lumina-store/    kalıcı veri: settings.toml, arama motorları, adres/arama ayrımı
assets/fonts/      Newsreader, Alegreya Sans, IBM Plex Mono (OFL) ve lisans metinleri
docs/screenshots/  gerçek çalıştırmalardan alınmış ekran görüntüleri
```

Henüz olmayan, planlanan crate'ler: `lumina-shield` (reklam engelleme), `lumina-media` (EME/CDM), `lumina-platform` (işletim sistemine özgü kod), `xtask`.

Arayüz ile motorun aynı pencereye çizimi: Servo sayfayı ekran dışı bir çerçeve arabelleğine çizer; her karede `lumina` önce bu resmi arayüzün sayfa için bıraktığı dikdörtgene kopyalar, sonra egui'yi üstüne çizer (`Browser::frame`, `PageView::blit`).

Bir crate'i ancak gerçek kodu olduğunda oluştur. Boş iskelet crate açma.

## Hata yönetimi

Uygulama hiçbir koşulda kendi kodundan çökmemeli. Bir sekmenin hatası o sekmede kalır.

- Üretim kodunda yasak: `unwrap()`, `expect()`, `panic!`, `unreachable!`, `todo!`, `unimplemented!`, `assert!` ailesi, sınır denetimsiz dizinleme (`v[i]`, `&s[a..b]`), taşabilecek aritmetik, `RefCell` çift ödünç alma riski taşıyan desenler.
- Yerine: `Result`/`Option` ile `?`, `.get()`, `checked_*`/`saturating_*`, `let … else`, `match`.
- Bu, workspace düzeyinde lint ile zorlanır; kuralı gevşetme:

  ```toml
  [workspace.lints.clippy]
  unwrap_used = "deny"
  expect_used = "deny"
  panic = "deny"
  unreachable = "deny"
  todo = "deny"
  unimplemented = "deny"
  indexing_slicing = "deny"
  arithmetic_side_effects = "deny"

  [workspace.lints.rust]
  unsafe_code = "deny"
  ```

- Testlerde `unwrap`/`expect`/`assert!` serbesttir (`#[cfg(test)]` altında `#[allow]`).
- `unsafe` yalnızca kaçınılmaz FFI sınırında, tek bir modülde, `// SAFETY:` açıklamasıyla ve o modüle özel `#[allow(unsafe_code)]` ile yazılır.
- Hata tipleri: kütüphane crate'lerinde `thiserror` ile somut enum'lar; `anyhow` yalnızca `lumina` ikilisinin en üst katmanında.
- Bağımlılıklardan (Servo dahil) gelebilecek panikler için iş parçacığı sınırlarında `catch_unwind` ile yakala, sekmeyi "çöktü" durumuna al ve kullanıcıya yeniden yükleme seçeneği sun. `panic = "abort"` kullanma.
- Hatalar yutulmaz: ya kullanıcıya anlaşılır bir dille gösterilir ya da `tracing` ile kaydedilir. `let _ = …` ile hata atmak gerekçe yorumu ister.
- Kullanıcıya gösterilen hata metinleri teknik jargon içermez ve ne yapabileceğini söyler.

## Performans bütçesi

Ölçmeden "hızlı" deme. Bu değerler hedeftir; aşan bir değişiklik gerekçe ister.

- Soğuk açılıştan ilk kareye: ≤ 400 ms (yayın derlemesi, SSD).
- Boşta, tek boş sekme: arayüz süreci ≤ 150 MB RSS; boşta CPU ≈ %0 (sürekli yeniden çizim yok — egui yalnızca olay geldiğinde çizer).
- Arayüz kare süresi: 60 Hz'de ≤ 8 ms; arayüz iş parçacığında engelleyen G/Ç, ağ ya da disk işi yok.
- Reklam filtresi istek başına ≤ 50 µs; filtre motoru derlenmiş hâlde diske önbelleklenir, açılışta listeler yeniden ayrıştırılmaz.
- Arka plandaki sekmeler kısılır; uzun süre kullanılmayanlar bellekten boşaltılıp gerektiğinde geri yüklenir.
- Yeni bağımlılık eklemeden önce: gerçekten gerekli mi, derleme süresine ve ikili boyutuna etkisi ne, bakımı sürüyor mu, lisansı MPL-2.0 ile uyumlu mu?

## Tasarım

Varsayılan kimlik: **sıcak, kâğıt hissi.** Sakin, okunaklı, dergi/kitap havasında bir araç; teknoloji ürünü gibi değil, iyi dizilmiş bir sayfa gibi durmalı.

- **Renk:** kırık beyaz kâğıt zemin, sıcak mürekkep siyahı metin, tek bir bastırılmış vurgu rengi. Saf `#FFFFFF` ve saf `#000000` kullanma. Koyu tema da sıcaktır: kömür/sepya, soğuk mavi-gri değil.
- **Tipografi (kullanıcının seçimi):** başlıklarda Newsreader, gövdede ve denetimlerde Alegreya Sans, adresler ve harfi harfine gösterilen metinler için IBM Plex Mono. Üçü de OFL lisanslı, `assets/fonts` altında ve ikiliye gömülü; sistem fontuna güvenme. Yeni bir font eklersen Türkçe karakterlerin (ç, ğ, ı, İ, ö, ş, ü) tamamını desteklediğini doğrula.
- **Vurgu rengi (kullanıcının seçimi):** varsayılan kiremit; kullanıcı ayarlardan çivit ya da zeytine geçebilir. Palet testleri (`lumina-theme`) her tema ve vurgu için WCAG AA kontrastını denetler; yeni renk eklerken bu testlere ekle.
- **Yasak kalıplar:** Inter, Roboto, Arial, Space Grotesk ve sistem varsayılanı; mor-mavi gradyanlar; cam efekti (glassmorphism); her şeyde aynı büyük köşe yarıçapı; neon parlamalar; gereksiz emoji ve ışıltı simgeleri; her kartta aynı gölge. Kısacası "yapay zekâ üretti" dedirten ortalama görünüm.
- **Tekrar etme:** her ekran aynı kart ızgarası olmasın. Ayarlar, yeni sekme sayfası, indirmeler ve geçmiş kendi içeriğine uygun yerleşim kullanır. Yeni bir ekran tasarlarken öncekini kopyalayıp doldurma.
- **Belirteçler:** renk, boşluk, yarıçap, yazı boyutu ve süreler yalnızca `lumina-theme` belirteçlerinden okunur. Arayüz kodunda sabit renk yazma — tema özelleştirmenin çalışması buna bağlı.
- **Bileşenler:** egui'nin hazır düğme, radyo ve onay kutularını kullanma; `lumina-ui/src/widgets.rs` içindeki Lumina'ya özel bileşenleri kullan ya da oraya yenisini ekle. Simgeler ikon fontundan gelmez, çizgiyle çizilir.
- **Mevcut yerleşimler:** ayarlar ekranı kenar boşluğunda etiketleri olan bir form gibi; ilk kurulum ekranı ortalanmış tek sütun, numaralı adımlarla. Yeni ekranlar bunları kopyalamaz.
- **Hareket:** kısa ve amaçlı (120–200 ms). Sistemdeki "hareketi azalt" tercihine uyulur.
- **Erişilebilirlik:** metin kontrastı en az WCAG AA; her işlev klavyeyle erişilebilir; odak göstergesi her zaman görünür; ekran okuyucu için AccessKit etkin.

### Tema özelleştirme

Dahili ve birinci sınıf bir özelliktir, eklenti değil. Bugün var olan: açık/koyu/sistem görünümü ve üç vurgu rengi, anında uygulanır ve `settings.toml` içinde saklanır. Aşağıdakilerin geri kalanı henüz yazılmadı.

- Kullanıcı arayüzden renkleri, vurgu rengini, yazı tiplerini ve boyutlarını, yoğunluğu, köşe yarıçapını, sekme çubuğu konumunu değiştirebilir; değişiklik anında, yeniden başlatmadan uygulanır.
- Temalar düz TOML dosyalarıdır (`$XDG_CONFIG_HOME/lumina/themes/`); içe/dışa aktarılabilir, elle düzenlenebilir, dosya değişince canlı yenilenir.
- Bozuk ya da eksik bir tema dosyası uygulamayı durdurmaz: geçersiz alanlar varsayılana düşer ve kullanıcıya hangi satırın sorunlu olduğu söylenir.
- Açık/koyu, sistem tercihini izleyebilir. Sitelere `prefers-color-scheme` doğru iletilir.

## Son kullanıcı odağı

- Hedef kişi geliştirici değil, sıradan kullanıcıdır. Varsayılanlar makul olmalı; hiçbir temel iş için yapılandırma dosyası düzenlemek gerekmemeli.
- Arayüz dili Türkçe ve İngilizce. Kullanıcıya görünen her metin `lumina-ui/src/text.rs` içindeki dil tablolarında durur (`Strings` yapısı); arayüz koduna gömülü kullanıcı metni yazma. Tablo düz bir Rust yapısı olduğu için eksik çeviri derleme hatasıdır. (Başta Fluent düşünülmüştü; ek bağımlılık ve çalışma zamanında eksik anahtar riski olmadığı için bu yol seçildi — kullanıcı aksini isterse değiştir.)
- Beklenen temel işlevler eksiksiz olmalı: sekmeler, geçmiş, yer imleri, indirmeler, sayfada bul, yakınlaştırma, oturum geri yükleme, gizli pencere, izin istemleri (kamera, mikrofon, konum, bildirim), parola alanı güvenliği, sertifika hatası uyarıları.
- Gizlilik varsayılandır: telemetri yok, izinsiz ağ isteği yok (filtre listesi güncellemesi hariç ve o da ayarlardan kapatılabilir).
- Reklam engelleyici varsayılan olarak açıktır; adres çubuğundan site bazında tek tıkla kapatılabilir ve kaç isteğin engellendiği görünür.

## Güvenlik

- Tarayıcı güvenilmeyen içerik çalıştırır. Web içeriğinden gelen her veri (URL, başlık, favicon, indirme adı) arayüzde gösterilmeden önce doğrulanır ve sınırlandırılır.
- Adres çubuğu her zaman gerçek kaynağı gösterir; IDN/punycode sahteciliğine karşı önlem alınır.
- TLS doğrulaması asla sessizce atlanmaz. HTTPS öncelikli gezinme varsayılandır.
- Parolalar ve çerez anahtarları düz metin saklanmaz; sistem anahtarlığı (Secret Service) kullanılır.
- İndirilen dosyalar kendiliğinden çalıştırılmaz.
- `cargo deny` ile bağımlılık lisansları ve bilinen güvenlik açıkları denetlenir.

## Geliştirme

```
cargo run -p lumina                                   # çalıştır (açılış sayfası)
cargo run -p lumina -- example.com                    # belirli bir adresi aç
cargo run -p lumina -- kedi resimleri                 # seçili arama motoruyla ara
cargo test --workspace                                # testler
cargo clippy --workspace --all-targets -- -D warnings # lint (panik yasakları dahil)
cargo fmt --all                                       # biçimlendirme
```

Kısayollar: Ctrl+L adres çubuğu, Ctrl+R / F5 yenile, Alt+Sol / Alt+Sağ geri / ileri, Ctrl+, ayarlar, Esc ayarları kapat.

`RUST_LOG=warn,lumina=trace` her pencere olayını günlüğe yazar.

- Bir iş, `clippy` ve testler temiz geçmeden bitmiş sayılmaz. Arayüzü etkileyen bir değişiklik, uygulama gerçekten çalıştırılıp görülmeden "çalışıyor" diye bildirilmez.
- Servo'ya dokunmayan crate'leri (`lumina-store`, `lumina-theme`, `lumina-ui`) hızlı denemek için ayrı bir hedef dizini kullan: `CARGO_TARGET_DIR=target/ui cargo test -p lumina-ui`. Böylece ana derlemenin kilidini beklemezsin.
- Arayüzü kullanıcının masaüstünü görüntülemeden sınamak için görünmez bir sway oturumu aç (`WLR_BACKENDS=headless WLR_RENDERER=pixman WLR_LIBINPUT_NO_DEVICES=1 sway -c <asgari yapılandırma>`), Lumina'yı o oturumun `WAYLAND_DISPLAY` değeri ve geçici bir `XDG_CONFIG_HOME` ile çalıştır, `grim` ile ekran görüntüsü al. Tuşlar için `wtype` kullan ama her tuşun arasına `-s 350` gibi bir bekleme koy; yoksa tuşlar kaybolur. Bu oturumda fare aygıtı yoktur; `swaymsg seat … cursor` işe yaramaz, wlr sanal işaretçi protokolünü kullanan bir araç gerekir.
- Servo'yu derlemek ağırdır (uzun süre, çok disk alanı, sistem bağımlılıkları). Servo sürümü `Cargo.toml` içinde belirli bir commit'e sabitlenir; yükseltme ayrı ve tek amaçlı bir değişikliktir.
- Testler gerçek davranışı sınar: filtre motoru gerçek liste kurallarıyla, tema yükleyici gerçek TOML dosyalarıyla, gezinme yerel bir test sunucusuyla. Motoru taklit eden sahte katmanlar yazma.
- Kod, yorumlar ve commit mesajları İngilizce; kullanıcıyla iletişim ve bu dosya Türkçe.
- Commit'ler küçük ve tek amaçlı olur. Kullanıcı istemeden commit ya da push yapma.

### Bu makineye özgü not

- Makine 4 çekirdek ve 7 GB RAM'e sahip. Servo'nun ilk derlemesi çok uzun sürer; `cargo clean` çalıştırma ve Servo'nun özellik (feature) kümesini gereksiz yere değiştirme — ikisi de tam yeniden derleme demektir. Uzun derlemeleri arka planda çalıştır.
- `~/.zprofile` yalnızca tty1 girişinde ve grafik oturumu yokken sway başlatır. Bu koşul kaldırılırsa çalıştırılan her komut iç içe bir sway oturumu açıp takılır.

## Yol haritası ve kapsam kararları

**Kapsam ve sıra kullanıcının kararıdır.** Bir aşamaya başlamadan önce o aşamada ne yapılacağını ve neyin sonraya bırakılmasını düşündüğünü kullanıcıya sor. Kullanıcının onayı olmadan hiçbir özelliği erteleme, kırpma ya da "sonraki aşamada" diye not düşüp geçme. Bir tarayıcıda olması beklenen bir şey eksikse bunu açıkça söyle. Font, renk gibi tasarım seçimlerini de seçenek sunarak sor.

Yapılanlar:

- Servo ile gerçek sayfaları açan pencere; fare, tekerlek, klavye ve IME girdisi.
- Araç çubuğu: geri, ileri, yenile, adres çubuğu (adres ya da arama), ayarlar düğmesi; yükleme sırasında vurgu rengiyle çizilen alt kenar.
- Kalıcı ayarlar (`$XDG_CONFIG_HOME/lumina/settings.toml`): arama motoru, açılış sayfası, görünüm, vurgu rengi, dil. Bozuk dosya ya da geçersiz değer uygulamayı durdurmaz; ayarlar ekranında satır numarasıyla bildirilir.
- İlk kurulum ekranı ve ayarlar ekranı; Türkçe ve İngilizce arayüz.
- Sayfa çökünce gösterilen "yeniden yükle" ekranı ve sitelere `prefers-color-scheme` iletimi.

Kullanıcının belirlediği sıradaki aşamalar (2026-11-12): sekmeler; indirmeler; geçmiş ve yer imleri; reklam engelleyici; saydamlık (hem pencerenin kendisi hem araç çubuğu ve diğer arayüz yüzeyleri; sonradan eklenmiş bir efekt değil, tema modelinin yerleşik bir parçası olarak — kullanıcı tema seçerken saydamlığı da ayarlayabilmeli). Kullanıcının kullanım limiti dar olduğunda önce en küçük işi yap ve işi küçük, tek başına biten parçalara böl.

Henüz yapılmayanların tamamı: sekmeler; yeni pencere açan bağlantılar; sayfa içi denetimler (`<select>` listesi, dosya ve renk seçici, `alert`/`confirm`/`prompt`, sağ tık menüsü — şu an Servo'nun varsayılan yanıtı veriliyor); izin istemleri; dokunmatik girdi; indirmeler; geçmiş ve yer imleri; oturum geri yükleme; sayfada bul ve yakınlaştırma; reklam engelleyici; tema dosyaları ve tema düzenleyici; sistemin koyu tema tercihini Wayland'da okuma (portal); gizli pencere; EME + ClearKey ve WebRTC doğrulaması; erişilebilirlik ağacı (AccessKit); panik yakalama sınırları; performans bütçesi ölçümleri; paketleme; Windows ve macOS; Widevine.
