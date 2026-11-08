/**
 * Flowmark prelude v0.1
 *
 * The fixed vocabulary for *.review.ts files. A review file may only call
 * what is declared here, plus functions and types it defines itself.
 * Review files have no imports or exports.
 *
 * The checker type-checks every review file in its own program together
 * with this prelude (see SPEC.md §12):
 *   strict: true, noUnusedLocals: true, lib: ["es2022"], types: []
 *
 * Every declaration has a @category. Viewers use it for colours and for the
 * badges on collapsed blocks; the checker uses it to compare the review card
 * with the body.
 *
 *   entry        where execution starts: entry
 *   access       who may do this: Access, authorize
 *   input        untrusted data and its validation: Raw, shape (entry `input`), validate
 *   read         reads: db.find/list/count, cache.get, files.read
 *   write        persistent changes: db.insert/update/upsert/delete, cache.set/forget, files.write/delete
 *   effect       things that leave the process: emit, queue, mail, notify, external, log
 *   output       what the caller gets: respond, redirect, return value of handle
 *   failure      how it stops: fail, assert
 *   consistency  what happens together or not at all: db.transaction, db.afterCommit, lock
 *   config       behaviour that depends on environment or flags: config, flag
 *   util         pure helpers: hash, pick, omit, ...
 *
 * @irreversible marks calls that a rollback cannot undo.
 */

// ─── basic types ─────────────────────────────────────────────────────────────

declare const __brand: unique symbol
type Brand<T, B extends string> = T & { readonly [__brand]: B }

/**
 * Unvalidated data from outside. Anything derived from it is untrusted.
 * @category input
 */
type Raw = any

type Id = string | number
type Email = Brand<string, "Email">
/** One-way hashed value (passwords). */
type Hashed = Brand<string, "Hashed">
/** Clear-text secret: token, key, password. Must never reach output, logs or mail. */
type Secret = Brand<string, "Secret">
/** "30s", "5m", "1h", "7d" */
type Duration = string

// ─── access ──────────────────────────────────────────────────────────────────

/**
 * Who may trigger an entry point.
 * @category access
 */
type Access =
  /** anyone, no login */
  | "public"
  /** any logged-in user */
  | "user"
  /** other services only (signed request, private network) */
  | "internal"
  /** no outside caller: queue, scheduler, event listener */
  | "system"
  | { role: string | string[] }
  | { ability: string }
  /** anyone holding a valid signature, e.g. a webhook: { signature: "Stripe" } */
  | { signature: string }

/** The caller. A review file may add fields by declaring `interface Actor { ... }`. */
interface Actor {
  id: Id
  roles: string[]
}

/**
 * Fails with 403 unless the current actor may do `ability`, optionally on `subject`.
 * @category access
 */
declare function authorize(ability: string, subject?: unknown): void

// ─── entry points ────────────────────────────────────────────────────────────

type HttpMethod = "GET" | "POST" | "PUT" | "PATCH" | "DELETE"

type HttpTrigger = `${HttpMethod} /${string}`
type EventTrigger = `event ${keyof Events & string}`
type JobTrigger = `job ${keyof Jobs & string}`
type MessageTrigger = `message ${string}`
type ScheduleTrigger = `schedule ${string}`
type CliTrigger = `cli ${string}`
type GraphqlTrigger = `query ${string}` | `mutation ${string}` | `subscription ${string}`
type RpcTrigger = `rpc ${string}`
type WsTrigger = `ws ${string}`

/**
 * What starts an entry point: one string, "<kind> <name>".
 *
 *   "POST /api/users"          HTTP request, including webhooks
 *   "event UserRegistered"     event from this app (declared in Events)
 *   "job SendInvoice"          queued job from this app (declared in Jobs)
 *   "message orders.created"   message from a broker: Kafka, SQS, RabbitMQ, ...
 *   "schedule daily 03:00"     time: cron expression or plain words
 *   "cli users:import"         console command
 *   "mutation createUser"      GraphQL: query / mutation / subscription
 *   "rpc UserService.Create"   RPC: gRPC, ...
 *   "ws chat.message"          WebSocket message
 *
 * @category entry
 */
