import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { ArrowDownToLine, ArrowUpFromLine, Building2, RefreshCw, Send, ShieldCheck, UserRound } from "lucide-react"
import { FormEvent, useState } from "react"
import { api } from "./api"
import { rawToRusd, usdToMinor, usdToRaw } from "./amounts"
import { AdminDashboard } from "./admin-dashboard"
import { StatementPage } from "./statement"
import "./token-info.css"

const clients: Record<string,string>={alice:"Alicja Kowalska",bob:"Bartosz Nowak",carol:"Karolina Wiśniewska"}

export function App(){
  return window.location.pathname.startsWith("/admin")?<AdminDashboard/>:window.location.pathname.startsWith("/statement")?<StatementPage/>:<CustomerApp/>
}

function CustomerApp(){
  const queryClient=useQueryClient()
  const [clientId,setClientId]=useState("alice")
  const [mode,setMode]=useState<"purchase"|"sale"|"transfer">("purchase")
  const [amount,setAmount]=useState("100.00")
  const [recipientId,setRecipientId]=useState("bob")
  const [purpose,setPurpose]=useState("private_transfer")
  const account=useQuery({queryKey:["account",clientId],queryFn:()=>api.account(clientId),refetchInterval:3000})
  const accounts=useQuery({queryKey:["accounts"],queryFn:api.accounts,refetchInterval:3000})
  const records=useQuery({queryKey:["records",clientId],queryFn:()=>api.records(clientId),refetchInterval:3000})
  const tokenInfo=useQuery({queryKey:["token-information"],queryFn:api.tokenInformation,refetchInterval:10_000})
  const recipientWallet=accounts.data?.find(item=>item.clientId===recipientId)?.walletAddress??recipientId
  const mutation=useMutation<unknown,Error>({mutationFn:()=>mode==="purchase"?api.purchase(clientId,crypto.randomUUID(),usdToMinor(amount)):mode==="sale"?api.sale(clientId,crypto.randomUUID(),usdToRaw(amount)):api.transfer(clientId,crypto.randomUUID(),recipientWallet,usdToRaw(amount),purpose),onSuccess:()=>queryClient.invalidateQueries()})
  const submit=(event:FormEvent)=>{event.preventDefault();mutation.mutate()}
  const changeClient=(value:string)=>{setClientId(value);if(recipientId===value)setRecipientId(Object.keys(clients).find(id=>id!==value)??"alice");mutation.reset()}
  return <main className="customer-dashboard">
    <header><div className="brand"><Building2/> rUSD CASP</div><div className="account-switcher"><UserRound/><label htmlFor="client">Testuj jako</label><select id="client" value={clientId} onChange={event=>changeClient(event.target.value)}>{Object.entries(clients).map(([id,name])=><option key={id} value={id}>{name}</option>)}</select></div><a className="route-link" href={`/statement?client=${clientId}`}>Wyciąg klienta</a><a className="route-link" href="/admin">Panel administratora</a><span className="badge"><ShieldCheck/> środowisko demonstracyjne</span></header>
    <div className="customer-workspace">
      <section className="customer-account-column">
        <section className="hero"><div><p className="eyebrow">KONTO: {clients[clientId].toUpperCase()}</p><h1>{account.data?rawToRusd(account.data.availableRaw):"—"} rUSD</h1><p>Prawo klienta do części tokenów przechowywanych przez CASP</p><p className="wallet-address">Logiczny adres odbiorczy: <strong>{account.data?.walletAddress??"—"}</strong></p></div><div className="locked">Zablokowane<strong>{account.data?rawToRusd(account.data.lockedRaw):"—"} rUSD</strong></div></section>
        <section className="card token-info"><h2>Informacje o rUSD</h2>{tokenInfo.data?<><p><strong>{tokenInfo.data.name}</strong></p><div className="issuer-parity"><span>Parytet emisji i wykupu u emitenta</span><strong>{tokenInfo.data.parityStatement}</strong><small>To odniesienie tokenu do USD, a nie detaliczny kurs transakcyjny CASP.</small></div><div className="token-facts"><span>Stan emitenta<b>{tokenInfo.data.assetState}</b></span><span>Pokrycie rezerw<b>{tokenInfo.data.reserveCoveragePercent==null?"—":`${tokenInfo.data.reserveCoveragePercent.toFixed(2)}%`}</b></span><span>Sieć / kontrakt<b>{tokenInfo.data.chainId} · {shortAddress(tokenInfo.data.contractAddress)}</b></span><span>Metodologia ESG<b>{tokenInfo.data.esgMethodologyVersion}</b></span></div><p className="hint">{tokenInfo.data.esgNote}</p><a className="route-link" href={tokenInfo.data.whitePaperUrl} target="_blank" rel="noreferrer">Dokument informacyjny emitenta</a><small>{tokenInfo.data.disclaimer}</small></>:<p className="hint">{tokenInfo.error?tokenInfo.error.message:"Pobieranie informacji bezpośrednio ze źródła emitenta…"}</p>}</section>
      </section>
      <section className="card customer-operation-column"><nav><button className={mode==="purchase"?"active":""} onClick={()=>setMode("purchase")}><ArrowDownToLine/> Kup</button><button className={mode==="sale"?"active":""} onClick={()=>setMode("sale")}><ArrowUpFromLine/> Sprzedaj</button><button className={mode==="transfer"?"active":""} onClick={()=>setMode("transfer")}><Send/> Przelej</button></nav><form onSubmit={submit}><label>{mode==="transfer"?"Kwota brutto w rUSD":mode==="purchase"?"Kwota płatności w USD":"Liczba sprzedawanych rUSD"}</label><input value={amount} onChange={e=>setAmount(e.target.value)} inputMode="decimal"/>{mode==="transfer"&&<div className="transfer-fields"><label>Odbiorca<select value={recipientId} onChange={event=>setRecipientId(event.target.value)}>{Object.entries(clients).filter(([id])=>id!==clientId).map(([id,name])=><option key={id} value={id}>{name}</option>)}</select></label><label>Cel przelewu<select value={purpose} onChange={event=>setPurpose(event.target.value)}><option value="private_transfer">Przelew prywatny</option><option value="goods_or_services">Towary lub usługi</option></select></label></div>}{mode!=="transfer"&&<div className="exchange-rate"><span>Kurs transakcyjny CASP · demo</span><strong>1 rUSD = 1,00 USD</strong><small>CASP ustala cenę kupna i sprzedaży dla klienta. W realnym systemie może ona różnić się od parytetu wykupu u emitenta; w demonstracji nie symulujemy wahań ani spreadu.</small></div>}<p className="hint">{mode==="transfer"?"Przelew jest bezgasową operacją w rejestrze CASP. Z kwoty brutto pobierana jest demonstracyjna opłata transakcyjna 0,1%; odbiorca otrzymuje kwotę netto.":"Zakup i sprzedaż aktualizują wyłącznie wewnętrzny rejestr CASP — bez transakcji blockchainowej. Wyświetlany kurs jest ofertą CASP, a nie gwarancją emitenta."}</p><button className="primary" disabled={mutation.isPending}>{mutation.isPending?<><RefreshCw className="spin"/> Przetwarzanie…</>:mode==="purchase"?"Kup rUSD":mode==="sale"?"Sprzedaj rUSD":"Przelej rUSD"}</button>{mutation.error&&<p className="error">{mutation.error.message}</p>}{mutation.isSuccess&&<p className="success">Operacja zakończona i zapisana w rejestrze.</p>}</form></section>
      <section className="card history customer-history-column"><h2>Rejestr operacji</h2><p className="hint">Historia konta {clients[clientId]}. Kwota otrzymanego przelewu uwzględnia potrąconą prowizję.</p><div className="table">{records.data?.map(record=><CustomerRecord key={record.recordId} record={record} clientId={clientId}/>)}{records.data?.length===0&&<p>Brak operacji.</p>}</div></section>
    </div>
  </main>
}
function shortAddress(value:string){return `${value.slice(0,8)}…${value.slice(-6)}`}
function CustomerRecord({record,clientId}:{record:import("./api").RecordItem;clientId:string}){const transfer=record.orderType==="internal_transfer";const sent=transfer&&record.sourceAccount===clientId;const received=transfer&&record.destinationAccount===clientId;const external=record.orderType==="external_deposit";const amount=received?record.netQuantityRaw:record.grossQuantityRaw;const title=sent?"Wysłano przelew":received?"Otrzymano przelew":external?"Wpłata zewnętrzna":record.orderType==="purchase"?"Zakup":record.orderType==="sale"?"Sprzedaż":"Wykup";return <article><div><strong>{title} · {rawToRusd(amount)} rUSD</strong>{sent&&Number(record.feeQuantityRaw)>0&&<small>Prowizja CASP: {rawToRusd(record.feeQuantityRaw)} rUSD · odbiorca otrzymał {rawToRusd(record.netQuantityRaw)} rUSD</small>}{external&&<small>Nadawca on-chain: {record.sourceAccount??"—"}</small>}<small>{new Date(record.createdAtUnixMs).toLocaleString("pl-PL")} · {record.operationId}</small></div><span className={`status ${record.status}`}>{record.status==="completed"?"zakończona":record.status}</span></article>}
