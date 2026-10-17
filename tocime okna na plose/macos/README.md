# Ovládání macOS po vzoru Pop!_OS

**Option nahrazuje Super**. Přesun okna mezi plochami obsluhuje vlastní aplikace
**PopSpace** (`native/build/PopSpace.app`). Ostatní zkratky zůstávají v Hammerspoonu,
který načítá `/Users/jirka/.hammerspoon/init.lua` a spouští se při přihlášení.

| Zkratka | Akce |
|---|---|
| Option + → / ↓ | Další okno, po posledním znovu první |
| Option + ← / ↑ | Předchozí okno, po prvním znovu poslední |
| Option + Shift + šipka | Rozložení aktivního volného okna směrem k okraji |
| Option + Control + → / ↓ | Další běžná plocha |
| Option + Control + ← / ↑ | Předchozí běžná plocha |
| Option + Control + Shift + šipka | Přesun okna na sousední plochu a přechod s ním |
| Option + W | Otevřít / zavřít Mission Control |
| Option + Tab | Dosavadní AltTab |

## Rozkládání oken

Používá přechody volných oken z COSMIC, nikoli automatické dlaždicování všech oken:

- Ze základní velikosti: vlevo, vpravo, nahoru nebo dolů → příslušná polovina.
- Kolmá šipka → roh. Například vlevo, pak nahoru → levá horní čtvrtina.
- Opačná šipka rozšíří danou osu. Levý horní roh + dolů → levá polovina.
- Levá polovina + doprava → maximalizace v použitelné ploše obrazovky.
- Opakovaná šipka ven z již dosaženého okraje okno nechá na místě.
  Přesun na další plochu je záměrně na výslovné kombinaci s Control.
  COSMIC může v tomto bodě přesouvat dál na plochu nebo monitor; tato část není shodná.
- Ruční změna velikosti/polohy se rozpozná; další šipka vychází z aktuálního okna.
- Aplikace si může vynutit minimální velikost. Například zdejší Finder nedovolí
  šířku menší než 843 bodů, zatímco polovina monitoru má 756. Takové okno zůstane
  přisazené ke správnému okraji, ale bude širší než polovina.

## Přesun mezi plochami

PopSpace přesouvá konkrétní okno přímo přes systémové rozhraní macOS.
**Option, Control a Shift mohou zůstat stisknuté**: přesun spouští stisk šipky,
nikoliv uvolnění modifikátorů. Nepřetahuje náhledy v Mission Control ani kurzor.
Kontroluje skutečnou cílovou plochu, aktivní plochu, zaměření a zachování rozměrů.
Další přesun přijme po dokončení probíhajícího; šipku pro další krok stiskni znovu.

V okně PopSpace lze vybrat **Rychlá animace**, **Téměř okamžitě** nebo **Běžná
animace macOS**. První dvě volby posílají Docku gesto s různou rychlostí;
nejde o záruku pevné délky animace. Pro viditelný přechod je systémové
„Omezit pohyb“ nyní vypnuté.

- ← a ↑ znamenají předchozí, → a ↓ následující plochu v aktuálním pořadí macOS.
- Na prvním/posledním místě se nepřeskakuje na opačný konec a nevytvářejí se nové plochy.
- Přesouvají se běžná okna na běžné plochy stejného monitoru.
- Nativní fullscreen a okna připnutá na všechny plochy jsou vynechána.
- Okno se určuje podle systémového ID; shodné názvy nejsou překážkou.
- Rychlé gesto se používá jen mezi bezprostředně sousedními plochami na monitoru
  pod kurzorem. Při přeskočení fullscreen plochy nebo na jiném monitoru se použije
  běžný přechod. Více fyzických monitorů nebylo živě vyzkoušeno.
- Přesun používá neveřejné systémové rozhraní; aktualizace macOS může vyžadovat
  úpravu. Gesto je povoleno pouze na macOS 26. SIP zůstává zapnutý.

PopSpace potřebuje vlastní oprávnění **Zpřístupnění**. Zavření jeho okna ponechá
zapnuté zkratky a ikonu ↔ v horní liště. Vypnout je lze tlačítkem v okně;
ukončení aplikace je rovněž uvolní. Spouštění PopSpace při přihlášení zatím
není nastavené; aplikaci lze otevřít z `native/build/PopSpace.app`.

Sestavení ze zdrojů: `sh native/build.sh`. Aplikaci před sestavením ukonči.
Lokální podpis se při změně programu mění; macOS pak může vyžadovat odebrání
starého záznamu PopSpace ze Zpřístupnění a opětovné přidání nové verze.