type Trigger =
  | HttpTrigger
  | EventTrigger
  | JobTrigger
  | MessageTrigger
  | ScheduleTrigger
  | CliTrigger
  | GraphqlTrigger
  | RpcTrigger
  | WsTrigger

/** Input of an HTTP entry. */
interface HttpRequest {
  /** body + query string, unvalidated */
  raw: Raw
  /** path parameters, unvalidated */
  params: Record<string, Raw>
  headers: Record<string, Raw>
  /** null when nobody is logged in */
  actor: Actor | null
  ip: string
}

/** Input of a GraphQL, RPC or WebSocket entry. */
interface CallInput {
  /** arguments or message body, unvalidated */
  raw: Raw
  /** headers or metadata, unvalidated */
  meta: Record<string, Raw>
  actor: Actor | null
}

/** Input of a broker message entry. The body comes from another system. */
interface MessageInput {
  /** message body, unvalidated */
  raw: Raw
  meta: Record<string, Raw>
}

/** Input of a console command. */
interface CliInput {
  args: Record<string, Raw>
  options: Record<string, Raw>
}

/** What `handle` receives, derived from the trigger. */
type Payload<T, I = never> =
  T extends HttpTrigger ? HttpRequest & WithInput<I> :
  T extends `event ${infer K extends keyof Events}` ? Events[K] :
  T extends `job ${infer K extends keyof Jobs}` ? Jobs[K] :
  T extends MessageTrigger ? MessageInput & WithInput<I> :
  T extends CliTrigger ? CliInput & WithInput<I> :
  T extends GraphqlTrigger | RpcTrigger | WsTrigger ? CallInput & WithInput<I> :
  void

/** `input` on what `handle` receives, when the entry declares `input: shape<I>()`. */
type WithInput<I> = [I] extends [never] ? {} : {
  /** the input, already validated against I before `handle` runs */
  input: I
}

declare const __shape: unique symbol
/** What `shape<I>()` returns. */
interface Shape<I> {
  readonly [__shape]: I
}

/**
 * The shape an entry's input is validated against before `handle` runs (body, arguments, message):
 *
 *   input: shape<SignupInput>(),
 *
 * Invalid input is rejected with the framework's validation error (HTTP: usually 422 or 400) and `handle`
 * never runs. If the status matters, say it in a comment.
 * `handle` then reads `req.input` (validated) instead of `req.raw` (not validated).
 * @category input
 */
declare function shape<I>(): Shape<I>

/**
 * Event name → payload. Each review file declares the events it uses:
 *   interface Events { UserRegistered: { user: User } }
 */
interface Events {}

/**
 * Job name → payload. Each review file declares the jobs it uses:
 *   interface Jobs { SendInvoice: { orderId: Id } }
 */
interface Jobs {}

/** Options every entry has. */
interface EntryBase<T extends Trigger, I> {
  /** what starts it */
  trigger: T
  /** who may start it; required for every entry, "system" when nobody outside can */
  access: Access
  /**
   * The shape the input is validated against before `handle` runs. HTTP, GraphQL, RPC, WebSocket,
   * message and cli entries only. Leave it out when the code validates nothing up front.
   */
  input?: Shape<I>
  /** middleware, guards or interceptors worth knowing about, by name */
  guards?: string[]
  timeout?: Duration
  /** the flow. For HTTP, GraphQL, RPC and WebSocket the return value is the output. */
  handle(input: Payload<T, I>): unknown
}

/** HTTP, GraphQL, RPC, WebSocket */
interface RequestOptions {
  /** e.g. "10/min" */
  rateLimit?: string
}

/** jobs, messages, events */
interface RetryOptions {
  /** retries after the first attempt (Laravel `$tries = 3` is `retries: 2`) */
  retries?: number
  backoff?: Duration
  /** at most one run per key at a time */
  unique?: boolean
  /** safe to run twice with the same input; the checker warns when retries > 0 without it */
  idempotent?: boolean
}

