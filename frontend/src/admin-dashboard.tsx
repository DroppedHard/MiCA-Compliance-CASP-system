import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { useState } from "react"
import { AlertTriangle, Building2, CheckCircle2, CircleDollarSign, Database, Server, ShieldCheck, Users, WalletCards } from "lucide-react"
import { api, type RecordItem } from "./api"
import { rawToRusd } from "./amounts"

const names:Record<string,string>={alice:"Alicja",bob:"Bartosz",carol:"Karolina"}

export function AdminDashboard(){
  const queryClient=useQueryClient()
  const [inventoryAmount,setInventoryAmount]=useState("1000.00")
  const reportRange=lastSevenDays()
  const reconciliation=useQuery({queryKey:["admin","reconciliation"],queryFn:api.reconciliation,refetchInterval:10_000})
  const wallets=useQuery({queryKey:["admin","wallets"],queryFn:api.wallets,refetchInterval:10_000})
  const accounts=useQuery({queryKey:["admin","accounts"],queryFn:api.accounts,refetchInterval:10_000})
  const records=useQuery({queryKey:["admin","records"],queryFn:async()=>{
    const clients=await api.accounts()
    const clientRecords=await Promise.all(clients.map(account=>api.records(account.clientId)))
    return [...new Map(clientRecords.flat().map(record=>[record.recordId,record])).values()].sort((left,right)=>right.createdAtUnixMs-left.createdAtUnixMs)
  },refetchInterval:10_000})
  const fees=useQuery({queryKey:["admin","fees"],queryFn:api.fees,refetchInterval:10_000})
  const dailyReport=useQuery({queryKey:["admin","daily-report",reportRange.from,reportRange.to],queryFn:()=>api.dailyReport(reportRange.from,reportRange.to),refetchInterval:10_000})
  const bootstrap=useQuery({queryKey:["admin","bootstrap"],queryFn:api.bootstrapStatus,refetchInterval:10_000})
  const replenishments=useQuery({queryKey:["admin","replenishments"],queryFn:api.replenishments,refetchInterval:10_000})
  const plan=useQuery({queryKey:["admin","rebalancing-plan"],queryFn:api.rebalancingPlan,refetchInterval:10_000})
  const replenish=useMutation({mutationFn:()=>api.replenish(crypto.randomUUID(),Math.round(Number(inventoryAmount)*100)),onSuccess:async()=>{await Promise.all([queryClient.invalidateQueries({queryKey:["admin","replenishments"]}),queryClient.invalidateQueries({queryKey:["admin","wallets"]}),queryClient.invalidateQueries({queryKey:["admin","reconciliation"]}),queryClient.invalidateQueries({queryKey:["admin","accounts"]})])}})
  const sweepFees=useMutation({mutationFn:()=>api.sweepFees(crypto.randomUUID()),onSuccess:async()=>{await Promise.all([queryClient.invalidateQueries({queryKey:["admin","fees"]}),queryClient.invalidateQueries({queryKey:["admin","wallets"]}),queryClient.invalidateQueries({queryKey:["admin","reconciliation"]})])}})
  const state=reconciliation.data?.status
  const error=reconciliation.error??wallets.error??accounts.error??records.error??fees.error??dailyReport.error??bootstrap.error
  const failedRecords=records.data?.filter(record=>isFailure(record.status))??[]

  return <main className="casp-admin-dashboard">
    <header><div className="brand"><Building2/> rUSD CASP · administrator</div><a className="route-link" href="/">Widok klienta</a><span className="badge"><ShieldCheck/> kontrolki demo bez logowania</span></header>
    <section className="card rate-context"><div><p className="eyebrow">MECHANIZMY CENOWE</p><h2>Parytet emitenta ≠ kurs CASP</h2></div><p className="hint">Emitent: 1 rUSD = 1 USD. CASP ustala własny kurs detaliczny; w demo także wynosi on 1:1.</p></section>
    {error&&<section className="error-banner"><AlertTriangle/> {error.message}</section>}
    <div className="admin-workspace">
      <div className="admin-column admin-custody-column">
        <section className="admin-hero card"><div><p className="eyebrow">UZGODNIENIE SALD</p><h1>{state?statusLabel(state):"Oczekiwanie na dane"}</h1><p className="hint">{state?statusExplanation(state):"Backend zbiera dowód z blockchaina i rejestru CASP."}</p></div><div className={`state-orb ${state??"unavailable"}`}>{state==="balanced"?<CheckCircle2/>:<AlertTriangle/>}</div></section>
        <section className="admin-metrics">
          <AdminMetric icon={WalletCards} label="Portfel gorący" value={formatRaw(wallets.data?.hotRaw)} detail="Cel demo: 20%"/>
          <AdminMetric icon={Database} label="Portfel zimny" value={formatRaw(wallets.data?.coldRaw)} detail="Cel demo: 80%"/>
          <AdminMetric icon={Building2} label="Korporacyjny" value={formatRaw(wallets.data?.corporateRaw)} detail="Poza pokryciem"/>
          <AdminMetric icon={CircleDollarSign} label="Różnica" value={formatSigned(reconciliation.data?.differenceRaw)} detail={`Blok: ${reconciliation.data?.evidenceBlock??"—"}`}/>
        </section>
        <section className="card"><h2>Zobowiązania i zapas</h2><div className="breakdown"><Row label="Pozycje klientów" value={formatRaw(reconciliation.data?.customerAvailableRaw)}/><Row label="Pozycje zablokowane" value={formatRaw(reconciliation.data?.customerLockedRaw)}/><Row label="Nieprzypisany zapas" value={formatRaw(reconciliation.data?.inventoryAvailableRaw)}/><Row label="Prowizje oczekujące" value={formatRaw(fees.data?.pendingRaw)}/><Row label="Łączne zobowiązanie" value={formatRaw(reconciliation.data?.obligationTotalRaw)} strong/><Row label="Portfel gorący + zimny" value={formatRaw(reconciliation.data?.custodyTotalRaw)} strong/></div><p className="policy-note">{reconciliation.data?.policyVersion??"casp-custody-reconciliation-v1"} · {reconciliation.data?new Date(reconciliation.data.checkedAtUnixMs).toLocaleString("pl-PL"):"—"}</p></section>
        <section className="card"><h2>Mapa klientów i custody</h2><div className="custody-graph"><div className="graph-wallet">GORĄCY + ZIMNY<strong>{formatRaw(reconciliation.data?.custodyTotalRaw)}</strong></div><div className="graph-line"/><div className="client-nodes">{accounts.data?.map(account=><div className="client-node" key={account.clientId}><Users/><span>{names[account.clientId]??account.clientId}</span><strong>{rawToRusd(account.availableRaw)} rUSD</strong><small>Zablokowane: {rawToRusd(account.lockedRaw)}</small></div>)}</div></div></section>
      </div>

      <div className="admin-column admin-inventory-column">
        <section className="card"><h2>Ręczne zwiększenie zapasu</h2><p className="hint">Zakup puli u emitenta z demonstracyjnym podziałem 20% / 80%. Automatyczne uzupełnianie jest wyłączone.</p><form className="inventory-form" onSubmit={event=>{event.preventDefault();if(Number(inventoryAmount)>0)replenish.mutate()}}><label htmlFor="inventory-amount">Kwota zakupu w USD</label><input id="inventory-amount" value={inventoryAmount} onChange={event=>setInventoryAmount(event.target.value)} inputMode="decimal"/><button className="primary" disabled={replenish.isPending||Number(inventoryAmount)<=0}>{replenish.isPending?"Realizacja…":"Kup pulę od emitenta"}</button></form>{replenish.error&&<p className="error">{replenish.error.message}</p>}<div className="breakdown"><Row label="Docelowy portfel gorący" value={formatRaw(plan.data?.targetHotRaw)}/><Row label="Docelowy portfel zimny" value={formatRaw(plan.data?.targetColdRaw)}/><Row label="Korekta gorącego" value={formatDelta(plan.data?.hotDeltaRaw)}/><Row label="Korekta zimnego" value={formatDelta(plan.data?.coldDeltaRaw)}/></div><p className="policy-note">{plan.data?.policyVersion??"casp-manual-inventory-20-80-v1"}</p><div className="table">{replenishments.data?.slice(0,5).map(operation=><article key={operation.operationId}><div><strong>{formatMinor(operation.amountUsdMinor)} USD</strong><small>{operation.operationId}</small></div><span className={`status ${operation.status==="completed"?"completed":""}`}>{operationStatusLabel(operation.status)}</span></article>)}</div></section>
        <section className="card fee-sweep"><div><p className="eyebrow">PROWIZJE CASP</p><h2>{formatRaw(fees.data?.pendingRaw)} do przeniesienia</h2><p className="hint">Transfer z portfela gorącego do korporacyjnego.</p></div><button className="primary" disabled={sweepFees.isPending||!fees.data||Number(fees.data.pendingRaw)===0} onClick={()=>sweepFees.mutate()}>{sweepFees.isPending?"Przenoszenie…":"Przenieś prowizje"}</button>{sweepFees.error&&<p className="error">{sweepFees.error.message}</p>}{sweepFees.data&&<p className="success">Przeniesiono {formatRaw(sweepFees.data.amountRaw)} · {shortHash(sweepFees.data.transactionHash)}</p>}</section>
        <section className="card"><h2>Integracja z emitentem</h2><div className="integration-status"><Server/><div><strong>{bootstrap.data?.status??"brak danych"}</strong><p className="hint">Zakup początkowy: {bootstrap.data?.operationId??"—"}</p></div></div>{bootstrap.data?.lastError?<p className="error">{bootstrap.data.lastError}</p>:failedRecords.length?<p className="error">Nieudane operacje: {failedRecords.length}</p>:<p className="success">Brak zgłoszonych błędów procesów.</p>}<p className="hint">Hash emisji: {shortHash(bootstrap.data?.issuerTransactionHash)}</p></section>
      </div>

      <div className="admin-column admin-audit-column">
        <section className="card"><h2>Ostatnie operacje</h2><p className="hint">Rejestr audytowy usług CASP — nie jest źródłem sald.</p><div className="operation-flow">{records.data?.slice(0,6).map(record=><Operation key={record.recordId} record={record}/>)}</div>{records.data?.length===0&&<p className="empty-state">Brak zarejestrowanych operacji klientów.</p>}</section>
        <section className="card"><h2>Raportowanie dzienne</h2><p className="hint">Agregat udostępniany emitentowi; operacje fiat są wykluczone z estymaty środka wymiany.</p><div className="daily-report">{dailyReport.data?.days.map(day=><article key={day.dateUtc}><strong>{day.dateUtc}</strong><span>Wszystkie operacje: {day.totalOperationCount}</span><span>Środek wymiany: {day.meansOfExchangeCount}</span><span>Wartość: {formatMinor(day.meansOfExchangeValueUsdMinor)} USD</span><small>{day.methodologyVersion} · USD/EUR 1:1</small></article>)}</div>{dailyReport.data?.days.length===0&&<p className="empty-state">Brak aktywności w ostatnich siedmiu dniach.</p>}</section>
      </div>
    </div>
  </main>
}

