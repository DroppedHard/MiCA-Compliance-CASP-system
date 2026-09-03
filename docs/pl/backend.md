# Backend CASP

Serwis Rust prowadzi niezależny rejestr praw klientów i komunikuje się z emitentem przez HTTP. SQLite przechowuje pozycje klientów, zapas, prowizje i historię operacji; blockchain dostarcza salda portfeli custody.

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

