# Alt‑Tab: mapa zdrojů a verzí

Stav zmapovaný 14. 10. 2026 ze souborů v tomto repozitáři. Účelem této přípravy je porozumět kódu před opravami. Původní soubory verzí 1.0–1.3 nebyly změněny.

## Domluvený způsob práce

- Pracujeme na Macu a upravujeme soubory pro Linux.
- Instalaci a ověření v přihlášeném prostředí COSMIC provádí uživatel ručně na Linuxu.
- Na Macu nespouštět zdejší instalátory, služby ani přepínač. Při této přípravě se nic nekompilovalo ani neinstalovalo.
- Chyby budeme řešit až v navazujícím zadání. Tento přehled není potvrzením funkčnosti v COSMIC.
- Z obsahu repozitáře nevíme, která varianta je právě nainstalovaná na uživatelově Linuxu. Nejvyšší číslo složky samo o sobě neurčuje používanou verzi.

## Co je v Python payloadu

Rust není přeložený do Pythonu. Oba instalátory verze 1.1 obsahují proměnnou `PAYLOAD`:

```text
Python instalátor
  → PAYLOAD: Base64 text
  → dekódování na archiv .tar.gz
  → ověření PAYLOAD_SHA256
  → rozbalení Rustového projektu včetně Cargo.toml, Cargo.lock a config/
  → cargo test a cargo build na Linuxu
  → nativní binárka cosmic-ext-app-switcher
  → instalace binárky, uživatelské služby a autostartu
```

Archivy obsahují zdrojové soubory, nikoli předkompilovanou binárku. Závislosti nejsou přibalené jako kompletní offline kopie; Cargo je podle potřeby získává při sestavení.

Python obstarává rozbalení, sestavení, zálohy, instalaci a obnovu. Samotné přepínání oken, historie a vykreslování jsou v Rustu.

## Která verze obsahuje co

| Umístění | Skutečný obsah a vztah k ostatním |
| --- | --- |
| `ver 1.0/gemini-code-1791967416778.md` | Historický návod se shellovými příkazy a náhradou `src/app.rs`. Přidává šipky a potvrzení uvolněním Alt/Super. Ukázka používá `mpsc::SyncSender<ActivateCommand>`, nikoli MRU službu zavedenou ve verzi 1.1. Není to kompletní projekt. |
| `ver 1.1/cosmic_alt_tab_mru_install.py` | Samostatný Python instalátor s kompletním zdrojovým projektem v payloadu. Přidává trvalou MRU službu. Vyžaduje existující binárku původního přepínače. |
| `ver 1.1/cosmic_alt_tab_mru_fixed.py` | Stejná instalační logika, jiný payload. Přidává předpočítání ikon/popisků, odklad zobrazení obsahu a úpravy zavírání UI. |
| `ver 1.1/poznamky.txt` | Dva historické příkazy ke spuštění instalátoru na Linuxu. |
| `ver 1.2/alt_tab12.py` | Prázdný soubor, 0 bajtů. Žádný payload ani instalační logika. |
| `ver 1.2/poznamky22.txt` | Strom zamýšleného projektu a za sebou vložené bloky `main.rs`, `app.rs`, `icons.rs`, `wayland.rs` a Python instalátoru. Nejde o jeden platný Rust/Python soubor. Strom zmiňuje `ui.rs` a `mru.rs`, ale jejich těla zde chybí. |
| `ver 1.3/src/main.rs` | 1 400 řádků: vstup programu a vložené moduly `mru`, `icons`, `app`, `ui`, `wayland`. Logika navazuje na payload `fixed`; nejde o nový kompletní Cargo projekt. |
| `ver 1.3/install.py` | Instalátor čte místní projekt, případně projekt z `--source`. Žádný payload neobsahuje. V jeho vlastní složce chybí `Cargo.toml`, `Cargo.lock` a `config/`. |

Číslování složek se liší od verzí uvnitř souborů: oba Python instalátory 1.1 mají v úvodním textu `v1.0`, zatímco přibalený Cargo workspace uvádí verzi `0.1.5`.

