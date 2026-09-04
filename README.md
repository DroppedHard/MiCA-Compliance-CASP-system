# CASP rUSD

[English version](README.en.md)

Repozytorium zawiera demonstracyjną platformę dostawcy usług w zakresie kryptoaktywów obsługującego rUSD. CASP prowadzi własny rejestr praw klientów, zarządza wspólnymi portfelami powierniczymi i kupuje pulę tokenów od emitenta. Projekt jest środowiskiem badawczym, a nie usługą produkcyjną.

## Elementy systemu

- `backend/` — API i rejestr CASP w Rust, SQLite, custody, prowizje i raportowanie;
- `frontend/` — aplikacja React z widokiem klienta i administratora;
- `docs/` — szczegółowa dokumentacja domenowa;
- `compose.yaml` — lokalne wdrożenie CASP łączące się z osobno uruchomionym emitentem.

CASP nie czyta bazy emitenta. Korzysta z jego publicznego API, mockBanku i blockchaina przez jawnie skonfigurowane adresy.

## Szybkie uruchomienie

Najpierw uruchom repozytorium emitenta. Następnie wykonaj:

```powershell
docker compose up --build --detach --wait
```

Domyślnie Docker Desktop łączy się z emitentem przez `host.docker.internal`. Dostępne są:

- portal klienta: `http://127.0.0.1:5174`;
- panel administratora: `http://127.0.0.1:5174/admin`;
- API CASP: `http://127.0.0.1:3200`.

Adresy integracji można nadpisać przed startem:

```powershell
$env:ISSUER_URL="http://host.docker.internal:3000"
$env:RPC_URL="http://host.docker.internal:8545"
$env:MOCK_BANK_URL="http://host.docker.internal:3100"
docker compose up --build --detach --wait
```

Zatrzymanie i wyzerowanie danych:

```powershell
docker compose down
docker compose down --volumes
```

Klucze z Compose należą do publicznych kont Hardhat i służą wyłącznie lokalnemu demo.

## Model działania

Przy pierwszym uruchomieniu CASP kupuje od emitenta 10 000 rUSD. Emisja trafia na portfel gorący, a rebalans przenosi 80% puli do portfela zimnego. Zakup, sprzedaż i przelew między klientami zmieniają prawa zapisane w SQLite i nie wykonują transakcji blockchainowej. Operacje zewnętrzne korzystają z portfela gorącego.

## Testy API uruchomionego CASP

Scenariusze P0 wymagają działającego emitenta, mockBanku, sieci Hardhat i
backendu CASP. Środowisko Pythona można przygotować następująco:

```powershell
python -m venv .venv
.\.venv\Scripts\python.exe -m pip install -r .\scripts\lib.txt
```

Runner korzysta obecnie wyłącznie z biblioteki standardowej, więc `lib.txt`
jest stabilnym, pozbawionym zależności manifestem dla venv.

Po uruchomieniu obu projektów Compose wykonaj:

```powershell
.\.venv\Scripts\python.exe .\scripts\run-p0-api-tests.py
```

Skrypt sprawdza przykłady CA-01–CA-06 i jest mutujący: pozostawia rekordy
audytowe oraz niewielkie zmiany puli i sald. Używa unikalnych identyfikatorów,
zwraca niezerowy kod przy błędzie i zapisuje raport w
`test-results/api-p0-casp-*.json`. Szczegóły zawiera dokument
[scenariuszy API P0 CASP](docs/pl/p0-api-tests.md).

## Weryfikacja i dalsza dokumentacja

- [backend CASP i endpointy](docs/pl/backend.md);
- [frontend klienta i administratora](docs/pl/frontend.md);
- [indeks dokumentacji CASP](docs/pl/README.md);
- [model rejestru operacji](docs/pl/service-records.md).
- [scenariusze API P0 CASP](docs/pl/p0-api-tests.md).

Podstawowa weryfikacja to `cargo test`, `cargo clippy`, `npm.cmd test` i `npm.cmd run build`. Testy całego przepływu dwóch instytucji zostaną dopasowane później. Repozytorium celowo nie zawiera GitHub Actions.
