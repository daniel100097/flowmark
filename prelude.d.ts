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
 *   entry        where execution starts: route, job, on, cli, schedule
 *   access       who may do this: Access, authorize
 *   input        untrusted data and its validation: Raw, validate
 *   read         reads: db.find/list/count, cache.get, files.read
 *   write        persistent changes: db.insert/update/upsert/delete, cache.set/forget, files.write/delete
 *   effect       things that leave the process: emit, queue, mail, notify, external, log
 *   output       what the caller gets: respond, redirect, return value of a route
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

interface HttpRequest {
  /** body + query string, unvalidated */
  raw: Raw
  /** path parameters, unvalidated */
  params: Record<string, Raw>
  headers: Record<string, Raw>
  /** null when access is "public" and nobody is logged in */
  actor: Actor | null
  ip: string
}

interface RouteOptions {
  /** required: who may call this */
  access: Access
  /** e.g. "10/min" */
  rateLimit?: string
  /** other middleware or guards worth knowing about, by name */
  guards?: string[]
}

type RouteHandler = (req: HttpRequest) => unknown

/** @category entry */
interface Router {
  get(path: string, options: RouteOptions, handler: RouteHandler): void
  post(path: string, options: RouteOptions, handler: RouteHandler): void
  put(path: string, options: RouteOptions, handler: RouteHandler): void
  patch(path: string, options: RouteOptions, handler: RouteHandler): void
  delete(path: string, options: RouteOptions, handler: RouteHandler): void
}

/**
 * HTTP entry point. The handler's return value is the response body (200),
 * unless it calls `respond` or `redirect`.
 * @category entry
 */
declare const route: Router

interface AsyncOptions {
  /** default "system" */
  access?: Access
  /** runs later on a queue (true) or inside the caller's process (false) */
  queued: boolean
  retries?: number
  backoff?: Duration
  timeout?: Duration
  /** at most one run per key at a time */
  unique?: boolean
  /** safe to run twice with the same input; the checker warns when retries > 0 without it */
  idempotent?: boolean
}

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

/**
 * Event listener.
 * @category entry
 */
declare function on<K extends keyof Events>(
  event: K,
  options: AsyncOptions,
  handler: (payload: Events[K]) => unknown,
): void

/**
 * Queued job.
 * @category entry
 */
declare function job<K extends keyof Jobs>(
  name: K,
  options: AsyncOptions,
  handler: (payload: Jobs[K]) => unknown,
): void

/**
 * Console command.
 * @category entry
 */
declare function cli(
  command: string,
  options: { access: Access },
  handler: (args: Record<string, Raw>) => unknown,
): void

interface ScheduleOptions {
  /** may a new run start while the last one is still running? */
  overlap?: boolean
  /** runs on one server only */
  single?: boolean
  timeout?: Duration
}

/**
 * Time-based entry point. `when` is a cron expression or plain words ("daily 03:00").
 * @category entry
 */
declare function schedule(when: string, options: ScheduleOptions, handler: () => unknown): void

// ─── input ───────────────────────────────────────────────────────────────────

/**
 * Validates untrusted input against T. Fails with 422 when invalid.
 * Returns only the validated fields.
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
 * decided by the listener's `on(..., { queued })`.
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
 * Answers with a status and body. A plain `return x` in a route means 200 with body x.
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
