import {describe,expect,it} from "vitest"
import {usdToMinor,usdToRaw} from "./amounts"
describe("amount conversion",()=>{it("uses USD cents and six token decimals",()=>{expect(usdToMinor("12,34")).toBe(1234);expect(usdToRaw("12.34")).toBe(12_340_000)});it("rejects sub-cent values",()=>expect(()=>usdToMinor("1.001")).toThrow())})
