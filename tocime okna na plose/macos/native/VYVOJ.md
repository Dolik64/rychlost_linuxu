# PopSpace: funkční a vývojová verze

## Funkční verze — 17. 10. 2026

Uživatel potvrdil přesun tam i zpět při držených Option + Control + Shift.
Rychlost zatím neodpovídá Pop!_OS. Tuto verzi při dalším vývoji nepřepisuj.

- Zdroj: `PopSpace.m`, gesto: `swipe.h`.
- Sestavení: `build.sh` → `build/PopSpace.app`.
- Úplná ověřená záloha: `../backups/2026-10-17-popspace-working/`.
- Záloha obsahuje i již podepsanou aplikaci, export nastavení, konfiguraci
  Hammerspoonu, kontrolní součty SHA-256 a postup obnovení `OBNOVENI.md`.

## Vývoj rychlosti

- Zdroj: **`PopSpace-dev.m`**, gesto: **`swipe-dev.h`**.
- Sestavení: `sh build-dev.sh` → `build-dev/PopSpaceDev.app`.
- Samostatný identifikátor: `cz.jirka.PopSpace.Dev`; vlastní nastavení a oprávnění.
- Při spuštění jsou globální zkratky vždy vypnuté. Testovací tlačítko může běžet
  vedle funkčního PopSpace. Během testu nepoužívej klávesové zkratky funkční verze.
- Fyzické zkratky vývojové verze lze zapnout až po jejich vypnutí ve funkční
  verzi. Výhradní registrace brání současnému převzetí stejných kombinací.

První vývojová úprava zkracuje čekání na nepřijaté gesto z 800 na 120 ms.
Pokud Dock už animuje, čeká na jeho skutečné dokončení. Ověření cílové plochy,
zaměření, viditelnosti a geometrie zůstává zachováno včetně 80 ms stabilního stavu.
Výsledek rozlišuje přijaté gesto od náhradní aktivace a měří od spuštění:

1. Potvrzení členství okna v cílové ploše.
2. Aktivaci cílové plochy.
3. Konec přechodu.
4. Obnovení zaměření okna.

Samotné zkrácení limitu nezaručuje zrychlení: pokud prodlevu způsobuje probíhající
animace, nemusí mít vliv. Nejdřív změřit oba směry, běžné i maximalizované okno,
pak případně upravit rychlost gesta. Porovnávat celkové časy i časy jednotlivých kroků.

Stav: sestavení, systémové symboly a podpis ověřeny. Živý test vývojové verze
zatím neproběhl; vyžaduje její samostatné oprávnění Zpřístupnění.

`symbols.h` a licenční soubor jsou společné. Pokud bude potřeba měnit systémové
symboly, vytvoř nejprve samostatnou vývojovou kopii i tohoto souboru.
