import { describe, expect, it } from "vitest"
import { isFailedOperation, reconciliationDescription, reconciliationLabel, sevenDayRange } from "./model"

describe("panel administratora CASP", () => {
  it("rozróżnia rozbieżność custody od blokady operacyjnej", () => {
    expect(reconciliationLabel("mismatch")).toBe("Niezgodność sald")
    expect(reconciliationDescription("mismatch")).toContain("nie automatyczna blokada")
  })

  it("klasyfikuje końcowe błędy i wyznacza pełne siedem dni UTC", () => {
    expect(isFailedOperation("FAILED")).toBe(true)
    expect(isFailedOperation("completed")).toBe(false)
    expect(sevenDayRange(new Date("2026-09-03T12:00:00Z"))).toEqual({ from: "2026-08-28", to: "2026-09-03" })
  })
})