## Cyklus oken

Navazuje na linuxový `cosmic-ws-switcher v0.2.0` z vedlejšího souboru.
Pořadí zůstává stálé a nemění se podle posledního použití. Nově zjištěná okna se
přidávají na konec. Zahrnuje běžná viditelná okna aplikací na aktuální ploše
aktuálního monitoru; skryté aplikace, minimalizovaná okna a pomocné dialogy vynechává.
Překrytá okna zahrnuje. Dialogy mohou přesměrovávat fokus na rodiče a rozbíjet cyklus.
Podporuje opakované stisky i držení šipky. Při rychlém vstupu navazuje až 800 ms na
poslední vybraný cíl, než macOS doručí skutečnou změnu fokusu.

## Zjištěné kolize a změny

| Původní chování | Řešení |
|---|---|
| Rectangle: Control + Option + čtyři šipky rozkládaly na poloviny | Tyto čtyři vazby odstraněny; nově přepínají plochy |
| Rectangle: Control + Option + Shift + ↑ zvětšovalo výšku | Vazba odstraněna; nově přesouvá okno na předchozí plochu |
| Původní Hammerspoon: Control + Option + ←/→ cyklovalo okna | Cyklus přesunut na samotné Option + šipky |
| Hammerspoon i PopSpace měly stejné čtyři zkratky pro přesun | Starý mover a jeho přepínač vypnuty přes `nativeSpaces=true`; PopSpace nyní registruje zkratky výhradně |
| AltTab: W v otevřeném přehledu zavíralo vybrané okno | Zavírání v přehledu změněno na Backspace (⌫) |
| Option + W v rozložení US píše ∑ | Převzato pro Mission Control podle požadavku |
| Option + šipky posouvají textový kurzor po slovech/odstavcích | Globálně převzato pro cyklus oken |
| Option + Shift + šipky rozšiřují výběr textu | Globálně převzato pro rozložení oken |

AltTab má nadále Option + Tab a ostatní nastavení; běžné Command + W zavírá okno
jako dříve. Ostatní zkratky Rectangle zůstaly. Aktivních je 13 zkratek
v Hammerspoonu a 4 v PopSpace. VoiceOver používá některé kombinace Control + Option;
při jeho případném zapnutí by bylo nutné profil znovu sladit.

## Ověření

17. 10. 2026:

- Uživatel potvrdil, že po předání zkratek PopSpace začíná přesun ihned při
  držených modifikátorech. Následná oprava obnovuje aktivní okno přes
  `SetFrontProcessWithOptions` a ověřuje fokus podle systémového ID okna.
- Po opravě prošly čtyři živé přesuny Finderu tam/zpět v běžné a maximalizované
  velikosti: **1,50 / 1,09 / 1,06 / 1,11 s**. Ve všech případech souhlasila
  cílová a aktivní plocha, zaměření i geometrie. Jde o dobu do potvrzení výsledku,
  nikoliv délku samotné animace.
- Uživatel následně potvrdil také oba směry na fyzické klávesnici bez puštění
  modifikátorů. Tato funkční verze je archivovaná v
  `backups/2026-10-17-popspace-working/`, včetně podepsané aplikace a kontrolních součtů.
  Další optimalizace probíhají výhradně v `native/PopSpace-dev.m`;
  samostatné sestavení a postup testování popisuje `native/VYVOJ.md`.
- Test předání zkratek ověřuje 13 ponechaných vazeb, vypnutí starého moveru
  a odstranění kolizí při opakovaném spuštění profilu.
- Sestavení a kontrola podpisu PopSpace prošly. Samotné sestavení nepotvrzuje
  funkčnost fyzických kláves ani výslednou animaci.

Historické testy 15.–16. 10. (časy starého moveru se nevztahují na PopSpace):

- Uživatel potvrdil funkčnost fyzických kláves **Option + Shift + ←** a **Option + W**.
- 13 regresních testů cyklu: obě směrové smyčky, rychlé stisky, opožděný fokus,
  přidání/zavření okna, změna plochy, výběr monitoru, pozastavení při přesunu.
- 45 kontrol geometrie: poloviny, rohy, opačné směry, liché rozměry a záporné
  souřadnice druhého monitoru.
- Živě: úplný cyklus přes 17 oken v obou směrech, včetně návratu na začátek.
  Nejdelší čekání na potvrzení fokusu bylo 0,52 s. Po úpravě návaznosti rychlých
  stisků prošel 16. 10. i celý cyklus šesti oken aktuální plochy v obou směrech
  (nejdelší potvrzení fokusu 0,16 s).
