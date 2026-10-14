# COSMIC Alt‑Tab MRU 1.4.0

Nová pracovní verze z 14. 10. 2026. Obsahuje celý zdrojový projekt a instalátor pro ruční spuštění na Linuxu. Verze 1.0–1.3 a původní Python payloady jsou ponechané beze změny.

**Stav ověření:** na Macu prošlo 7 testů instalátoru se simulovaným sestavením a systémovými příkazy. Rustové sestavení, 27 testů přepínače a grafické chování v COSMIC zatím nejsou ověřené; zde není dostupný Cargo/Rust ani Linuxová relace. Tato verze není potvrzenou opravou zaznamenaných segfaultů nebo zamrzání compositoru.

## Co se změnilo

- `src/ipc.rs`: UI drží zámek až do návratu hlavní smyčky a úklidu. Další stisky zkratky se posílají přímo běžícímu UI, takže nepotřebují tento zámek získat. Druhá instance nemůže během zavírání vytvořit další povrch.
- Socket má jednoho vlastníka. Při odstranění kontroluje zařízení a inode; starý úklid nesmaže nahrazený socket. Z `Activate`, `Cancel` a konce `main()` zmizelo opakované mazání pathname.
- Předaný `next`/`prev` dostává odpověď. Zavírací UI odpovídá „zkus znovu“; nové spuštění čeká omezenou dobu na konec vlastníka. Nejasné doručení při chybě spojení se hlásí jako chyba, nikoli jako úspěch či důvod ke smazání cizího socketu. Při takové chybě se stisk automaticky nepřehrává, aby se výběr neposunul dvakrát.
- `src/app.rs`: odstranění layer surface a konec procesu se neposílají současně. Po `destroy_layer_surface` aplikace čeká na `LayerEvent::Done`; případnou aktivaci vybraného okna pak provede mimo UI vlákno a teprve po odpovědi ukončí proces. Pokud do 2 sekund nepřijde potvrzení odstranění, zapíše chybu a ukončí se bez aktivace.
- `src/lifecycle.rs`: obsah lišty zůstává skrytý, dokud neuplyne 35 ms **a** nepřijde první `Window::Opened` z konfigurace layer surface. Pozdní časovač během zavírání panel znovu neodkryje. Tři sekundy bez konfigurace vyvolají zavření místo čekání bez konce.
- `src/icons.rs`: `.desktop` soubory, cesty i bajty ikon se načítají před vytvořením povrchu. `view()` klonuje připravené `icon::Handle`; nevyhledává názvy ikon ani nečte `.desktop` soubory. Dekódování a nahrání textur dál zajišťuje renderer.
- `src/ui.rs`: zachované pevné rozměry kořene, lišty a ikon; `ContentFit::Contain` zachovává poměr stran ikony.
- Trvalá chyba UI listeneru ukončí UI místo těsné smyčky opakovaných `accept()`. MRU služba rozpozná ukončený/chybný descriptor z `poll()`.
- `CurrentWorkspace` vrací výslovnou chybu, protože původní protokol neposkytuje spolehlivou filtraci této volby. Už tiše nevrací všechna okna. Běžně nabízené režimy „All workspaces“ a „Current monitor“ zůstávají.
- Služba má `RestartSec=5`, `StartLimitIntervalSec=60` a `StartLimitBurst=3`, aby se při opakovaném pádu stále dokola nespouštěla.
- Release build uchovává symboly a základní ladicí informace pro rozbor případného dalšího pádu.

MRU algoritmus v `src/mru.rs` včetně původních testů je bajtově shodný s výchozím payloadem `fixed`. Šipky, Tab, Shift+Tab, Enter, Escape a potvrzení uvolněním Alt/Super jsou zachované. Snímek pořadí oken se během výběru nepřeskupuje.

## Ruční kontrola a instalace na Linuxu