Kořenový `cosmic-zdroje.tar.gz` je podle svého `Cargo.toml` jiný projekt: `cosmic-ws-switcher` se závislostí na `cosmicmsg`. Nepoužívat jeho manifest jako automatickou náhradu chybějícího manifestu Alt‑Tabu 1.3. Tento archiv nebyl rozbalen do Alt‑Tab projektu.

## Čitelné kopie payloadů

Přidané složky jsou přesné kopie původních archivovaných souborů:

- [rozbalene-zdroje/ver-1.1-install](rozbalene-zdroje/ver-1.1-install)
- [rozbalene-zdroje/ver-1.1-fixed](rozbalene-zdroje/ver-1.1-fixed)

Každá obsahuje 19 původních souborů a přidaný `PAYLOAD-PUVOD.json`. Ten zaznamenává cestu k původnímu instalátoru, jeho SHA‑256, SHA‑256 archivu a SHA‑256 každého rozbaleného souboru. Kopie zahrnují také licenci, původní dokumentaci, `config/` a `applet/`.

| Payload | Ověřený SHA‑256 archivu |
| --- | --- |
| `install` | `ab26bb421d405c136a2e1613a26e981c1c1055e5460ba26f88603bbe88b66fa9` |
| `fixed` | `ad35fdb42138d3874d06516521b39638a3a49f347c63d5f9fa902c814ae906ca` |

Pomocník [tools/rozbalit_payloady.py](tools/rozbalit_payloady.py) čte konstanty přes Python AST; původní instalátory neimportuje ani nespouští. Ověřuje kontrolní součet, povoluje jen soubory/adresáře uvnitř cíle a odmítá přepsat odlišné kopie. Následující příkazy se spouštějí z kořene repozitáře a fungují také na Macu:

```sh
# Vytvoření chybějících kopií; existující shodné soubory ponechá.
python3 "alt tab/tools/rozbalit_payloady.py"

# Pouze kontrola shody, bez zápisu souborů.
python3 "alt tab/tools/rozbalit_payloady.py" --check
```

Tyto složky slouží jako referenční základ. Úprava kopie automaticky nezmění payload v původním `.py`. Pro budoucí opravu je potřeba zvolit pracovní zdroje a následně buď předat zdrojový projekt, nebo výslovně znovu zabalit opravený payload a přepočítat jeho kontrolní součet. Zpětné balení zatím přidáno nebylo.

## Jak přepínač funguje

1. Rustový proces s `--daemon` drží spojení s Waylandem. Každému oknu přidělí vlastní číselný klíč. Události aktivace posouvají okna v historii MRU (naposledy použité první). Identitou není název okna ani aplikace.
2. Historie žije v paměti služby a začíná jejím spuštěním. Při nečinnosti služba čeká v `poll()` na Wayland nebo lokální požadavek; neprovádí časované vzorkování aktivního okna.
3. Zkratka spustí binárku bez argumentů, případně s `--reverse`. Pokud už UI existuje, další proces mu pošle `next` nebo `prev` a skončí.
4. Nové UI si vyžádá seřazený seznam oken od služby. Během výběru používá tento neměnný seznam. Dopředný první výběr je index 1, tedy předchozí okno, pokud jsou alespoň dvě.
5. UI zobrazuje lištu přes Wayland Layer Shell. Tab, Shift+Tab, šipky a myš mění výběr; uvolnění Alt/Super nebo Enter jej potvrzuje; Escape a ztráta focusu ruší výběr.
6. Potvrzení pošle službě dvojici `generation` a klíč okna. Služba odmítne zastaralý požadavek po svém restartu nebo okno, které už neexistuje. Historii aktualizují události aktivace od compositoru.

Jsou zde dva odlišné sockety v `XDG_RUNTIME_DIR`:

| Socket | Účel |
| --- | --- |
| `cosmic-ext-app-switcher.sock` | Krátké příkazy `next`/`prev` existujícímu UI. |
| `cosmic-ext-app-switcher-mru.sock` | JSON požadavky `List`, `Activate` a `Ping` pro službu historie. |

## Kde hledat jednotlivé části

V rozbalených verzích jsou části samostatné soubory; v 1.3 jsou vložené do `src/main.rs`:

| Oblast | Soubor v payloadu | Začátek v `ver 1.3/src/main.rs` |
| --- | --- | --- |
| Argumenty, start UI, socket a zámky | `src/main.rs` | řádek 1, `main()` řádek 100 |
| Pořadí a identita oken v historii | `src/mru.rs` | `mod mru`, řádek 163 |
| Ikony a popisky z `.desktop` souborů, XDG/Flatpak/Snap, Steam | `src/icons.rs` | `mod icons`, řádek 201 |
| Stav UI, klávesy, potvrzení, zavírání, čtení socketu | `src/app.rs` | `mod app`, řádek 357 |
| Rozměry, lišta, zkrácení popisku, vykreslování | `src/ui.rs` | `mod ui`, řádek 633 |
| Wayland, služba, komunikace, filtrace oken | `src/wayland.rs` | `mod wayland`, řádek 810 |
| Témata a nastavení rozsahu přepínání | `config/src/lib.rs` | není přibalený do složky 1.3 |
| Panelový applet pro nastavení tématu, rozsahu a zkratky | `applet/src/app.rs` | není přibalený do složky 1.3 |

Applet umí měnit konfiguraci zkratky, ale zdejší Python instalátory sestavují a instalují pouze binárku přepínače. Applet ani konfiguraci zkratek samy neinstalují/nepřepisují.

## Rozdíly důležité pro budoucí opravy

Mezi oběma payloady 1.1 se liší pouze `src/main.rs`, `src/app.rs` a `src/ui.rs`. Zbývajících 16 archivovaných souborů je shodných. Pythonová instalační logika je stejná; mimo payload a kontrolní součet se mění jen názvy skriptu v úvodních příkladech.

Varianta `fixed` předpočítává `visuals`, čeká 28 ms před zobrazením obsahu (`ready`), používá průhledné okno a pevné rozměry buněk i lišty. Při potvrzení/zrušení nastavuje `finishing`, skryje obsah a odstraňuje UI socket. Startovní zámek drží až do získání seznamu oken. Změny zaznamenáváme jako implementaci; jejich účinek na blikání zde nebyl ověřen.

Poznámky 1.2 obsahují obdobu této varianty s přeuspořádanými importy, formátováním a méně komentáři. Python instalátor začíná na řádku 1189 a sestavuje projekt ze složky; před sestavením ještě volá `cargo test`.

Verze 1.3 slučuje logiku do jednoho souboru. Oproti rozbalenému `fixed` chybějí testovací moduly, některé komentáře a diagnostické výpisy; zkrácená je i chyba při chybějících Wayland rozhraních. `install.py` 1.3 už před sestavením `cargo test` nevolá. Rozbalený přepínač 1.1 má 16 unit testů: historie 6, ikony 3, UI 3, kontrola protokolů 4. Applet má další 4 vlastní testy.

`WorkspaceScope::CurrentWorkspace` v posuzovaných MRU verzích vrací všechna okna. Původní `config/src/lib.rs` tuto možnost záměrně skrývá ve výběru. Filtr aktuálního monitoru se řídí výstupy naposledy použitého okna. To je existující omezení zdrojů, nikoli nově provedená změna.

## Instalátor a kontrola této přípravy

Na Linuxu instalátory používají `~/.local/bin/cosmic-ext-app-switcher`, službu `cosmic-ext-app-switcher-mru.service` v uživatelském systemd a XDG autostart. Zálohy jsou v `~/.local/share/cosmic-alt-tab-mru/backups/`; build cache je v `~/.cache/cosmic-alt-tab-mru/target`. Přepínače `--mru-check`, `--mru-list` a `--check-compat` slouží k diagnostice na Linuxu; Python `--rollback` obnovuje předchozí instalaci.

Při této přípravě byly ověřeny SHA‑256 obou payloadů a shoda všech 38 rozbalených původních souborů. Kontrola syntaxe Pythonu proběhla přes AST bez spuštění instalační logiky. Rustové testy, kompilace, služba ani grafické chování na Linuxu se zde neověřovaly. Tvrzení o dřívějším sestavení/testech v přibaleném `MRU-NAVOD.md` jsou historickou dokumentací, nikoli výsledkem tohoto průchodu.
