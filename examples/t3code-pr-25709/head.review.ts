// flowmark 0.1
/**
 * Update server settings
 * A client patches the environment's server settings. Scopes are checked per patch key; the patch is
 * merged, secrets go to the secret store, settings.json is written atomically and pushed to clients.
 *
 * access:   scope settings:write, or providers:manage for provider and secret provider keys (updateServerSettings), system (pushSettingsToClients)
 * input:    SettingsUpdate, secret provider config unchecked !
 * writes:   settings.json, secrets/*.bin (rolled back if settings.json fails)
 * effects:  event SettingsChanged → notify settingsUpdated (clients with orchestration:read)
 * output:   ServerSettings, secrets redacted
 * config:   T3CODE_FEATURE_SECRETS (web build: hides the Secrets page only; the server accepts secretProviders in every build)
 * shared:   scopesForPatch, also used for project script edits
 * tests:    scope per patch key, per-entry merge, unreadable file refused, secret rollback; secretProviders: scope, merge, envelope decode
 */

// --- data ---

// ? keys the server does not know are dropped without an error (Effect Schema default for Struct)
type SettingsUpdate = { // @ WsServerUpdateSettingsRpc
  patch: ServerSettingsPatch
  providerInstanceMutation?: ProviderInstanceMutation
}

// 66 optional keys, one per settings field; only the keys this file talks about are listed
type ServerSettingsPatch = { // @ ServerSettingsPatch
  providerInstances?: Record<string, ProviderInstance>
  usageLimitSources?: Record<string, UsageLimitSource | null> // null removes the entry
  usagePriceOverrides?: Record<string, object | null> // null removes the entry
  usageModelAliases?: Record<string, string | null> // null removes the entry
  deviceHosts?: DeviceHost[]
  // only the web Secrets page writes it: add and edit send { [id]: config }, remove sends { [id]: null } @ SecretProviderDialog "updateSettings({ secretProviders: { [result.success.id]: result.success.config } })" @ EnvironmentSecretProviders "updateSettings({ secretProviders: { [id]: null } })"
  // ! `config` is never decoded on the server: for kind one_password it stores any account, including one starting with "-" that `op --account` would read as a flag; only the web dialog checks the account, so the code that will run `op` must decode it again @ SecretProviderConfig "config: Schema.optionalKey(Schema.Unknown)" @ secretProviderFromDraft "OnePasswordSecretProviderConfig.fields.account"
  secretProviders?: Record<SecretProviderId, SecretProviderWrite | null> // null removes the entry @ ServerSettingsPatch "Schema.Record(SecretProviderId, Schema.NullOr(SecretProviderConfigWrite))"
}

type ProviderInstanceMutation = { // @ ProviderInstanceMutation
  operation: "create" | "upsert" | "remove"
  instanceId: string
  instance?: ProviderInstance // not for "remove"
}

// 66 fields with decoding defaults; if one fails to decode, the server runs on defaults and refuses writes @ loadSettingsFromDisk "failed to parse settings.json, using defaults"
type ServerSettings = { // @ ServerSettings
  providerInstances: Record<string, ProviderInstance>
  usageLimitSources: Record<string, UsageLimitSource>
  usagePriceOverrides: Record<string, object>
  usageModelAliases: Record<string, string>
  deviceHosts: DeviceHost[]
  // not a secret field: sent as stored to every client with orchestration:read (pushSettingsToClients) @ mapSettingsSecrets
  secretProviders: Record<SecretProviderId, SecretProvider> // @ ServerSettings "secretProviders: Schema.Record(SecretProviderId, SecretProviderConfig)"
}

// sensitive environment variables and secret config fields are kept in the secret store, not in settings.json @ mapSettingsSecrets
type ProviderInstance = { driver: string; environment?: object; config?: unknown } // @ ProviderInstanceConfig
type UsageLimitSource = object // @ UsageLimitSourceConfig
type DeviceHost = { kind: "ssh"; target: string } // target never starts with "-" @ SshDeviceHostConfig "Schema.isPattern(/^[^\s-][^\s]*$/)"

type SecretProviderId = string // letter or digit first, then letters, digits, _ and -; max 64 @ SecretProviderSlug "Schema.isPattern(/^[a-zA-Z0-9][a-zA-Z0-9_-]*$/)"

