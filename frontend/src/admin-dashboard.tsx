import { useQuery } from "@tanstack/react-query"
import { AlertTriangle, Building2, CheckCircle2, CircleDollarSign, Database, Server, ShieldCheck, Users, WalletCards } from "lucide-react"
import { api, type RecordItem } from "./api"
import { rawToRusd } from "./amounts"

const names:Record<string,string>={alice:"Alicja",bob:"Bartosz",carol:"Karolina"}

export function AdminDashboard(){
  const reconciliation=useQuery({queryKey:["admin","reconciliation"],queryFn:api.reconciliation,refetchInterval:10_000})
  const wallets=useQuery({queryKey:["admin","wallets"],queryFn:api.wallets,refetchInterval:10_000})
  const accounts=useQuery({queryKey:["admin","accounts"],queryFn:api.accounts,refetchInterval:10_000})
  const records=useQuery({queryKey:["admin","records"],queryFn:async()=>{
    const clients=await api.accounts()
    const clientRecords=await Promise.all(clients.map(account=>api.records(account.clientId)))
    return [...new Map(clientRecords.flat().map(record=>[record.recordId,record])).values()].sort((left,right)=>right.createdAtUnixMs-left.createdAtUnixMs)
  },refetchInterval:10_000})
  const fees=useQuery({queryKey:["admin","fees"],queryFn:api.fees,refetchInterval:10_000})
  const bootstrap=useQuery({queryKey:["admin","bootstrap"],queryFn:api.bootstrapStatus,refetchInterval:10_000})
  const state=reconciliation.data?.status
  const error=reconciliation.error??wallets.error??accounts.error??records.error??fees.error??bootstrap.error
  const failedRecords=records.data?.filter(record=>isFailure(record.status))??[]

  return <main>
    <header><div className="brand"><Building2/> rUSD CASP · administrator</div><a className="route-link" href="/">Widok klienta</a><span className="badge"><ShieldCheck/> kontrolki demo bez logowania</span></header>
    <section className="admin-hero card"><div><p className="eyebrow">REKONSYLIACJA CUSTODY</p><h1>{state?statusLabel(state):"Oczekiwanie na dane"}</h1><p className="hint">{reconciliation.data?.reason??"Backend zbiera dowód z blockchaina i ledgeru CASP."}</p></div><div className={`state-orb ${state??"unavailable"}`}>{state==="balanced"?<CheckCircle2/>:<AlertTriangle/>}</div></section>
    {error&&<section className="error-banner"><AlertTriangle/> {error.message}</section>}
    <section className="admin-metrics">
      <AdminMetric icon={WalletCards} label="Hot wallet" value={formatRaw(wallets.data?.hotRaw)} detail="Cel demonstracyjny: 20%"/>
      <AdminMetric icon={Database} label="Cold wallet" value={formatRaw(wallets.data?.coldRaw)} detail="Cel demonstracyjny: 80%"/>
      <AdminMetric icon={Building2} label="Corporate wallet" value={formatRaw(wallets.data?.corporateRaw)} detail="Poza pokryciem klientów"/>
      <AdminMetric icon={CircleDollarSign} label="Różnica custody" value={formatSigned(reconciliation.data?.differenceRaw)} detail={`Blok dowodowy: ${reconciliation.data?.evidenceBlock??"—"}`}/>
    </section>
    <div className="admin-grid">
      <section className="card"><h2>Zobowiązania i inventory</h2><div className="breakdown"><Row label="Dostępne pozycje klientów" value={formatRaw(reconciliation.data?.customerAvailableRaw)}/><Row label="Pozycje zablokowane" value={formatRaw(reconciliation.data?.customerLockedRaw)}/><Row label="Nieprzypisany zapas" value={formatRaw(reconciliation.data?.inventoryAvailableRaw)}/><Row label="Prowizje oczekujące na sweep" value={formatRaw(fees.data?.pendingRaw)}/><Row label="Łączne zobowiązanie" value={formatRaw(reconciliation.data?.obligationTotalRaw)} strong/><Row label="Hot + cold" value={formatRaw(reconciliation.data?.custodyTotalRaw)} strong/></div><p className="policy-note">{reconciliation.data?.policyVersion??"casp-custody-reconciliation-v1"} · ostatnia kontrola {reconciliation.data?new Date(reconciliation.data.checkedAtUnixMs).toLocaleString("pl-PL"):"—"}</p></section>
      <section className="card"><h2>Integracja z emitentem</h2><div className="integration-status"><Server/><div><strong>{bootstrap.data?.status??"brak danych"}</strong><p className="hint">Operacja początkowego zakupu: {bootstrap.data?.operationId??"—"}</p></div></div>{bootstrap.data?.lastError?<p className="error">{bootstrap.data.lastError}</p>:failedRecords.length?<p className="error">Wykryto nieudane operacje: {failedRecords.length}</p>:<p className="success">Brak zgłoszonych błędów procesów.</p>}<p className="hint">Hash emisji: {shortHash(bootstrap.data?.issuerTransactionHash)}</p></section>
    </div>
    <section className="card history"><h2>Mapa klientów i custody</h2><p className="hint">Wizualizacja pochodzi z pozycji ledgeru. Źródłem decyzji o pokryciu pozostaje backendowa rekonsyliacja.</p><div className="custody-graph"><div className="graph-wallet">HOT + COLD<strong>{formatRaw(reconciliation.data?.custodyTotalRaw)}</strong></div><div className="graph-line"/><div className="client-nodes">{accounts.data?.map(account=><div className="client-node" key={account.clientId}><Users/><span>{names[account.clientId]??account.clientId}</span><strong>{rawToRusd(account.availableRaw)} rUSD</strong><small>Zablokowane: {rawToRusd(account.lockedRaw)}</small></div>)}</div></div></section>
    <section className="card history"><h2>Ostatnie operacje</h2><p className="hint">Przepływ jest budowany z rejestru usług CASP i służy do audytu. Nie jest źródłem sald ani decyzji o pokryciu.</p><div className="operation-flow">{records.data?.slice(0,6).map(record=><Operation key={record.recordId} record={record}/>)}</div>{records.data?.length===0&&<p className="empty-state">Brak zarejestrowanych operacji klientów.</p>}</section>
    <section className="card history"><h2>Raportowanie dzienne</h2><p className="hint">Agregaty przekazywane emitentowi zostaną podłączone w punkcie 8 roadmapy. Panel nie generuje danych zastępczych.</p></section>
  </main>
}

