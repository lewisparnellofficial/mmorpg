# Development Account Login and Character Entry Specification

**Status:** Proposed implementation specification

## Purpose

This specification defines the first player-facing account login and character
entry experience. It replaces the current shared development-token model with
a local server-owned account store while preserving the authoritative server,
typed session, and Linux-first constraints of the project.

This is a development-slice authentication boundary. It is not a production
account service, public deployment authorization system, encrypted transport,
or identity-recovery design.

## Player experience

The normal graphical client journey is:

```text
Launch mmorpg-client
  -> trusted native login screen
  -> account authentication
  -> trusted native character selection
  -> content compatibility and world-entry loading
  -> authoritative bootstrap snapshot
  -> normal scripted UI and game world
```

The player must see a login screen before the client authenticates. It must
provide:

- A username field.
- A password mode with a password field.
- An access-token mode with a token field.
- A Log In action.
- Clear connection, authentication, and rejection status.

Password and access-token credentials are mutually exclusive. The client must
not include entered secrets in HUD text, ordinary logs, diagnostics, errors,
or addon-visible state.

After successful authentication, the player sees only the characters returned
by the server for that account. Each entry shows at least character name and
role. The player selects one entry and enters the world only after the server
accepts the selection and the content compatibility check.

An account with no characters is a valid result. The character-selection
screen must show a clear message that starter accounts and characters are
provisioned by the local server administrator. Player account registration and
character creation are outside this milestone.

Authentication failure returns the client to the login screen with a clear
error. A connection failure may use the normal reconnect policy. A failed or
unknown automatic character selection returns to character selection without
displaying stale world state.

Login and character-selection screens are trusted native client UI. Player
addons and the ordinary scripted/default HUD load only after the selected
character has received an authoritative bootstrap and the session is ready.
Addons must not read credentials, alter account selection, or render over the
trusted credential controls.

## Command-line use and multiboxing

`mmorpg-client` is the user-facing executable. Command-line authentication is
a supported end-user feature for launchers and multibox play, not a test-only
bypass.

The client supports a positional server address and:

```text
--username USER
--password VALUE
--password-stdin
--auth-token VALUE
--token-file PATH
--character NAME
```

`--username` is required whenever a command-line credential is provided.
Exactly one credential source is allowed: `--password`, `--password-stdin`,
`--auth-token`, or `--token-file`. `--password-stdin` reads one password from
standard input for launchers and scripts. `--token-file` reads an account token
from a user-managed local file.

Literal `--password` and `--auth-token` are intentionally supported, but the
documentation must warn that shell history and local process inspection can
expose them. `--password-stdin` and a permission-restricted token file are the
recommended noninteractive forms.

For example:

```bash
mmorpg-client 127.0.0.1:4000 \
  --username lewis \
  --password 'correct horse battery staple' \
  --character Aria
```

Complete command-line credentials trigger the same login operation that the
native login screen triggers. If `--character NAME` is present, the client
waits for the authenticated server character list, resolves an exact
case-insensitive name match only within that list, selects that character, and
enters the world automatically. It must not submit a globally supplied
character ID or bypass account ownership validation.

If credentials are supplied without `--character`, the client authenticates
automatically and then shows graphical character selection. The client does
not persist usernames, passwords, or tokens across launches.

## Account store and administration

Every development server launch requires `--account-store PATH`. There is no
compiled-in account, shared fallback identity, or development-token fallback.

The server owns a private TOML account store. It is loaded and fully validated
at startup; account-store changes take effect only after a server restart.
The account store is local operational data, must not be committed, and must
be written atomically by the server's administration commands.

The store represents:

- Stable account IDs and normalized unique usernames.
- Password verifiers.
- Named, account-scoped access-token digests.
- Account-owned character records with stable IDs, names, and roles.
- A forward-compatible location for future SSH public-key records.

Passwords use Argon2id verifiers. Plaintext passwords are never written to
the account store. Access tokens are cryptographically random opaque secrets;
the store retains only their digests, owner account, and administrator-supplied
label. A raw issued token is printed once to the local administrator and must
not be logged elsewhere.

The server provides local account administration commands to:

- Initialize an account store.
- Create an account with a password.
- Add a character with a name and one of the starter roles.
- Issue a named account access token.
- Revoke a named account access token.

The administration surface is local server tooling, not a client registration
protocol. It must avoid accepting a plaintext password through a documented
persisted configuration value; its noninteractive password input supports
standard input.

One account may use multiple simultaneous client processes when each selects a
different character. A second active session for the same account and
character is rejected. Existing server-side character ownership and fencing
remain the enforcement point.

## Authentication, protocol, and authority

Username/password and access-token authentication are accepted only on a
loopback-bound listener. The client must not use this development credential
scheme over a remote listener before a future TLS and production-authentication
decision.

The typed protocol moves from v1 to v2. The v2 authentication request carries:

- A username.
- A credential method: password or access token.

The current v1 token-only authentication payload is retired. Its frozen
fixtures remain, and a v1 client receives the version-independent
`VersionRejected` control frame. The server does not maintain a dual v1/v2
authentication mode.

The renderer-independent session continues to own the post-authentication
state machine:

```text
Disconnected
  -> Connecting
  -> Authenticating
  -> AwaitingCharacterList
  -> AwaitingCharacterSelection
  -> EnteringWorld
  -> AwaitingBootstrap
  -> Ready
```

After authentication, the required sequence remains:

1. Request and receive the account's character list.
2. Select a returned character.
3. Receive server confirmation of that selection.
4. Send and receive acceptance of the content digest.
5. Request world entry and receive the authoritative connection result.
6. Request and atomically apply the authoritative bootstrap snapshot.

Only a `Ready` session may submit gameplay intent. The server resolves account
identity, character ownership, character binding, and gameplay authority; the
client and addons submit intent only.

On disconnect, the client clears presented-world state and pending gameplay
intents. A reconnect repeats authentication, character-list retrieval,
selection, content checking, world entry, and bootstrap. If the original
launch included `--character`, the client may automatically reselect that name
only after receiving the new account-scoped list.

The account and credential repository remains separate from sockets and from
the authoritative simulation owner. A future SSH authentication provider may
add public-key challenge/response and `--identity-file` or SSH-agent support
without giving players or addons direct socket, process, or filesystem access.
SSH-style authentication is not implemented by this specification.

## Validation requirements

Tests that validate `mmorpg-core`, content definitions, or presentation-model
projection remain independent of account authentication. They may construct
authoritative world state directly.

Session, server/client integration, diagnostic, reconnect, persistence,
slow-client, and graphical tests must exercise the real login flow. Every
such test creates an isolated temporary account store, provisions its account
and characters through the server administration surface, and starts the
server with `--account-store`.

Required focused coverage includes:

- Account-store initialization, parsing, validation, atomic writes, and
  malformed-store rejection.
- Password success and failure without plaintext persistence.
- Token issuance, authentication, revocation, and absence of secret logging.
- Username scoping and account-owned character lookup.
- Loopback-only credential enforcement.
- Multiple sessions for different characters of one account and rejection of
  a second active session for the same character.
- Protocol-v2 authentication encoding/decoding, invalid credential shapes,
  bounds, and protocol-v1 version rejection.
- Authentication before character listing, character selection before entry,
  content acceptance before world entry, bootstrap before readiness, and
  gameplay-intent rejection before readiness.
- Login-screen error handling, empty accounts, interactive character
  selection, and stale-world clearing.
- CLI password, password-stdin, token, token-file, unknown-character, and
  automatic-entry paths.
- Reconnect through the full authenticated session sequence and automatic
  re-selection only when `--character` was supplied.

Existing graphical and typed smoke scripts must stop depending on a shared
hard-coded credential. They must create their own temporary account stores and
use the same supported command-line credential flow as an end user.

## Explicit non-goals

This milestone does not provide:

- Public internet deployment or TLS.
- Production-grade account recovery, password reset, email verification, MFA,
  bans, moderation, or audit tooling.
- Client-side credential persistence or secret-store integration.
- In-client account registration or character creation.
- SSH public-key authentication, identity-file loading, or SSH-agent support.
- A production database-backed account repository, session resume, gateway
  failover, or multi-machine credential service.