// stored envelope, final once shipped: an entry an older build cannot decode makes it run on defaults @ packages/contracts/src/settings.ts "Final once shipped"
// tested: @ packages/contracts/src/settings.test.ts "keeps an unknown kind or a longer label from invalidating the rest of the settings"
type SecretProvider = { // @ SecretProviderConfig
  kind: string // a slug like SecretProviderId, e.g. one_password; unknown kinds are kept as they are @ "kind: SecretProviderKind"
  label: string // trimmed, not empty, no max length @ "label: TrimmedNonEmptyString"
  config?: Raw // decoded by each kind's reader, not by the server @ "config: Schema.optionalKey(Schema.Unknown)"
}

// a patch entry: label max 64, `config` still unchecked
type SecretProviderWrite = SecretProvider // @ SecretProviderConfigWrite "label: SecretProviderLabel"

// what the web dialog sends as `config` for kind one_password
// tested: @ packages/contracts/src/settings.test.ts "rejects a 1Password account op could misread"
type OnePasswordConfig = { // @ OnePasswordSecretProviderConfig
  account: string // shorthand, sign-in address or ID; no spaces, never starts with "-", max 253 @ OnePasswordAccount "ONE_PASSWORD_ACCOUNT_PATTERN.test(value)"
}

interface Events {
  SettingsChanged: { settings: ServerSettings } // in-memory PubSub @ apps/server/src/serverSettings.ts "PubSub.unbounded<ServerSettings>()"
}

// --- flow ---

// tested: @ RpcAuthorization.test.ts "allows provider-only mutations while denying mixed settings without their grant"
const updateServerSettings = entry({
  trigger: "rpc server.updateSettings", // Effect RPC over the WebSocket @ WsServerUpdateSettingsRpc "Rpc.make(WS_METHODS.serverUpdateSettings"
  access: { ability: "settings:write or providers:manage, by patch key" }, // @ requiredScopesForRpcCall "if (method === WS_METHODS.serverUpdateSettings) return requiredScopesForSettingsUpdate(payload);"
  input: shape<SettingsUpdate>(), // @ WsServerUpdateSettingsRpc "patch: ServerSettingsPatch,"
  async handle(call): Promise<ServerSettings> { // @ layerWsRpc "[WS_METHODS.serverUpdateSettings]:"
    // checked before the handler runs
    for (const scope of requiredScopes(call.input)) authorize(scope) // @ RpcAuthorization.ts "requiredScopes.find((scope) => !scopes.includes(scope))"
    // L3 SSH device hosts that resolve to this machine are dropped
    const patch = await dropLocalSshHosts(call.input.patch) // @ "remoteSshDeviceHosts(patch.deviceHosts)"
    // a provider instance mutation, if any, is applied after the patch in the same save
    const settings = await saveSettings((current) => applyPatch(current, patch, call.input.providerInstanceMutation)) // @ "providerInstanceMutation === undefined"
    return redactSecrets(settings) // @ "return ServerSettings.redactServerSettingsForClient(settings)"
  },
})

// --- details ---

function requiredScopes(input: SettingsUpdate): string[] { // @ requiredScopesForSettingsUpdate
  const scopes = scopesForPatch(input.patch) // @ "requiredScopesForServerSettingsPatch(input.patch)"
  if (input.providerInstanceMutation === undefined) return scopes
  // a mutation with an empty patch needs only providers:manage
  if (Object.values(input.patch).every((value) => value === undefined)) return ["providers:manage"] // @ "[AuthProvidersManageScope]"
  return [...new Set([...scopes, "providers:manage"])]
}

// used by: project script edits @ requiredScopesForProjectMutation "requiredScopesForServerSettingsPatch({ defaultProjectScripts: mutation.scripts })"
function scopesForPatch(patch: ServerSettingsPatch): string[] { // @ requiredScopesForServerSettingsPatch
  // tested: @ RpcAuthorization.test.ts "does not let a settings grant create or remove providers or secret providers"
  const providerKeys = ["providers", "providerInstances", "usageLimitSources", "secretProviders"] // @ "changesProviders = true"
  const keys = Object.keys(patch).filter((key) => patch[key as keyof ServerSettingsPatch] !== undefined) // L3 @ "if (value === undefined) continue;"
  const changesProviders = keys.some((key) => providerKeys.includes(key))
  const changesSettings = keys.some((key) => !providerKeys.includes(key))
  // a patch with only provider keys does not need settings:write
  return [...(changesSettings || !changesProviders ? ["settings:write"] : []), ...(changesProviders ? ["providers:manage"] : [])] // @ "...(changesSettings || !changesProviders ? [AuthSettingsWriteScope] : []),"
}

