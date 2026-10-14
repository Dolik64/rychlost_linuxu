# COSMIC Alt+Tab s historii pouziti oken

Uprava z 14. 10. 2026, zalozena na upstream revizi
28c9425de4570a3fb599ef29d5498b77a33ca817 projektu
https://github.com/j0rdiun/cosmic-ext-app-switcher .
Puvodni licence je soucasti zdroju. Upravy jsou poskytovany za stejne licence.

## Chovani

- Historie je po jednotlivych oknech (Windows Alt+Tab), nikoliv po aplikacich
  (macOS Cmd+Tab). Dve okna stejne aplikace maji vlastni identitu a pozici.
- Aktivace pres mys, dock i ostatni zkratky aktualizuje historii pomoci Wayland
  udalosti. Zmena titulku, pouhe otevreni okna bez aktivace a opakovane hlaseni
  stejneho aktivniho stavu historii neprepise.
- Pri otevreni prepinace vznikne nemenny seznam. Prvni Tab vybere predchozi
  pouzite okno. Dalsi Tab/sipka vpravo prochazi starsi okna. Shift+Tab/sipka
  vlevo jde zpet. Uvolneni Alt/Super nebo Enter potvrdi, Escape zrusi.
- Pri zavreni okna behem vyberu se neaktivuje jine okno misto nej.
- Historie se uchovava pouze v RAM, od spusteni sluzby. Poradi pred jejim startem
  nelze rekonstruovat. Po restartu sluzby postupne aktivuj otevrena okna.
- Sluzba spi v poll(), pokud neprichazeji udalosti. Neprovadi periodicke dotazy
  ani opakovane spousteni cosmicmsg.
- Zachovava stavajici vzhled, nastaveni rozsahu a uzivatelskou upravu sipkami.
  Omezeni upstreamu pro rozsah CurrentWorkspace (stare protokoly) zustava;
  z teto upravy se nestava filtr jedne plochy. cosmic-ws-switcher zustava samostatny.

## Instalace

Spust Python instalator v terminalu bez sudo, uvnitr COSMIC. Obsahuje tyto zdroje,
Cargo.lock a testy. Sit je nutna jen pro chybejici zavislosti Cargo.
Potrebujes Rust/Cargo a vyvojove knihovny jako pri puvodnim sestaveni.

Instalator nejprve sestavi a otestuje novy program, potom provede zalohu
puvodni binarky a vlastnich servisnich souboru, atomickou vymenu a kontrolu sluzby.
Pri chybe instalace po zaloze se pokusi automaticky obnovit puvodni soubory.
Nesaha na konfiguraci klavesovych zkratek ani cosmic-ws-switcher.

Sluzba: cosmic-ext-app-switcher-mru.service (systemd --user).
XDG autostart spousti pri prihlaseni skript, ktery preda aktualni Wayland
prostredi systemd a nastartuje sluzbu. PartOf=graphical-session.target
zajisti vazbu na graficke sezeni.

## Overeni na vlastni plose

1. Mysí aktivuj Terminal, Nastaveni a Firefox v tomto poradi.
2. Kratke Alt+Tab ma vratit Nastaveni, dalsi kratke Alt+Tab Firefox.
3. Podrz Alt a opakovanym Tab/sipkami projdi seznam. Behem vyberu se nepreskupuje.
4. Escape ma zachovat puvodni okno.
5. Otevri dve okna stejne aplikace a over, ze se prepina mezi obema.
6. Po odhlaseni/prihlaseni over automaticky start sluzby.

Diagnostika:

```bash
~/.local/bin/cosmic-ext-app-switcher --mru-check
~/.local/bin/cosmic-ext-app-switcher --mru-list
systemctl --user status cosmic-ext-app-switcher-mru.service
journalctl --user -u cosmic-ext-app-switcher-mru.service -n 50 --no-pager
```

--mru-list obsahuje nazvy aktualnich oken; pred sdilenim zkontroluj soukromy obsah.

## Obnova

Spust stejny Python instalator s parametrem --rollback. Obnovi posledni zalohu
binarky, jednotky a autostartu. Pracovni zdroje a zalohy ponecha na disku.

## Overeni pri priprave

Uspesne sestaven cely program (cargo build --locked), cargo check a vsech 16
testu (cargo test --locked --bin cosmic-ext-app-switcher). Overena i obnova
souboru, opravneni a symbolickeho odkazu instalatorem.
Testy historie overuji predchozi okno, opakovana hlaseni, ztratu focusu pri overlay,
zavrena okna, vice oken jedne aplikace a inicializaci bez historie.
Skutecne graficke sezeni COSMIC neni v priprave k dispozici; finalni test reakce
na Alt a automatickeho startu je nutne provest na uzivatelove plose.
