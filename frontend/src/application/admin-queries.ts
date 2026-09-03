import { queryOptions } from "@tanstack/react-query"
import { adminApi } from "../infrastructure/admin-api"
import { customerApi } from "../infrastructure/customer-api"

export const adminQueries = {
  reconciliation: () => queryOptions({ queryKey: ["admin", "reconciliation"], queryFn: adminApi.reconciliation, refetchInterval: 10_000 }),
  wallets: () => queryOptions({ queryKey: ["admin", "wallets"], queryFn: adminApi.wallets, refetchInterval: 10_000 }),
  accounts: () => queryOptions({ queryKey: ["admin", "accounts"], queryFn: customerApi.accounts, refetchInterval: 10_000 }),
  records: () => queryOptions({ queryKey: ["admin", "records"], queryFn: async () => {
    const clients = await customerApi.accounts()
    const records = await Promise.all(clients.map(account => customerApi.records(account.clientId)))
    return [...new Map(records.flat().map(record => [record.recordId, record])).values()].sort((left, right) => right.createdAtUnixMs - left.createdAtUnixMs)
  }, refetchInterval: 10_000 }),
  fees: () => queryOptions({ queryKey: ["admin", "fees"], queryFn: adminApi.fees, refetchInterval: 10_000 }),
  blacklist: () => queryOptions({ queryKey: ["admin", "blacklist"], queryFn: adminApi.blacklist }),
  accountRestrictions: () => queryOptions({ queryKey: ["admin", "account-restrictions"], queryFn: adminApi.accountRestrictions }),
  dailyReport: (from: string, to: string) => queryOptions({ queryKey: ["admin", "daily-report", from, to], queryFn: () => adminApi.dailyReport(from, to), refetchInterval: 10_000 }),
  bootstrap: () => queryOptions({ queryKey: ["admin", "bootstrap"], queryFn: adminApi.bootstrapStatus, refetchInterval: 10_000 }),
  replenishments: () => queryOptions({ queryKey: ["admin", "replenishments"], queryFn: adminApi.replenishments, refetchInterval: 10_000 }),
  rebalancingPlan: () => queryOptions({ queryKey: ["admin", "rebalancing-plan"], queryFn: adminApi.rebalancingPlan, refetchInterval: 10_000 }),
}