function AdminMetric({icon:Icon,label,value,detail}:{icon:typeof ShieldCheck;label:string;value:string;detail:string}){return <article className="card admin-metric"><Icon/><span>{label}</span><strong>{value}</strong><small>{detail}</small></article>}
function Row({label,value,strong=false}:{label:string;value:string;strong?:boolean}){return <div className={strong?"summary-row strong":"summary-row"}><span>{label}</span><b>{value}</b></div>}
function Operation({record}:{record:RecordItem}){return <article className={`operation ${isFailure(record.status)?"failed":""}`}><div className="operation-route"><span>{names[record.sourceAccount??""]??record.sourceAccount??"CASP"}</span><i>→</i><span>{names[record.destinationAccount??""]??record.destinationAccount??"CASP"}</span></div><strong>{rawToRusd(record.quantityRaw)} rUSD</strong><small>{operationLabel(record.orderType)} · {new Date(record.createdAtUnixMs).toLocaleString("pl-PL")}</small><span className="operation-status">{record.status}</span></article>}
function formatRaw(value:string|undefined|null){return value===undefined||value===null?"—":`${rawToRusd(value)} rUSD`}
function formatSigned(value:string|undefined|null){if(value===undefined||value===null)return "—";const number=Number(value)/1_000_000;return `${number>0?"+":""}${number.toLocaleString("pl-PL",{maximumFractionDigits:6})} rUSD`}
function shortHash(value:string|undefined|null){return value?`${value.slice(0,10)}…${value.slice(-6)}`:"—"}
function statusLabel(value:"balanced"|"warning"|"blocked"|"unavailable"){return {balanced:"Zgodność potwierdzona",warning:"Pokrycie z ostrzeżeniem",blocked:"Operacje zakupowe zablokowane",unavailable:"Brak wiarygodnych danych"}[value]}
function operationLabel(value:string){return value==="purchase"?"zakup":value==="sale"?"sprzedaż":value}
function isFailure(value:string){return ["failed","rejected","cancelled"].includes(value.toLowerCase())}
