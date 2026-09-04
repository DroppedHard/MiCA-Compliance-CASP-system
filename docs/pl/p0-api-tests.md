# Scenariusze API P0 CASP

Skrypt `scripts/run-p0-api-tests.py` wykonuje na uruchomionym wdrożeniu
przykładowe przebiegi CA-01–CA-06. Wymaga działającego emitenta, mockBanku,
łańcucha Hardhat i backendu CASP.

Sprawdzane są:

- idempotentny bootstrap i docelowe wartości 20/80;
- ręczne zasilenie puli, jego idempotencja i rebalans;
- jednokrotne księgowanie zakupu klienta;
- przelew wewnętrzny z prowizją 0,1%;
- dwa współbieżne żądania próbujące wydać to samo saldo;
- idempotentna sprzedaż oraz wykup przekazany emitentowi.

Po uruchomieniu obu Compose wykonaj w repozytorium CASP:

```powershell
python .\scripts\run-p0-api-tests.py
```

Skrypt jest mutujący i pozostawia rekordy operacji w SQLite oraz niewielkie
zmiany puli i sald demonstracyjnych. Używa unikalnego prefiksu dla każdego
przebiegu, dlatego można go ponawiać bez konfliktu identyfikatorów. Nie czyści
wolumenów i nie powinien być kierowany do środowiska produkcyjnego.

Raport JSON jest zapisywany w `test-results/api-p0-casp-*.json`. Dokładne
asercje dotyczące liczby wywołań portów oraz stanu SQLite pozostają w testach
integracyjnych Rusta; skrypt Pythona potwierdza zachowanie tego samego procesu
przez rzeczywistą granicę HTTP kontenera.
