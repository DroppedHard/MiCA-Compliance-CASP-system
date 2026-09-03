import { describe, expect, it } from "vitest"
import { isPurchaseBlocked, modeAfterStateChange, recipientWallet } from "./model"

describe("widok klienta CASP", () => {
  it("blokuje wyłącznie zakup podczas nieodwracalnego wygaszania", () => {
    expect(isPurchaseBlocked("purchase", "wind_down")).toBe(true)
    expect(isPurchaseBlocked("sale", "wind_down")).toBe(false)
    expect(isPurchaseBlocked("purchase", "mint_blocked")).toBe(false)
    expect(modeAfterStateChange("purchase", "wind_down")).toBe("sale")
  })

  it("zamienia identyfikator klienta na jego logiczny adres odbiorczy", () => {
    const accounts = [{ clientId: "bob", walletAddress: "rusd:casp:bob", availableRaw: "0", lockedRaw: "0", inventoryAvailableRaw: "0" }]
    expect(recipientWallet(accounts, "bob")).toBe("rusd:casp:bob")
    expect(recipientWallet(accounts, "unknown")).toBe("unknown")
  })
})
