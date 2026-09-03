# Frontend CASP

Aplikacja React zawiera portal klienta i panel administratora. Konta demonstracyjne można przełączać bez logowania, a obsługa MetaMask nie jest wymagana.

```powershell
npm.cmd install
npm.cmd run dev
npm.cmd test
npm.cmd run build
```

Portal działa pod `http://127.0.0.1:5174/`, panel pod `/admin`, a zestawienie klienta pod `/statement?client=alice`. Klient może kupować, sprzedawać, wykonywać przelewy wewnętrzne i zlecać wypłatę Ethereum. Administrator kontroluje zapas, portfele, uzgodnienie, prowizje, blokady, raporty i ręczny rebalans.

