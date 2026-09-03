import { queryOptions } from "@tanstack/react-query"
import { customerApi } from "../infrastructure/customer-api"
import { publicApi } from "../infrastructure/public-api"

export const customerQueries = {
  accounts: () => queryOptions({ queryKey: ["accounts"], queryFn: customerApi.accounts, refetchInterval: 3_000 }),
  account: (clientId: string) => queryOptions({ queryKey: ["account", clientId], queryFn: () => customerApi.account(clientId), refetchInterval: 3_000 }),
  records: (clientId: string) => queryOptions({ queryKey: ["records", clientId], queryFn: () => customerApi.records(clientId), refetchInterval: 3_000 }),
  tokenInformation: () => queryOptions({ queryKey: ["token-information"], queryFn: publicApi.tokenInformation, refetchInterval: 10_000 }),
  exchangeRate: () => queryOptions({ queryKey: ["exchange-rate"], queryFn: publicApi.exchangeRate, refetchInterval: 10_000 }),
}
