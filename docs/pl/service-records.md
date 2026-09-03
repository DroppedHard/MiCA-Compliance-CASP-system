# Model rejestru operacji CASP

Księga pozycji jest źródłem sald klientów. Osobny, dopisywany rejestr usług pozwala odtworzyć przyjęcie i realizację dyspozycji, ale sam nie zmienia salda. Zakres pól jest badawczym podzbiorem inspirowanym rozporządzeniem delegowanym Komisji (UE) 2025/1140.

Rekord zawiera identyfikatory rekordu i operacji, klienta i strony, klasyfikację usługi, aktywo i sieć, znaczniki czasu, kwoty brutto/netto/prowizji, dane fiat i metodę ceny, hash transakcji on-chain, wykonawcę, wersję polityki oraz przyczynę odrzucenia.

Korekty są dopisywane do `service_record_amendments` i wskazują rekord źródłowy. Nie nadpisują historii i nie cofają księgowania; finansowe odwrócenie wymaga operacji kompensacyjnej.

- `GET /api/v1/admin/service-records` — eksport rekordów;
- `GET /api/v1/admin/service-record-amendments` — historia korekt;
- `POST /api/v1/admin/service-record-amendments` — nowa adnotacja.

Demo ustawia pięcioletni termin retencji, ale nie realizuje automatycznej archiwizacji ani formalnego eksportu regulatora.

