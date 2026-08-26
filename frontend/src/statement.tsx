import { useQuery } from "@tanstack/react-query"
import { useState } from "react"
import { api } from "./api"
import { rawToRusd } from "./amounts"
import "./statement.css"

const clients:Record<string,string>={alice:"Alicja Kowalska",bob:"Bartosz Nowak",carol:"Karolina Wiśniewska"}
export function StatementPage(){
  const parameters=new URLSearchParams(window.location.search)
  const today=new Date().toISOString().slice(0,10)
  const monthStart=`${today.slice(0,8)}01`
  const [client,setClient]=useState(parameters.get("client")??"alice")
  const [from,setFrom]=useState(parameters.get("from")??monthStart)
  const [to,setTo]=useState(parameters.get("to")??today)
  const statement=useQuery({queryKey:["statement",client,from,to],queryFn:()=>api.statement(client,from,to),enabled:Boolean(from&&to&&from<=to)})
  const data=statement.data
  return <main className="statement-page"><header><div><p className="eyebrow">rUSD CASP · WYCIĄG DEMONSTRACYJNY</p><h1>Wyciąg klienta</h1><span>{data?.statementVersion??"casp-client-statement-v1"}</span></div><div className="statement-actions"><a href="/">Powrót do konta</a><button onClick={()=>window.print()}>Zapisz / drukuj PDF</button></div></header><section className="statement-controls"><label>Klient<select value={client} onChange={event=>setClient(event.target.value)}>{Object.entries(clients).map(([id,name])=><option key={id} value={id}>{name}</option>)}</select></label><label>Od<input type="date" value={from} onChange={event=>setFrom(event.target.value)}/></label><label>Do<input type="date" value={to} onChange={event=>setTo(event.target.value)}/></label></section>{statement.error&&<p className="error">{statement.error.message}</p>}{data&&<><section className="statement-identity"><div><span>Klient</span><strong>{clients[data.clientId]??data.clientId}</strong></div><div><span>Okres UTC</span><strong>{data.fromDateUtc} — {data.toDateUtc}</strong></div><div><span>Aktywo</span><strong>{data.assetSymbol}</strong></div><div><span>Wygenerowano</span><strong>{new Date(data.generatedAtUnixMs).toLocaleString("pl-PL")}</strong></div></section><section className="statement-balances"><Balance label="Saldo otwarcia" available={data.openingAvailableRaw} locked={data.openingLockedRaw}/><Balance label="Saldo zamknięcia" available={data.closingAvailableRaw} locked={data.closingLockedRaw}/></section><section><h2>Podsumowanie okresu</h2><div className="statement-summary"><Metric label="Zakupy" value={data.totalPurchasesRaw}/><Metric label="Sprzedaże" value={data.totalSalesRaw}/><Metric label="Przelewy wysłane" value={data.totalTransfersSentRaw}/><Metric label="Przelewy otrzymane" value={data.totalTransfersReceivedRaw}/><Metric label="Opłaty" value={data.totalFeesRaw}/><Metric label="Wykupy u emitenta" value={data.totalRedemptionsRaw}/></div></section><section><h2>Operacje</h2><div className="statement-table"><div className="statement-row heading"><span>Data</span><span>Typ / kontrahent</span><span>Dostępne Δ</span><span>Zablokowane Δ</span><span>Opłata</span></div>{data.movements.map(movement=><div className="statement-row" key={movement.operationId}><span>{new Date(movement.occurredAtUnixMs).toLocaleString("pl-PL")}</span><span><b>{label(movement.operationType)}</b><small>{movement.counterparty??"—"} · {movement.operationId}</small></span><span>{signed(movement.availableDeltaRaw)}</span><span>{signed(movement.lockedDeltaRaw)}</span><span>{rawToRusd(movement.feeRaw)} rUSD</span></div>)}{data.movements.length===0&&<p>Brak operacji w wybranym okresie.</p>}</div></section><footer>{data.disclaimer}</footer></>}</main>
}
function Balance({label,available,locked}:{label:string;available:string;locked:string}){return <article><span>{label}</span><strong>{rawToRusd(available)} rUSD</strong><small>Zablokowane: {rawToRusd(locked)} rUSD</small></article>}
function Metric({label,value}:{label:string;value:string}){return <div><span>{label}</span><b>{rawToRusd(value)} rUSD</b></div>}
function signed(value:string){const amount=Number(value);return `${amount>0?"+":""}${rawToRusd(value)} rUSD`}
function label(value:string){return {purchase:"Zakup",sale:"Sprzedaż",redemption:"Wykup",internal_transfer_sent:"Przelew wysłany",internal_transfer_received:"Przelew otrzymany"}[value]??value}
