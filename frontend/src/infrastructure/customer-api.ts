import { Schema } from "effect"
import { AccountSchema, ClientStatementSchema, ExternalWithdrawalSchema, OrderSchema, RecordItemSchema, TransferSchema } from "../domain/api-contracts"
import { clientPath, postJson, request } from "./http-client"

export const customerApi = {
  accounts: () => request("/api/v1/clients", undefined, Schema.Array(AccountSchema)),
  account: (id: string) => request(`${clientPath(id)}/account`, undefined, AccountSchema),
  records: (id: string) => request(`${clientPath(id)}/records`, undefined, Schema.Array(RecordItemSchema)),
  purchase: (id: string, operationId: string, amountUsdMinor: number) => request(`${clientPath(id)}/purchases`, postJson({ operationId, amountUsdMinor }), OrderSchema),
  sale: (id: string, operationId: string, tokenAmountRaw: number) => request(`${clientPath(id)}/sales`, postJson({ operationId, tokenAmountRaw }), OrderSchema),
  transfer: (id: string, operationId: string, recipientClientId: string, tokenAmountRaw: number, purposeClassification: string) => request(`${clientPath(id)}/transfers`, postJson({ operationId, recipientClientId, tokenAmountRaw, purposeClassification }), TransferSchema),
  externalWithdrawal: (id: string, operationId: string, destinationAddress: string, tokenAmountRaw: number) => request(`${clientPath(id)}/external-withdrawals`, postJson({ operationId, destinationAddress, tokenAmountRaw }), ExternalWithdrawalSchema),
  statement: (id: string, from: string, to: string) => request(`${clientPath(id)}/statement?from=${encodeURIComponent(from)}&to=${encodeURIComponent(to)}`, undefined, ClientStatementSchema),
}