Přenes **celou složku `ver 1.4`**, včetně `src/`, `config/`, `applet/`, `Cargo.toml` a `Cargo.lock`. `install.py` zde nemá vložený payload a samostatně nestačí. Vyžaduje Rust/Cargo podporující edition 2024, `pkg-config`, vývojové knihovny Wayland/xkbcommon a uživatelský systemd. Nastavená zkratka má už ukazovat na `~/.local/bin/cosmic-ext-app-switcher`.

V terminálu otevřeném v této složce, uvnitř přihlášeného COSMIC a **bez sudo**, můžeš nejprve spustit:

```sh
python3 install.py --check-only
```

Provede Rustové testy, release sestavení, kontrolu verze výsledné binárky a kontrolu dostupných Wayland rozhraní. Může stáhnout chybějící připnuté závislosti a zapisuje build cache; nemění instalovanou binárku, službu ani autostart.

Samotná instalace:

```sh
python3 install.py
```

Instalátor také nejprve provede všechny uvedené kontroly. Před výměnou vyžaduje zavřený přepínač, uloží zálohu a vymění pouze vlastní binárku, službu a autostart. Při chybě po vytvoření zálohy se pokusí obnovit předchozí stav. Během výměny nepoužívej Alt‑Tab ani staré instalátory. Starší skripty z historických složek stále mohou přepsat novější instalaci, pokud je ručně spustíš.

Obnova poslední uložené instalace:

```sh
python3 install.py --rollback
```

Tato verze na macOS odmítne instalaci i rollback ještě před vytvářením instalačních adresářů. Nemění `swaync`, `nvidia-settings`, ovladače ani systémové knihovny. Kompilace používá výchozí limit dvou úloh; nastavené `CARGO_BUILD_JOBS` respektuje.

## Co ověřit v COSMIC

1. Po instalaci ověř `~/.local/bin/cosmic-ext-app-switcher --version` — očekává se `1.4.0` — a `--mru-check`.
2. Myší aktivuj tři různá okna. Opakovaný krátký Alt‑Tab má střídat dvě poslední. Vyzkoušej také dvě okna stejné aplikace.
3. Podrž Alt, procházej Tabem, Shift+Tabem a šipkami. Escape má zrušit výběr; Enter nebo uvolnění Alt jej potvrdit.
4. Několikrát rychle zopakuj celé stisknutí a uvolnění Alt‑Tab. Sleduj blikání, ztracené stisky, visící overlay a souběžné UI procesy.
5. Zavři vybrané okno během přepínání; program nemá aktivovat jiné okno pod stejnou identitou.
6. Ověř obvyklé měřítko obrazovky a monitory, které používáš. Zvlášť zaznamenej, zda se problém objevuje jen na určitém výstupu.
7. Po odhlášení/přihlášení zkontroluj start MRU služby. Historie se po restartu služby sbírá znovu.

Pokud dojde k pádu, užitečné jsou tyto čtecí příkazy na Linuxu:

```sh
~/.local/bin/cosmic-ext-app-switcher --version
systemctl --user status cosmic-ext-app-switcher-mru.service --no-pager
journalctl --user -u cosmic-ext-app-switcher-mru.service -n 100 --no-pager
coredumpctl list --since "15 minutes ago" --no-pager
```

Pro PID odpovídající novému pádu následně `coredumpctl info PID --no-pager`, případně `coredumpctl debug PID` a v GDB `thread apply all bt`. `PID` nahraď číslem z aktuálního seznamu. `RUST_BACKTRACE=1` zachycuje Rustové paniky; samo o sobě nevysvětlí SIGSEGV v nativní knihovně. Dostupnost core dumpů a GDB závisí na Linuxovém systému.

## Diagnostika: co je doložené a co zatím ne

Závod při několikanásobném odstranění socketu byl doložený kódem a je zde odstraněný. Ukončování pomocí `Task::batch([destroy, exit])` neposkytovalo požadované pořadí; nyní se čeká na událost z toolkitové implementace.

