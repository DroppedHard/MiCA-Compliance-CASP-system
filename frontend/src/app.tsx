import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { ArrowDownToLine, ArrowUpFromLine, Building2, RefreshCw, Send, ShieldCheck, UserRound } from "lucide-react"
import { FormEvent, useState } from "react"
import { api } from "./api"
import { rawToRusd, usdToMinor, usdToRaw } from "./amounts"
import { AdminDashboard } from "./admin-dashboard"

const clients: Record<string,string>={alice:"Alicja Kowalska",bob:"Bartosz Nowak",carol:"Karolina Wiśniewska"}

export function App(){
  return window.location.pathname.startsWith("/admin")?<AdminDashboard/>:<CustomerApp/>
}

function CustomerApp(){
  const queryClient=useQueryClient()
  const [clientId,setClientId]=useState("alice")
  const [mode,setMode]=useState<"purchase"|"sale"|"transfer">("purchase")
  const [amount,setAmount]=useState("100.00")
  const [recipientId,setRecipientId]=useState("bob")
  const [purpose,setPurpose]=useState("private_transfer")
  const account=useQuery({queryKey:["account",clientId],queryFn:()=>api.account(clientId),refetchInterval:3000})
  const records=useQuery({queryKey:["records",clientId],queryFn:()=>api.records(clientId),refetchInterval:3000})
  const mutation=useMutation<unknown,Error>({mutationFn:()=>mode==="purchase"?api.purchase(clientId,crypto.randomUUID(),usdToMinor(amount)):mode==="sale"?api.sale(clientId,crypto.randomUUID(),usdToRaw(amount)):api.transfer(clientId,crypto.randomUUID(),recipientId,usdToRaw(amount),purpose),onSuccess:()=>queryClient.invalidateQueries()})
  const submit=(event:FormEvent)=>{event.preventDefault();mutation.mutate()}
  const changeClient=(value:string)=>{setClientId(value);if(recipientId===value)setRecipientId(Object.keys(clients).find(id=>id!==value)??"alice");mutation.reset()}
  return <main>
    <header><div className="brand"><Building2/> rUSD CASP</div><div className="account-switcher"><UserRound/><label htmlFor="client">Testuj jako</label><select id="client" value={clientId} onChange={event=>changeClient(event.target.value)}>{Object.entries(clients).map(([id,name])=><option key={id} value={id}>{name}</option>)}</select></div><a className="route-link" href="/admin">Panel administratora</a><span className="badge"><ShieldCheck/> środowisko demonstracyjne</span></header>
    <section className="hero"><div><p className="eyebrow">KONTO: {clients[clientId].toUpperCase()}</p><h1>{account.data?rawToRusd(account.data.availableRaw):"—"} rUSD</h1><p>Prawo klienta do części tokenów przechowywanych przez CASP</p></div><div className="locked">Zablokowane<strong>{account.data?rawToRusd(account.data.lockedRaw):"—"} rUSD</strong></div></section>
    <div className="grid"><section className="card"><nav><button className={mode==="purchase"?"active":""} onClick={()=>setMode("purchase")}><ArrowDownToLine/> Kup</button><button className={mode==="sale"?"active":""} onClick={()=>setMode("sale")}><ArrowUpFromLine/> Sprzedaj</button><button className={mode==="transfer"?"active":""} onClick={()=>setMode("transfer")}><Send/> Przelej</button></nav><form onSubmit={submit}><label>{mode==="transfer"?"Kwota brutto w rUSD":"Kwota w USD"}</label><input value={amount} onChange={e=>setAmount(e.target.value)} inputMode="decimal"/>{mode==="transfer"&&<div className="transfer-fields"><label>Odbiorca<select value={recipientId} onChange={event=>setRecipientId(event.target.value)}>{Object.entries(clients).filter(([id])=>id!==clientId).map(([id,name])=><option key={id} value={id}>{name}</option>)}</select></label><label>Cel przelewu<select value={purpose} onChange={event=>setPurpose(event.target.value)}><option value="private_transfer">Przelew prywatny</option><option value="goods_or_services">Towary lub usługi</option></select></label></div>}<p className="hint">{mode==="transfer"?"Przelew jest bezgasową operacją w ledgerze CASP. Z kwoty brutto pobierana jest demonstracyjna opłata transakcyjna 0,1%; odbiorca otrzymuje kwotę netto.":"Kurs demonstracyjny: 1 rUSD = 1 USD. Zakup i sprzedaż aktualizują wyłącznie wewnętrzny ledger CASP — bez transakcji blockchainowej."}</p><button className="primary" disabled={mutation.isPending}>{mutation.isPending?<><RefreshCw className="spin"/> Przetwarzanie…</>:mode==="purchase"?"Kup rUSD":mode==="sale"?"Sprzedaj rUSD":"Przelej rUSD"}</button>{mutation.error&&<p className="error">{mutation.error.message}</p>}{mutation.isSuccess&&<p className="success">Operacja zakończona i zapisana w rejestrze.</p>}</form></section><section className="card"><h2>Nieprzypisany zapas CASP</h2><div className="metric">{account.data?rawToRusd(account.data.inventoryAvailableRaw):"—"} rUSD</div><p className="hint">Tokeny dostępne do przypisania klientom. Cała pula nadal pozostaje w portfelach powierniczych CASP.</p></section></div>
    <section className="card history"><h2>Rejestr operacji: {clients[clientId]}</h2><p className="hint">Każde konto ma osobne saldo i historię w bazie CASP.</p><div className="table">{records.data?.map(record=><article key={record.recordId}><div><strong>{record.orderType==="purchase"?"Zakup":record.orderType==="sale"?"Sprzedaż":record.orderType==="internal_transfer"?"Przelew wewnętrzny":"Wykup"} {rawToRusd(record.quantityRaw)} rUSD</strong><small>{new Date(record.createdAtUnixMs).toLocaleString("pl-PL")} · {record.operationId}</small></div><span className={`status ${record.status}`}>{record.status}</span></article>)}{records.data?.length===0&&<p>Brak operacji.</p>}</div></section>
  </main>
}
