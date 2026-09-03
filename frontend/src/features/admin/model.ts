export type ReconciliationStatus = "balanced" | "warning" | "mismatch" | "unavailable"

export const reconciliationLabel = (value: ReconciliationStatus) => ({ balanced: "Salda uzgodnione", warning: "Odchylenie podziału 20/80", mismatch: "Niezgodność sald", unavailable: "Brak danych do uzgodnienia" })[value]
export const reconciliationDescription = (value: ReconciliationStatus) => ({ balanced: "Portfele odpowiadają zapisom rejestru i docelowemu podziałowi.", warning: "Łączna ilość tokenów jest uzgodniona, ale proporcja portfela gorącego i zimnego odbiega od polityki demonstracyjnej. Operacje nie są blokowane.", mismatch: "Portfele nie odpowiadają rejestrowi. To alarm diagnostyczny, a nie automatyczna blokada operacji.", unavailable: "Nie udało się pobrać danych kontrolnych. Operacje nie są z tego powodu automatycznie blokowane." })[value]
export const isFailedOperation = (value: string) => ["failed", "rejected", "cancelled"].includes(value.toLowerCase())
export const sevenDayRange = (today: Date) => { const to = new Date(today); const from = new Date(to); from.setUTCDate(from.getUTCDate() - 6); return { from: from.toISOString().slice(0, 10), to: to.toISOString().slice(0, 10) } }