- Živě: 32 změn rozložení v TextEditu a Finderu; 24 přesných a 8 s respektováním
  minimální velikosti Finderu. Původní rozměry testovaných oken byly obnoveny.
- Živě: čtyři přesuny testovacího okna TextEditu mezi dvěma plochami tam a zpět.
  Každý ověřil skutečné členství na ploše, aktivní plochu, fokus i zachování rámu.
  Po zrychlení 16. 10. prošly další čtyři přesuny za 2,18 / 2,25 / 1,95 / 2,00 s.
  Skript čeká na skutečnou připravenost a výsledek jednotlivých kroků;
  po přetažení znovu načte cílovou plochu z obnoveného přehledu.
- Živě: přepnutí na sousední plochu a zpět bez přesunu původního okna;
  pokus jít před první plochu ji správně ponechal aktivní.
- Automaticky zasílané klávesy nástroje pro ovládání aplikací neaktivovaly globální
  Hammerspoon zkratky. Živé automatické testy proto volají stejné akce z profilu;
  nejsou samy o sobě důkazem průchodu fyzické klávesy přes ostatní aplikace.

Testy bez ovládání skutečných oken se spouštějí v této složce:

```sh
lua test-window-cycle.lua
lua test-snap-layout.lua
lua test-fast-space-mover.lua
lua test-native-handoff.lua
```

Živé testy lze spustit v konzoli Hammerspoonu pomocí `dofile` s absolutní cestou:
`validate-live.lua` pro cyklus a `validate-layout-live.lua` pro rozložení.
`validate-spaces-live.lua` ověřoval původní, nyní neaktivní mover. Přepínání ploch samostatně ověřuje
`validate-navigation-live.lua`. Testy rozložení a přesunů očekávají testovací dokument
`Codex-window-test-20261015.rtf` v TextEditu; není součástí běžného používání.
Během testů neklikej do jiných oken. Výsledky se vypisují do konzole.

Vlastní PopSpace má tlačítko **Test tam a zpět**. Použije pouze okno Finderu nebo
TextEditu s názvem začínajícím `Codex-`: normální velikost tam/zpět, maximalizace
tam/zpět, pak obnova původních rozměrů. Výsledek v jeho okně uvádí ověřenou plochu,
fokus, geometrii a dobu přesunu. Testovací okno musí být dostupné na aktuální ploše.
Při selhání může zůstat na cílové ploše. Test nespouštěj na důležitém dokumentu.

## Záloha a vypnutí

Původní konfigurace a exporty nastavení jsou v místní složce `backups/`
s prefixem `2026-10-15-before-option-shortcuts-`. Verze přesunu před zrychlením
je také uložená jako `2026-10-16-before-speed-space-mover.lua`. Složka je vyloučená z Gitu.
Konfigurace před předáním zkratek PopSpace je v `backups/2026-10-17-before-native-handoff/`.
Původní uživatelský mover zůstává zachovaný na disku, ale aktivní profil jej nespouští.

Dočasné vypnutí celého profilu v konzoli Hammerspoonu: `linuxControls.stop()`.
Opětovné zapnutí: `linuxControls.start({nativeSpaces=true})` nebo Reload Config.
Samostatný PopSpace tímto příkazem nevypneš; ukonči jej v jeho nabídce.

Pro návrat k původnímu cyklu zkopíruj zálohu `init.lua` do
`/Users/jirka/.hammerspoon/init.lua` a zvol Reload Config. Tím se neobnoví nastavení
Rectangle ani AltTab. Jejich změněné vazby lze vrátit jednotlivě podle tabulky výše;
případně lze po ukončení dané aplikace obnovit celý její export nastavení ze zálohy.
Celý export by nahradil i případné pozdější úpravy.

## Zdroje

[COSMIC – oficiální zkratky](https://support.system76.com/articles/pop-cosmic-keyboard-shortcuts/),
[COSMIC – logika volných oken](https://github.com/pop-os/cosmic-comp/blob/master/src/shell/layout/floating/mod.rs),
[Hammerspoon – Spaces](https://www.hammerspoon.org/docs/hs.spaces.html),
[Hammerspoon – zkratky](https://www.hammerspoon.org/docs/hs.hotkey.html),
[yabai – přímý přesun na macOS 26](https://github.com/asmvik/yabai/issues/2788),
[InstantSpaceSwitcher – gesto pro Dock](https://github.com/jurplel/InstantSpaceSwitcher).
Licence převzatých konstant gesta je v `native/THIRD-PARTY-NOTICES.txt`.
