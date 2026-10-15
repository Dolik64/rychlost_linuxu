# Nekonečné přepínání oken na macOS

Přepínač pro [Hammerspoon](https://www.hammerspoon.org/) inspirovaný
`cosmic-ws-switcher v0.2.0` z vedlejšího souboru.

- **Control + Option + →**: další okno, včetně přechodu z posledního na první.
- **Control + Option + ←**: předchozí okno, včetně přechodu z prvního na poslední.
- Funguje i po úplném puštění a opětovném stisku zkratky i při jejím držení.
- Pořadí se nemění podle posledního použití. Nově zjištěná okna se přidávají na konec.
- Zahrnuje běžná okna různých aplikací na aktuální ploše a monitoru.
- Pomocné dialogy vynechává: některé při aktivaci vracejí fokus na své hlavní okno,
  což by způsobilo přeskakování v cyklu.
- Vynechává minimalizovaná okna a skryté aplikace. Překrytá okna zahrnuje.

## Zapnutí

1. Nainstaluj Hammerspoon z jeho oficiálního webu, spusť jej a povol mu
   **Nastavení systému → Soukromí a zabezpečení → Zpřístupnění**.
2. V nabídce Hammerspoonu vyber **Open Config**. Do konfigurace přidej následující
   řádky; stávající obsah ponech:

```lua
if windowCycle then windowCycle.stop() end
windowCycle = dofile("/Users/jirka/Documents/rychlost_linuxu/tocime okna na plose/macos/window-cycle.lua").start()
```

3. Vyber **Reload Config**. S alespoň třemi okny na jedné ploše opakovaně stiskni
   Control + Option + →. Po posledním okně se musí znovu aktivovat první.
   Vyzkoušej i opačný směr a opakované stisky s úplným puštěním všech kláves.

Skript nenahrazuje systémovou zkratku Control + F4. Má vlastní zkratky;
nepotřebuje přehled oken ani potvrzení výběru. Jejich dostupnost závisí na dalších
globálních zkratkách, včetně případně zapnutého VoiceOveru.

## Úpravy

Jinou kombinaci modifikátorů lze předat do `.start()`, například
`.start({modifiers = {"ctrl", "alt", "cmd"}})`.

Ve výchozím nastavení se vybírá monitor aktivního okna. Pokud chceš zahrnout
aktuálně viditelné plochy na všech monitorech, použij
`.start({onlyCurrentScreen = false})`.

Pro vypnutí odstraň přidané řádky z konfigurace a zvol **Reload Config**.

## Ověření a omezení

**15. 10. 2026: zapnuto na tomto Macu a živě ověřeno na 13 otevřených oknech.**
Kontrola prošla v obou směrech, včetně návratu na první okno, bez vynechání
kteréhokoli zahrnutého okna. Hammerspoon má povolené Zpřístupnění a konfigurace
je načtená z `/Users/jirka/.hammerspoon/init.lua`.

Ověřeno automatickými testy se simulovanými okny: opakované oběhy v obou směrech,
rychlé stisky před dokončením aktivace okna, změny seznamu oken, změna plochy,
výběr monitoru a uvolnění zkratek při vypnutí. Testy spouští `lua test-window-cycle.lua`
v této složce. Ověření skutečného ovládání aplikací na macOS vyžaduje běžící
Hammerspoon s uděleným oprávněním.

Živou kontrolu lze spustit v konzoli Hammerspoonu:

```lua
dofile("/Users/jirka/Documents/rychlost_linuxu/tocime okna na plose/macos/validate-live.lua")
```

Kontrola dočasně aktivuje všechna zahrnutá okna v obou směrech, ověří návrat na
první okno i skutečný fokus a nakonec vrátí původní okno. Během kontroly neklikej
do dalších oken. Konzole vypíše PASS nebo FAIL; protokol obsahuje jen ID oken.

Hammerspoon upozorňuje na zpoždění aktualizace seznamu oken po změně plochy.
Bezprostředně během přechodu mezi plochami je proto potřeba chování ověřit.
Při rychlých stiscích skript až 300 ms navazuje na poslední požadovaný cíl;
ruční přepnutí jinam v tomto krátkém intervalu může být dalším stiskem přehlédnuto.
Při startu vychází pořadí z toho, jak Hammerspoon zjistí již otevřená okna.
Čísla latence linuxové verze nejsou měřením tohoto přepínače.

Dokumentace: [filtr oken](https://www.hammerspoon.org/docs/hs.window.filter.html),
[aktivace okna](https://www.hammerspoon.org/docs/hs.window.html#focus),
[zkratky](https://www.hammerspoon.org/docs/hs.hotkey.html#bind).
