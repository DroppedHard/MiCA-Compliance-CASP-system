# Backend CASP

Serwis Rust prowadzi niezależny rejestr praw klientów i komunikuje się z emitentem przez HTTP. SQLite przechowuje pozycje klientów, zapas, prowizje i historię operacji; blockchain dostarcza salda portfeli custody.

## Struktura kodu

Warstwa `src/api` jest celowo cienka: `router` scala trasy, `routes` deklaruje adresy HTTP, `requests` i `validators` opisują oraz sprawdzają dane wejściowe, a `responses` mapuje błędy domenowe na stabilne odpowiedzi HTTP. Handlery są rozdzielone na `public`, `administration` i `customer`; wywołują wyłącznie usługi przez `AppState`.

Przypadki użycia są w `src/services`, modele i reguły biznesowe w `src/domain`, a implementacje portów w `src/infrastructure/sqlite`, `src/infrastructure/blockchain` i `src/infrastructure/issuer`. Dzięki temu endpoint nie zawiera SQL ani szczegółów komunikacji z blockchainem albo emitentem.

## Portfele

- gorący — realizuje operacje zewnętrzne;
- zimny — przechowuje większość tokenów klientów;
- korporacyjny — przechowuje własny majątek CASP, np. wypłacone prowizje.

Bootstrap kupuje 10 000 rUSD bezpośrednio na portfel gorący, po czym rebalans przenosi 8 000 rUSD do zimnego. Przydzielenie zapasu klientowi jest atomowym księgowaniem SQLite, a nie transferem blockchainowym.

## Uruchomienie bez Dockera

Uruchom kontrakt, mockBank i backend emitenta, skopiuj `.env.example` do `.env`, a następnie wykonaj `cargo run`. Konfiguracja obejmuje `TOKEN_ADDRESS`, adresy i klucze trzech portfeli, `RPC_URL`, `ISSUER_URL`, `MOCK_BANK_URL` i `CASP_DATABASE_PATH`.

Najważniejsze obszary API to `/api/v1/clients`, portfele i uzgodnienie w `/api/v1/admin`, rebalans, powiększanie zapasu, czarna lista, blokady kont i raporty dzienne.

```powershell
cargo fmt --all --check
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
```
# Planowane OpenAPI

Kontrakty HTTP są obecnie utrzymywane ręcznie. Planowanym rozszerzeniem jest generowanie specyfikacji OpenAPI bezpośrednio z tras, modeli żądań i odpowiedzi oraz typów błędów. Ma to ułatwić integrację frontendu CASP i komunikację CASP–emitent, ale nie jest częścią bieżącego refaktoru struktury kodu.