function AdminMetric({icon:Icon,label,value,detail}:{icon:typeof ShieldCheck;label:string;value:string;detail:string}){return <article className="card admin-metric"><Icon/><span>{label}</span><strong>{value}</strong><small>{detail}</small></article>}
function Row({label,value,strong=false}:{label:string;value:string;strong?:boolean}){return <div className={strong?"summary-row strong":"summary-row"}><span>{label}</span><b>{value}</b></div>}
function Operation({record}:{record:RecordItem}){return <article className={`operation ${isFailure(record.status)?"failed":""}`}><div className="operation-route"><span>{names[record.sourceAccount??""]??record.sourceAccount??"CASP"}</span><i>→</i><span>{names[record.destinationAccount??""]??record.destinationAccount??"CASP"}</span></div><strong>{rawToRusd(record.netQuantityRaw)} rUSD</strong><small>{operationLabel(record.orderType)} · {new Date(record.receivedAtUnixMs).toLocaleString("pl-PL")}</small><small>{record.priceMethod} · {record.instructionChannel} · {record.policyVersion}</small><span className="operation-status">{operationStatusLabel(record.status)}</span></article>}
function formatRaw(value:string|undefined|null){return value===undefined||value===null?"—":`${rawToRusd(value)} rUSD`}
function formatSigned(value:string|undefined|null){if(value===undefined||value===null)return "—";const number=Number(value)/1_000_000;return `${number>0?"+":""}${number.toLocaleString("pl-PL",{maximumFractionDigits:6})} rUSD`}
function shortHash(value:string|undefined|null){return value?`${value.slice(0,10)}…${value.slice(-6)}`:"—"}
function statusLabel(value:"balanced"|"warning"|"blocked"|"unavailable"){return {balanced:"Zgodność potwierdzona",warning:"Pokrycie z ostrzeżeniem",blocked:"Operacje zakupowe zablokowane",unavailable:"Brak wiarygodnych danych"}[value]}
function statusExplanation(value:"balanced"|"warning"|"blocked"|"unavailable"){return {balanced:"Stan portfeli jest zgodny z pozycjami klientów i rejestrem CASP.",warning:"Pokrycie pozostaje dodatnie, ale zbliża się do przyjętego progu ostrzegawczego.",blocked:"Stan portfeli nie odpowiada zobowiązaniom klientów, zapasowi lub oczekującym prowizjom.",unavailable:"Brakuje aktualnego dowodu pozwalającego wiarygodnie uzgodnić salda."}[value]}
function operationLabel(value:string){return {purchase:"zakup",sale:"sprzedaż",internal_transfer:"przelew wewnętrzny",external_deposit:"wpłata zewnętrzna",redemption:"wykup u emitenta"}[value]??value}
function operationStatusLabel(value:string){return {completed:"zakończona",pending:"oczekująca",processing:"przetwarzana",failed:"nieudana",rejected:"odrzucona",cancelled:"anulowana"}[value.toLowerCase()]??value}
function isFailure(value:string){return ["failed","rejected","cancelled"].includes(value.toLowerCase())}
function formatMinor(value:string){return (Number(value)/100).toLocaleString("pl-PL",{minimumFractionDigits:2,maximumFractionDigits:2})}
function formatDelta(value:number|undefined){if(value===undefined)return "—";return `${value>0?"+":""}${rawToRusd(String(value))} rUSD`}
function lastSevenDays(){const to=new Date();const from=new Date(to);from.setUTCDate(from.getUTCDate()-6);return {from:from.toISOString().slice(0,10),to:to.toISOString().slice(0,10)}}