interface EventOptions extends RetryOptions {
  /**
   * required: true = runs later on a queue;
   * false = runs inside the emitter, before `emit` returns (and inside its transaction)
   */
  queued: boolean
}

interface ScheduleOptions {
  /** may a new run start while the last one is still running? */
  overlap?: boolean
  /** runs on one server only */
  single?: boolean
}

/** Extra options allowed for each kind of trigger. */
/** Entries started by this app or by time take no `input`: their payload is already typed. */
interface NoInput {
  input?: never
}

type EntryOptions<T> =
  T extends HttpTrigger | GraphqlTrigger | RpcTrigger | WsTrigger ? RequestOptions :
  T extends EventTrigger ? EventOptions & NoInput :
  T extends JobTrigger ? RetryOptions & NoInput :
  T extends MessageTrigger ? RetryOptions :
  T extends ScheduleTrigger ? ScheduleOptions & NoInput :
  {}

type EntryDef<T extends Trigger, I> = EntryBase<T, I> & EntryOptions<T>

interface Entry {
  readonly trigger: Trigger
}

/**
 * Declares an entry point. Always assign it to a const with a domain name:
 *
 *   const registerUser = entry({
 *     trigger: "POST /api/users",
 *     access: "public",
 *     input: shape<SignupInput>(),
 *     rateLimit: "10/min",
 *     async handle(req): Promise<UserResponse> { ... req.input ... },
 *   })
 *
 * What `handle` receives follows from the trigger, plus `input` when the entry declares one.
 * @category entry
 */
declare function entry<T extends Trigger, I = never>(def: EntryDef<T, I>): Entry

// ─── input ───────────────────────────────────────────────────────────────────

/**
 * Validates untrusted data against T, as a step in the flow. Fails with the code's validation error
 * (HTTP: usually 422 or 400) when invalid.
 * Returns only the validated fields.
 *
 * For the entry's own input that the framework validates before `handle` runs, declare
 * `input: shape<T>()` on the entry instead. Use `validate` for validation that really is a step:
 * in the middle of the handler, of something other than the request (a webhook body, a file,
 * an external API's answer), or of only part of the input.
 * @category input
 */
declare function validate<T>(source: HttpRequest | Raw): T

// ─── failure ─────────────────────────────────────────────────────────────────

/**
 * Stops and answers with an error status.
 * @category failure
 */
declare function fail(status: number, reason?: string): never

/**
 * Invariant. Fails with `status` (default 500) when the condition is false.
 * @category failure
 */
declare function assert(condition: unknown, status?: number, reason?: string): asserts condition

// ─── data ────────────────────────────────────────────────────────────────────

type Where<T> = Partial<T>

interface FindOptions {
  /** eager-loaded relations */
  with?: string[]
  /** row lock until the transaction ends (SELECT … FOR UPDATE) */
  lock?: boolean
}

interface ListOptions extends FindOptions {
  /** no limit = unbounded; the checker warns */
  limit?: number
  orderBy?: string
}

/**
 * Persistent storage. The type argument names the model or table:
 *   db.insert<User>({ ... })
 * so the checker can compare writes with the review card.
 */
interface Db {
  /** @category read */
  find<T>(where: Where<T>, options?: FindOptions): Promise<T | null>
  /** Fails with 404 when missing. @category read */
  findOrFail<T>(where: Where<T>, options?: FindOptions): Promise<T>
  /** @category read */
  list<T>(where: Where<T>, options?: ListOptions): Promise<T[]>
  /** @category read */
  count<T>(where: Where<T>): Promise<number>

  /** @category write */
  insert<T>(data: Partial<T>): Promise<T>
  /** Returns the number of changed rows. @category write */
  update<T>(where: Where<T>, data: Partial<T>): Promise<number>
  /** @category write */
  upsert<T>(where: Where<T>, data: Partial<T>): Promise<T>
  /** Returns the number of deleted rows. @category write */
  delete<T>(where: Where<T>, options?: { soft?: boolean }): Promise<number>

  /** Hand-written query. Always say in a comment what it reads or writes. @category read */
  query(sql: string, ...params: unknown[]): Promise<unknown>

