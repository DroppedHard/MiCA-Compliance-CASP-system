import { Effect, Schema } from "effect"

export const request = <A>(path: string, init: RequestInit | undefined, schema: Schema.Schema<A>) => Effect.runPromise(Effect.gen(function* () {
  const response = yield* Effect.tryPromise({ try: () => fetch(path, init), catch: () => new Error("Backend CASP jest niedostępny") })
  if (!response.ok) {
    const text = yield* Effect.tryPromise({ try: () => response.text(), catch: () => new Error(`Błąd HTTP ${response.status}`) })
    let message = `Błąd HTTP ${response.status}`
    try { const body = JSON.parse(text) as { error?: unknown }; if (typeof body.error === "string") message = body.error } catch { /* Proxy może zwrócić HTML. */ }
    return yield* Effect.fail(new Error(message))
  }
  const value = yield* Effect.tryPromise({ try: () => response.json(), catch: () => new Error("Backend CASP zwrócił niepoprawną odpowiedź JSON") })
  return yield* Schema.decodeUnknown(schema)(value)
}))

export const postJson = (value: unknown): RequestInit => ({ method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(value) })
export const clientPath = (clientId: string) => `/api/v1/clients/${encodeURIComponent(clientId)}`