// one writer at a time; the file is read again before every change
async function saveSettings(change: (current: ServerSettings) => ServerSettings): Promise<ServerSettings> { // @ updateAndPersistSettings
  return lock("settings.json", async () => { // in-process semaphore @ "writeSemaphore.withPermits(1)"
    const loaded = await files.read("settings.json") // decoded and migrated @ "yield* reloadSettings"
    // a file that does not decode is kept for the user to repair; every write fails until then
    // tested: @ apps/server/src/serverSettings.test.ts "refuses to save over a settings file it cannot parse"
    assert(!loaded.parseFailure, 500, "ServerSettingsError read-file") // @ "if (Option.isSome(parseFailure))"
    const next = change(loaded.settings) // @ "yield* update(current)"
    const { settings, secretWrites } = splitSecrets(loaded.settings, next) // @ "persistSettingsSecrets(current, updated)"
    // L2 file mode 0600; removed values are deleted @ ServerSecretStore.ts "chmod(tempPath, 0o600)"
    for (const secret of secretWrites) await files.write(`secrets/${secret.name}.bin`, secret.value) // @ "applySettingsSecretChanges(persisted.changes)"
    // L2 a failed write puts the old secret values back; a failed rollback is only logged @ rollbackSettingsSecretWrites "failed to roll back settings secret"
    // tested: @ apps/server/src/serverSettings.test.ts "rolls back provider secret changes when the settings file commit fails"
    await files.write("settings.json", settings) // temp file + rename, defaults left out @ "writeSettingsAtomically(next)" @ writeSettingsAtomically "stripDefaultServerSettings(settings, DEFAULT_SERVER_SETTINGS)"
    emit("SettingsChanged", { settings }) // @ "yield* emitChange(next)"
    return settings
  })
}

// per entry (null removes an id, a value replaces that id's entry): usageLimitSources, secretProviders, usagePriceOverrides, usageModelAliases @ applyServerSettingsPatch "mergeSettingsEntries("
// !? add and edit send the same { [id]: config }: an add whose id another client has just taken replaces that provider; the duplicate-name check runs only in the web dialog @ secretProviderFromDraft "A secret provider with this name exists."
// every other key is deep-merged into the current settings @ applyServerSettingsPatch "deepMerge(current, patchForMerge)"
// tested: @ packages/shared/src/serverSettings.test.ts "upserts and removes usageLimitSources per entry so concurrent edits cannot clobber"
// tested: @ packages/shared/src/serverSettings.test.ts "upserts and removes secretProviders per entry so concurrent edits cannot clobber"
declare function applyPatch(current: ServerSettings, patch: ServerSettingsPatch, mutation?: ProviderInstanceMutation): ServerSettings // @ applyServerSettingsPatch

// sensitive values become { value: "", redacted: true }; every other field is sent as stored
declare function redactSecrets(settings: ServerSettings): ServerSettings // @ redactServerSettingsForClient

// sensitive values move out of the settings into secret writes; settings.json keeps an empty, redacted marker
declare function splitSecrets(current: ServerSettings, next: ServerSettings): { settings: ServerSettings; secretWrites: { name: string; value: Secret }[] } // @ persistSettingsSecrets

declare function dropLocalSshHosts(patch: ServerSettingsPatch): Promise<ServerSettingsPatch> // @ remoteSshDeviceHosts

// --- async ---

// other server services also listen to SettingsChanged (provider registry, devices, storage cleanup, …); not traced here
const pushSettingsToClients = entry({
  trigger: "event SettingsChanged", // one subscription per client @ layerWsRpc "yield* serverSettings.subscribeChanges"
  access: "system",
  queued: true, // each subscriber reads the PubSub in its own fiber @ apps/server/src/serverSettings.ts "PubSub.subscribe(changesPubSub)"
  handle({ settings }) { // @ layerWsRpc "const settingsUpdates = settingsChanges.pipe("
    // the whole settings object goes to every client subscribed with orchestration:read
    notify("server config subscribers", "settingsUpdated", JSON.stringify(redactSecrets(settings))) // @ RPC_REQUIRED_SCOPES "[WS_METHODS.subscribeServerConfig]: AuthOrchestrationReadScope"
  },
})