  /** Everything inside commits or rolls back together. @category consistency */
  transaction<T>(fn: () => Promise<T>): Promise<T>
  /** Runs after the surrounding transaction commits (right away if there is none). @category consistency */
  afterCommit(fn: () => unknown): void
}

declare const db: Db

interface CacheStore {
  /** @category read */
  get<T>(key: string): Promise<T | null>
  /** @category write */
  set(key: string, value: unknown, ttl?: Duration): Promise<void>
  /** @category write */
  forget(key: string): Promise<void>
  /** Read, or compute and store. @category read */
  remember<T>(key: string, ttl: Duration, compute: () => Promise<T>): Promise<T>
}

declare const cache: CacheStore

interface FileStore {
  /** @category read */
  read(path: string): Promise<Raw>
  /** @category write */
  write(path: string, data: unknown, options?: { visibility?: "public" | "private" }): Promise<void>
  /** @category write */
  delete(path: string): Promise<void>
  /** A link that gives access to the file. @category output */
  url(path: string, options?: { expires?: Duration }): string
}

declare const files: FileStore

// ─── consistency ─────────────────────────────────────────────────────────────

/**
 * Lock shared by all servers: only one holder of `key` runs `fn` at a time.
 * @category consistency
 */
declare function lock<T>(key: string, fn: () => Promise<T>, options?: { wait?: Duration }): Promise<T>

// ─── effects ─────────────────────────────────────────────────────────────────

/**
 * Sends an event to its listeners. Whether a listener runs now or later is
 * decided by the listener's `queued` option.
 * @category effect
 */
declare function emit<K extends keyof Events>(event: K, payload: Events[K]): void

/**
 * Puts a job on the queue.
 * @category effect
 */
declare function queue<K extends keyof Jobs>(
  name: K,
  payload: Jobs[K],
  options?: { delay?: Duration; afterCommit?: boolean },
): void

/**
 * @category effect
 * @irreversible
 */
declare function mail(to: Email | Email[], template: string, data?: object): void

/**
 * SMS, push, chat message, ...
 * @category effect
 * @irreversible
 */
declare function notify(to: unknown, channel: string, message: string): void

interface ExternalService {
  [operation: string]: (...args: any[]) => Promise<any>
}

/**
 * Any call to another system: payment provider, third-party API, webhook,
 * another of our services. Each method call is one outgoing request:
 *   external("Stripe").createCustomer(email)
 * @category effect
 * @irreversible
 */
declare function external(service: string): ExternalService

/**
 * Only write a log line in a review file when it contains personal data
 * or is an audit trail. Ordinary logging is left out.
 * @category effect
 */
declare function log(level: "debug" | "info" | "warn" | "error", message: string, context?: object): void

// ─── output ──────────────────────────────────────────────────────────────────

/**
 * Answers with a status and body. A plain `return x` in an HTTP entry means 200 with body x.
 * @category output
 */
declare function respond<T>(status: number, body?: T, headers?: Record<string, string>): T

/** @category output */
declare function redirect(to: string, status?: 301 | 302 | 303 | 307 | 308): never

// ─── config ──────────────────────────────────────────────────────────────────

/**
 * Value from environment or config. Behaviour differs between environments.
 * @category config
 */
declare function config<T = any>(key: string): T

/**
 * Feature flag, optionally per subject (user, team, ...).
 * @category config
 */
declare function flag(name: string, subject?: unknown): boolean

// ─── util ────────────────────────────────────────────────────────────────────

/** @category util */
declare function hash(value: string): Hashed
/** @category util */
declare function checkHash(value: string, hashed: Hashed): boolean
/** @category util */
declare function encrypt(value: unknown): string
/** @category util */
declare function decrypt<T = any>(value: string): T
/** Random, unguessable token. @category util */
declare function randomToken(bytes?: number): Secret
/** @category util */
declare function now(): Date
/** @category util */
declare function pick<T, K extends keyof T>(obj: T, ...keys: K[]): Pick<T, K>
/** @category util */
declare function omit<T, K extends keyof T>(obj: T, ...keys: K[]): Omit<T, K>
