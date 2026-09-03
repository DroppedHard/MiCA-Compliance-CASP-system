import { ExchangeRateSchema, TokenInformationSchema } from "../domain/api-contracts"
import { request } from "./http-client"

export const publicApi = {
  exchangeRate: () => request("/api/v1/public/exchange-rate", undefined, ExchangeRateSchema),
  tokenInformation: () => request("/api/v1/public/token-information", undefined, TokenInformationSchema),
}
