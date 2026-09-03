import type { Account, RecordItem } from "../../domain/api-contracts"

export type CustomerMode = "purchase" | "sale" | "transfer" | "withdrawal"

export const isPurchaseBlocked = (mode: CustomerMode, assetState?: string) => mode === "purchase" && assetState === "wind_down"
export const modeAfterStateChange = (mode: CustomerMode, assetState?: string): CustomerMode => isPurchaseBlocked(mode, assetState) ? "sale" : mode
export const recipientWallet = (accounts: ReadonlyArray<Account> | undefined, recipientId: string) => accounts?.find(account => account.clientId === recipientId)?.walletAddress ?? recipientId
export const assetStateLabel = (value: string) => ({ active: "Aktywny", warning: "Ostrzeżenie", mint_blocked: "Emisja zablokowana", data_unavailable: "Brak danych", wind_down: "Wygaszanie" })[value] ?? value
export const assetStateDescription = (value: string) => value === "wind_down" ? "Nie można kupować nowych rUSD. Sprzedaż i wyjście klienta pozostają dostępne." : value === "mint_blocked" ? "Emitent nie może tworzyć nowych tokenów; obrót istniejącą pulą CASP pozostaje dostępny." : value === "warning" ? "Token działa, ale emitent wskazuje podwyższone ryzyko." : value === "active" ? "Token działa bez aktywnej blokady cyklu życia." : "Nie udało się potwierdzić bieżącego stanu tokenu."

export const customerRecordPresentation = (record: RecordItem, clientId: string) => {
  const transfer = record.orderType === "internal_transfer"
  const sent = transfer && record.sourceAccount === clientId
  const received = transfer && record.destinationAccount === clientId
  const withdrawal = record.orderType === "external_withdrawal"
  return {
    title: sent ? "Wysłano przelew" : received ? "Otrzymano przelew" : record.orderType === "external_deposit" ? "Wpłata zewnętrzna" : withdrawal ? "Wypłata na portfel" : record.orderType === "purchase" ? "Zakup" : record.orderType === "sale" ? "Sprzedaż" : "Wykup",
    amountRaw: received || withdrawal ? record.netQuantityRaw : record.grossQuantityRaw,
    showFee: (sent || withdrawal) && Number(record.feeQuantityRaw) > 0,
  }
}
