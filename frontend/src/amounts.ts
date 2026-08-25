export const rawToRusd=(raw:string)=>(Number(raw)/1_000_000).toLocaleString("pl-PL",{minimumFractionDigits:2,maximumFractionDigits:2})
export const usdToMinor=(value:string)=>{const normalized=value.replace(",",".");if(!/^\d+(\.\d{1,2})?$/.test(normalized))throw new Error("Podaj dodatnią kwotę z maksymalnie dwoma miejscami po przecinku");const minor=Math.round(Number(normalized)*100);if(minor<=0)throw new Error("Kwota musi być większa od zera");return minor}
export const usdToRaw=(value:string)=>usdToMinor(value)*10_000
