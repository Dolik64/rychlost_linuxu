# Ovládání macOS po vzoru Pop!_OS

Aktivní profil pro Hammerspoon. **Option nahrazuje Super**.
Načítá se přes `/Users/jirka/.hammerspoon/init.lua`; Hammerspoon se spouští při přihlášení.

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

Přesun probíhá přes náhledy v Mission Control. Běžné rozhraní Hammerspoonu na tomto
Macu vracelo úspěch, ale okno jiné aplikace se ve skutečnosti nepřesunulo.
Nový postup skutečné umístění, aktivní plochu a fokus kontroluje.

Po stisku **pusť modifikátory**. Přesun obvykle trvá přibližně 3–5 sekund;
na chvíli se zobrazí Mission Control a pohne kurzor. Po dokončení se kurzor vrátí.
Během přesunu nehýbej myší. Escape přesun zruší. Další naše zkratky během
přesunu nevstupují do rozpracované operace.

- ← a ↑ znamenají předchozí, → a ↓ následující plochu v aktuálním pořadí macOS.
- Na prvním/posledním místě se nepřeskakuje na opačný konec a nevytvářejí se nové plochy.
- Přesouvají se běžná okna na běžné plochy stejného monitoru.
- Nativní fullscreen a okna připnutá na všechny plochy jsou vynechána.
- Mission Control neposkytuje ID náhledů oken. Skript proto vyžaduje jediný přesně
  odpovídající název na daném monitoru. Dva stejné názvy nebo zkrácený název mohou
  přesun znemožnit; v takovém případě se zobrazí zpráva a žádný náhled se nepřetahuje.
- Rozhraní Mission Control se může změnit s aktualizací macOS. Více fyzických
  monitorů nebylo v této relaci živě vyzkoušeno.

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
| AltTab: W v otevřeném přehledu zavíralo vybrané okno | Zavírání v přehledu změněno na Backspace (⌫) |
| Option + W v rozložení US píše ∑ | Převzato pro Mission Control podle požadavku |
| Option + šipky posouvají textový kurzor po slovech/odstavcích | Globálně převzato pro cyklus oken |
| Option + Shift + šipky rozšiřují výběr textu | Globálně převzato pro rozložení oken |

AltTab má nadále Option + Tab a ostatní nastavení; běžné Command + W zavírá okno
jako dříve. Ostatní zkratky Rectangle zůstaly. Registrace všech 17 nových zkratek
v Hammerspoonu prošla. VoiceOver používá některé kombinace Control + Option;
při jeho případném zapnutí by bylo nutné profil znovu sladit.

## Ověření 15. 10. 2026

- Uživatel potvrdil funkčnost fyzických kláves **Option + Shift + ←** a **Option + W**.
- 13 regresních testů cyklu: obě směrové smyčky, rychlé stisky, opožděný fokus,
  přidání/zavření okna, změna plochy, výběr monitoru, pozastavení při přesunu.
- 45 kontrol geometrie: poloviny, rohy, opačné směry, liché rozměry a záporné
  souřadnice druhého monitoru.
- Živě: úplný cyklus přes 17 oken v obou směrech, včetně návratu na začátek.
  Nejdelší čekání na potvrzení fokusu bylo 0,52 s.
- Živě: 32 změn rozložení v TextEditu a Finderu; 24 přesných a 8 s respektováním
  minimální velikosti Finderu. Původní rozměry testovaných oken byly obnoveny.
- Živě: čtyři přesuny testovacího okna TextEditu mezi dvěma plochami tam a zpět.
  Každý ověřil skutečné členství na ploše, aktivní plochu, fokus i zachování rámu.
- Automaticky zasílané klávesy nástroje pro ovládání aplikací neaktivovaly globální
  Hammerspoon zkratky. Živé automatické testy proto volají stejné akce z profilu;
  nejsou samy o sobě důkazem průchodu fyzické klávesy přes ostatní aplikace.

Testy bez ovládání skutečných oken se spouštějí v této složce:

```sh
lua test-window-cycle.lua
lua test-snap-layout.lua
```

Živé testy lze spustit v konzoli Hammerspoonu pomocí `dofile` s absolutní cestou:
`validate-live.lua` pro cyklus, `validate-layout-live.lua` pro rozložení a
`validate-spaces-live.lua` pro přesuny. Poslední dva očekávají testovací dokument
`Codex-window-test-20261015.rtf` v TextEditu; není součástí běžného používání.
Během testů neklikej do jiných oken. Výsledky se vypisují do konzole.

## Záloha a vypnutí

Původní konfigurace a exporty nastavení jsou v místní složce `backups/`
s prefixem `2026-10-15-before-option-shortcuts-`. Je vyloučená z Gitu.

Dočasné vypnutí celého profilu v konzoli Hammerspoonu: `linuxControls.stop()`.
Opětovné zapnutí: `linuxControls.start()` nebo Reload Config.

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
[Drag.spoon – přístup přes Mission Control](https://github.com/mogenson/Drag.spoon).