Samotné `UnixStream::connect()` nemusí selhat jen proto, že aplikace přestala volat `accept()`; spojení může ještě přijmout kernelový backlog. Proto nová varianta používá potvrzení příkazu, nikoli jen úspěšný `connect()` jako důkaz funkčního UI.

Pevná prodleva 35 ms není potvrzení konfigurace, GPU fence ani záruka správného stride. Neshoda velikosti layoutu sama o sobě nedokazuje poškození řádkování textury. Časová prodleva zde slouží jen ke skrytí krátkého vyvolání a je doplněná událostí konfigurace. Ani načtení ikon do paměti nepotvrzuje dokončené vykreslení GPU.

Záznamy SIGSEGV v `libwayland-client` vyžadují úplnou cestu k binárce a zásobník konkrétního pádu. Chyby `swaync.service` nebo `nvidia-settings-autostart` samy nepotvrzují reset NVIDIA ani příčinu zamrznutí. Hlášení compositoru o importu textury je další samostatný podklad k prověření. Tato úprava proto neslibuje odstranění všech systémových dropů.

Zkontrolované související reporty compositoru: [#2768, import mezi GPU](https://github.com/pop-os/cosmic-comp/issues/2768) a [#2655, import/renderování na jedné NVIDIA](https://github.com/pop-os/cosmic-comp/issues/2655). Podobné hlášení v reportu neprokazuje stejnou příčinu na tomto počítači.

## Původ, implementační podklady a testy

Základ: čitelný payload `rozbalene-zdroje/ver-1.1-fixed`, jehož logika odpovídá sloučené variantě 1.3. Obnovuje také její chybějící Cargo workspace a původní testy. Python instalátor vychází z 1.3. Verze všech tří místních Cargo balíčků je `1.4.0`; verze externích závislostí v `Cargo.lock` se nezměnily.

Ověřený zdroj závislostí, podle kterého je napsané čekání na události:

- libcosmic `417923fbafc0f38a63846b307036e4f20fc4960d`; [ikony a handles](https://github.com/pop-os/libcosmic/blob/417923fbafc0f38a63846b307036e4f20fc4960d/src/widget/icon/handle.rs).
- Jeho Iced podmodul `7afd9ee0f83cfd1a4e9f9b56cf0f365e5ae60347`; [převod Configure na Opened a zpracování Done](https://github.com/pop-os/iced/blob/7afd9ee0f83cfd1a4e9f9b56cf0f365e5ae60347/winit/src/platform_specific/wayland/sctk_event.rs).
- [Destroy a navazující Done](https://github.com/pop-os/iced/blob/7afd9ee0f83cfd1a4e9f9b56cf0f365e5ae60347/winit/src/platform_specific/wayland/event_loop/state.rs), [implementace Task::chain a Task::batch](https://github.com/pop-os/iced/blob/7afd9ee0f83cfd1a4e9f9b56cf0f365e5ae60347/runtime/src/task.rs). Samotné `chain()` řadí streamy úloh; nenahrazuje potvrzení dokončeného odstranění povrchu.

Lokální kontrola instalátoru (bez Cargo, systemd a skutečné instalace):

```sh
python3 -B -m unittest discover -s tests -v
```

Sedm testů bylo úspěšně spuštěno na Macu. Test živého socketu potřeboval povolit lokální Unix socket mimo omezení sandboxu. Testy sestavení a instalace používají simulované příkazy; jejich úspěch neznamená, že Rust byl zkompilovaný.

V `src/` je připraveno 27 Rustových testů: 16 původních, 5 pro IPC/vlastnictví socketu, 5 pro pořadí konfigurace a zavírání, 1 pro odmítnutí nepodporovaného rozsahu. Spouštějí se při Linuxové instalaci. Historický applet obsahuje ještě 4 vlastní testy; běžný instalátor applet nesestavuje ani neinstaluje.
