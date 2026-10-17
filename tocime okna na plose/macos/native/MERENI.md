# Měření vývojového PopSpace — 17. 10. 2026

Časy se vztahují k potvrzení přesunu, cílové plochy, fokusu a geometrie;
nejsou měřením jednotlivých snímků animace. Testovací okno Finderu, vždy
normální tam/zpět, maximalizované tam/zpět. Původní rám okna se obnovuje.

## Výchozí vývojové sestavení, rychlost gesta 15

Čekání na nepřijaté gesto zkráceno z 800 na 120 ms. Všechny čtyři přesuny prošly,
ale rychlejší nejsou. V každém se použila náhradní aktivace okna.

| Přesun | Členství cílové plochy | Aktivní cílová plocha | Fokus | Celkem |
|---|---:|---:|---:|---:|
| Normální tam | 35 ms | 1248 ms | 1308 ms | 1,40 s |
| Normální zpět | 39 ms | 1218 ms | 1282 ms | 1,36 s |
| Maximalizované tam | 39 ms | 1331 ms | 1345 ms | 1,45 s |
| Maximalizované zpět | 49 ms | 1576 ms | 1673 ms | 1,82 s |

Systémová funkce `SLSManagedDisplayIsAnimating` nehlásila animaci. To samo o sobě
nedokazuje, že se vizuálně nic nepohybovalo; tento údaj nelze považovat za měření
plynulosti. Rozhodující je čas skutečné změny aktivní plochy a výsledné zaměření.
