import { adminApi } from "./infrastructure/admin-api"
import { customerApi } from "./infrastructure/customer-api"
import { publicApi } from "./infrastructure/public-api"

export type { Account, RecordItem } from "./domain/api-contracts"

export const api = { ...customerApi, ...adminApi, ...publicApi }
export const accountRestrictionApi = {
  list: adminApi.accountRestrictions,
  block: adminApi.blockAccount,
  unblock: adminApi.unblockAccount,
}
