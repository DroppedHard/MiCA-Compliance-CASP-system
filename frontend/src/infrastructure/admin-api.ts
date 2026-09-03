import { Schema } from "effect"
import { AccountRestrictionSchema, BlacklistEntrySchema, BootstrapSchema, DailyReportSchema, ExchangeRateSchema, FeePositionSchema, FeeSweepSchema, InventoryOperationSchema, RebalancingPlanSchema, RebalancingResultSchema, ReconciliationSchema, WalletsSchema } from "../domain/api-contracts"
import { postJson, request } from "./http-client"

export const adminApi = {
  bootstrapStatus: () => request("/api/v1/admin/bootstrap-inventory", undefined, BootstrapSchema),
  wallets: () => request("/api/v1/admin/wallets", undefined, WalletsSchema),
  reconciliation: () => request("/api/v1/admin/reconciliation", undefined, ReconciliationSchema),
  fees: () => request("/api/v1/admin/fees", undefined, FeePositionSchema),
  setExchangeRate: (usdMinorPerRusd: number) => request("/api/v1/admin/exchange-rate", postJson({ usdMinorPerRusd }), ExchangeRateSchema),
  blacklist: () => request("/api/v1/admin/address-blacklist", undefined, Schema.Array(BlacklistEntrySchema)),
  addToBlacklist: (address: string, reason: string) => request("/api/v1/admin/address-blacklist", postJson({ address, reason }), BlacklistEntrySchema),
  removeFromBlacklist: (address: string) => request(`/api/v1/admin/address-blacklist/${encodeURIComponent(address)}`, { method: "DELETE" }, Schema.Unknown),
  sweepFees: (operationId: string) => request("/api/v1/admin/fee-sweeps", postJson({ operationId }), FeeSweepSchema),
  dailyReport: (from: string, to: string) => request(`/api/v1/reports/daily-transactions?from=${encodeURIComponent(from)}&to=${encodeURIComponent(to)}`, undefined, DailyReportSchema),
  replenishments: () => request("/api/v1/admin/inventory-replenishments", undefined, Schema.Array(InventoryOperationSchema)),
  replenish: (operationId: string, amountUsdMinor: number) => request("/api/v1/admin/inventory-replenishments", postJson({ operationId, amountUsdMinor }), InventoryOperationSchema),
  rebalancingPlan: () => request("/api/v1/admin/rebalancing-plan", undefined, RebalancingPlanSchema),
  rebalance: () => request("/api/v1/admin/rebalancing", { method: "POST" }, RebalancingResultSchema),
  accountRestrictions: () => request("/api/v1/admin/client-account-restrictions", undefined, Schema.Array(AccountRestrictionSchema)),
  blockAccount: (clientId: string, reason: string) => request("/api/v1/admin/client-account-restrictions", postJson({ clientId, reason }), AccountRestrictionSchema),
  unblockAccount: (clientId: string) => request(`/api/v1/admin/client-account-restrictions/${encodeURIComponent(clientId)}`, { method: "DELETE" }, Schema.Unknown),
}
